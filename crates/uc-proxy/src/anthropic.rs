use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::models::{AnthropicContent, AnthropicMessage, AnthropicRequest, AnthropicResponse};
use crate::routes::ProxyState;

const ANTHROPIC_UPSTREAM: &str = "https://api.anthropic.com";

/// POST /v1/messages — Anthropic Messages API proxy with context injection + auto-capture.
pub async fn proxy_messages(
    State(state): State<Arc<ProxyState>>,
    headers: HeaderMap,
    Json(mut request): Json<AnthropicRequest>,
) -> Result<Response, StatusCode> {
    // 1. Extract the last user message
    let last_user_msg = request
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_text())
        .unwrap_or_default();

    if last_user_msg.is_empty() {
        return forward_anthropic(&state, &headers, &request).await;
    }

    debug!(query_len = last_user_msg.len(), "extracting context for Anthropic message");

    // Strip system-reminder tags from the query to avoid searching for system prompt content
    let clean_query = sanitize_query(&last_user_msg);

    // 2. Search for relevant context (bypasses gating — proxy always searches)
    let search_results = state
        .engine
        .search(&clean_query, &state.user_id, 20)
        .await;

    let context = match search_results {
        Ok(ref results) => {
            // Filter out chunks that contain system prompt artifacts
            let clean_results: Vec<_> = results
                .iter()
                .filter(|r| !is_system_prompt_leak(&r.content))
                .cloned()
                .collect();
            eprintln!("[memoryport-proxy] search returned {} results ({} after filtering) for query len={}", results.len(), clean_results.len(), clean_query.len());
            if clean_results.is_empty() {
                None
            } else {
                Some(uc_core::assembler::assemble_context(&clean_results, state.context_budget))
            }
        }
        Err(ref e) => {
            eprintln!("[memoryport-proxy] search error: {e}");
            None
        }
    };

    // 3. Inject context directly into the last user message.
    // Appending to the user's actual message is the most reliable injection
    // method — it can't be overridden by system prompts or memory systems.
    if let Some(ref ctx) = context {
        eprintln!("[memoryport-proxy] injecting {} chunks ({} tokens)", ctx.chunks_included, ctx.token_count);
        if ctx.chunks_included > 0 {
            // Format as plain text — no XML tags that could trigger prompt injection filtering
            let plain_context = ctx.formatted
                .replace("<unlimited_context>", "")
                .replace("</unlimited_context>", "")
                .replace("<session ", "Session ")
                .replace("</session>", "")
                .replace("<turn ", "")
                .replace("</turn>", "")
                .replace("<document ", "Document ")
                .replace("</document>", "")
                .replace("<knowledge ", "Knowledge ")
                .replace("</knowledge>", "")
                .replace(">", ": ")
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");

            let context_suffix = format!(
                "\n\nFor additional context, here is information from my previous conversations that is relevant to my question:\n\n{}\n\nPlease use the above context to answer my question.",
                plain_context
            );

            // Append to the last user message's content
            if let Some(last_user) = request.messages.iter_mut().rev().find(|m| m.role == "user") {
                match &mut last_user.content {
                    AnthropicContent::Text(ref mut text) => {
                        text.push_str(&context_suffix);
                    }
                    AnthropicContent::Blocks(ref mut blocks) => {
                        blocks.push(crate::models::AnthropicContentBlock::Text {
                            text: context_suffix,
                        });
                    }
                }
            }
        }
    }

    // DEBUG: dump the modified request to see exactly what Claude receives
    if let Ok(json) = serde_json::to_string_pretty(&request) {
        let _ = std::fs::write("/tmp/memoryport_last_request.json", &json);
        eprintln!("[memoryport-proxy] full request dumped to /tmp/memoryport_last_request.json ({} bytes)", json.len());
    }

    // 4. Forward to Anthropic
    let response = forward_anthropic(&state, &headers, &request).await?;

    // 5. Store user message (async)
    {
        let engine = state.engine.clone();
        let user_id = state.user_id.clone();
        let session_id = state.session_id.clone();
        let msg = last_user_msg.clone();
        let model = request.model.clone();
        tokio::spawn(async move {
            // Sanitize before storing — strip system prompt content
            let msg = sanitize_for_storage(&msg);
            if msg.is_empty() {
                eprintln!("[memoryport-proxy] skipping empty message after sanitization");
                return;
            }
            let params = uc_core::models::StoreParams {
                user_id: user_id.clone(),
                session_id: session_id.clone(),
                chunk_type: uc_core::models::ChunkType::Conversation,
                role: Some(uc_core::models::Role::User),
                source_integration: Some("proxy".into()),
                source_model: Some(model),
            };
            eprintln!("[memoryport-proxy] storing user message ({} chars) for user={} session={}", msg.len(), user_id, session_id);
            if let Err(e) = engine.store(&msg, params).await {
                eprintln!("[memoryport-proxy] store FAILED: {e}");
            }
            if let Err(e) = engine.flush().await {
                eprintln!("[memoryport-proxy] flush FAILED: {e}");
            }
        });
    }

    Ok(response)
}

/// Strip system prompt artifacts from a query before searching.
fn sanitize_query(query: &str) -> String {
    // Remove everything after <system-reminder or similar tags
    let clean = if let Some(idx) = query.find("<system-reminder") {
        &query[..idx]
    } else {
        query
    };
    clean.trim().to_string()
}

/// Check if a chunk contains system prompt content that leaked through.
fn is_system_prompt_leak(content: &str) -> bool {
    let markers = [
        "<system-reminder>",
        "</system-reminder>",
        "system-reminder",
        "IMPORTANT: this context may or may not be relevant",
        "You should not respond to this context",
        "auto-memory, persists across conversations",
        "Contents of /Users/",
    ];
    markers.iter().any(|m| content.contains(m))
}

/// Clean content before storing — remove system prompt fragments.
fn sanitize_for_storage(content: &str) -> String {
    // Cut off at any system-reminder tag
    let clean = if let Some(idx) = content.find("<system-reminder") {
        &content[..idx]
    } else if let Some(idx) = content.find("</system-reminder") {
        &content[..idx]
    } else {
        content
    };

    // Also cut at "Contents of /Users/" which is memory file dumps
    let clean = if let Some(idx) = clean.find("Contents of /Users/") {
        &clean[..idx]
    } else {
        clean
    };

    clean.trim().to_string()
}

async fn forward_anthropic(
    state: &ProxyState,
    original_headers: &HeaderMap,
    request: &AnthropicRequest,
) -> Result<Response, StatusCode> {
    let upstream = format!("{}/v1/messages", ANTHROPIC_UPSTREAM);

    let mut req_builder = state.http.post(&upstream).json(request);

    // Forward all authentication and Anthropic-specific headers.
    // Claude Code sends "authorization: Bearer ..." while direct API clients
    // may send "x-api-key". Forward both, plus all anthropic-* headers.
    for (name, value) in original_headers.iter() {
        let name_str = name.as_str();
        if name_str == "authorization"
            || name_str == "x-api-key"
            || name_str.starts_with("anthropic-")
        {
            req_builder = req_builder.header(name, value);
        }
    }

    let upstream_resp = req_builder.send().await.map_err(|e| {
        warn!(error = %e, "failed to forward to Anthropic");
        StatusCode::BAD_GATEWAY
    })?;

    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let resp_headers = upstream_resp.headers().clone();
    let body_bytes = upstream_resp.bytes().await.map_err(|e| {
        warn!(error = %e, "failed to read Anthropic response");
        StatusCode::BAD_GATEWAY
    })?;

    // Try to extract and store assistant response (best-effort)
    if status.is_success() {
        if let Ok(anthropic_resp) = serde_json::from_slice::<AnthropicResponse>(&body_bytes) {
            let assistant_text = anthropic_resp.text();
            if !assistant_text.is_empty() {
                let engine = state.engine.clone();
                let user_id = state.user_id.clone();
                let session_id = state.session_id.clone();
                let model = request.model.clone();
                tokio::spawn(async move {
                    let params = uc_core::models::StoreParams {
                        user_id,
                        session_id,
                        chunk_type: uc_core::models::ChunkType::Conversation,
                        role: Some(uc_core::models::Role::Assistant),
                        source_integration: Some("proxy".into()),
                        source_model: Some(model),
                    };
                    if let Err(e) = engine.store(&assistant_text, params).await {
                        warn!(error = %e, "failed to store assistant response");
                    }
                    let _ = engine.flush().await;
                });
            }
        }
    }

    let mut response = (status, body_bytes).into_response();
    if let Some(ct) = resp_headers.get("content-type") {
        response.headers_mut().insert("content-type", ct.clone());
    }

    Ok(response)
}
