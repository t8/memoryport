use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::routes::ProxyState;

const ANTHROPIC_UPSTREAM: &str = "https://api.anthropic.com";

/// POST /v1/messages — Anthropic Messages API proxy with context injection + auto-capture.
/// Uses raw JSON manipulation to avoid deserializing/reserializing content blocks
/// (which can have types we don't model, causing "Input tag 'Other'" errors).
pub async fn proxy_messages(
    State(state): State<Arc<ProxyState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, StatusCode> {
    // Parse as raw JSON value — preserves all fields exactly
    let mut request: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        warn!(error = %e, "failed to parse request body");
        StatusCode::BAD_REQUEST
    })?;

    // 1. Extract the last user message text
    let last_user_msg = extract_last_user_text(&request);

    if last_user_msg.is_empty() {
        return forward_raw(&state, &headers, &body).await;
    }

    debug!(query_len = last_user_msg.len(), "extracting context for Anthropic message");

    let clean_query = sanitize_query(&last_user_msg);

    // 2. Search for relevant context
    let context = match state.engine.search(&clean_query, &state.user_id, 20).await {
        Ok(ref results) => {
            let clean: Vec<_> = results
                .iter()
                .filter(|r| !is_system_prompt_leak(&r.content))
                .cloned()
                .collect();
            eprintln!(
                "[memoryport-proxy] search returned {} results ({} after filtering)",
                results.len(),
                clean.len()
            );
            if clean.is_empty() {
                None
            } else {
                Some(uc_core::assembler::assemble_context(&clean, state.context_budget))
            }
        }
        Err(ref e) => {
            eprintln!("[memoryport-proxy] search error: {e}");
            None
        }
    };

    // 3. Inject context by appending to the last user message in the raw JSON
    if let Some(ref ctx) = context {
        if ctx.chunks_included > 0 {
            eprintln!(
                "[memoryport-proxy] injecting {} chunks ({} tokens)",
                ctx.chunks_included, ctx.token_count
            );

            let plain_context = format_plain_context(&ctx.formatted);

            let suffix = format!(
                "\n\nFor additional context, here is information from my previous conversations that is relevant to my question:\n\n{}\n\nPlease use the above context to answer my question.",
                plain_context
            );

            append_to_last_user_message(&mut request, &suffix);
        }
    }

    // DEBUG: dump request
    if let Ok(json) = serde_json::to_string_pretty(&request) {
        let _ = std::fs::write("/tmp/memoryport_last_request.json", &json);
    }

    // 4. Forward the modified raw JSON
    let modified_body = serde_json::to_vec(&request).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = forward_raw(&state, &headers, &modified_body).await?;

    // 5. Store user message (async, sanitized)
    let model = request
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown")
        .to_string();
    {
        let engine = state.engine.clone();
        let user_id = state.user_id.clone();
        let session_id = state.session_id.clone();
        let msg = last_user_msg.clone();
        tokio::spawn(async move {
            let msg = sanitize_for_storage(&msg);
            if msg.len() < 10 {
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

/// Extract text from the last user message, handling both string and content-block formats.
fn extract_last_user_text(request: &serde_json::Value) -> String {
    let messages = match request.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return String::new(),
    };

    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        let content = match msg.get("content") {
            Some(c) => c,
            None => continue,
        };

        // String content
        if let Some(s) = content.as_str() {
            return s.to_string();
        }

        // Array of content blocks — extract text blocks
        if let Some(blocks) = content.as_array() {
            let texts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| {
                    if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                        b.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if !texts.is_empty() {
                return texts.join("\n");
            }
        }
    }

    String::new()
}

/// Append text to the last user message in the raw JSON.
fn append_to_last_user_message(request: &mut serde_json::Value, suffix: &str) {
    let messages = match request.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(m) => m,
        None => return,
    };

    // Find last user message (iterate in reverse)
    for msg in messages.iter_mut().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        let content = match msg.get_mut("content") {
            Some(c) => c,
            None => continue,
        };

        // String content — just append
        if let Some(s) = content.as_str().map(|s| s.to_string()) {
            *content = serde_json::Value::String(format!("{}{}", s, suffix));
            return;
        }

        // Array of content blocks — append a new text block
        if let Some(blocks) = content.as_array_mut() {
            blocks.push(serde_json::json!({
                "type": "text",
                "text": suffix
            }));
            return;
        }

        break;
    }
}

/// Forward raw bytes to Anthropic, passing through auth headers.
async fn forward_raw(
    state: &ProxyState,
    original_headers: &HeaderMap,
    body: &[u8],
) -> Result<Response, StatusCode> {
    let upstream = format!("{}/v1/messages", ANTHROPIC_UPSTREAM);

    let mut req_builder = state
        .http
        .post(&upstream)
        .header("content-type", "application/json")
        .body(body.to_vec());

    // Forward all auth + anthropic headers
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

    // Store assistant response (best-effort)
    // Handle both non-streaming (JSON) and streaming (SSE) responses
    if status.is_success() {
        let (assistant_text, model) = if let Ok(resp_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            // Non-streaming: parse JSON response
            let text = extract_assistant_text(&resp_json);
            let model = resp_json.get("model").and_then(|m| m.as_str()).unwrap_or("unknown").to_string();
            (text, model)
        } else if let Ok(body_str) = std::str::from_utf8(&body_bytes) {
            // Streaming: parse SSE events to extract text deltas
            let text = extract_text_from_sse(body_str);
            let model = extract_model_from_sse(body_str);
            (text, model)
        } else {
            (String::new(), "unknown".into())
        };

        let assistant_text = sanitize_for_storage(&assistant_text);
        if assistant_text.len() >= 10 {
            let engine = state.engine.clone();
            let user_id = state.user_id.clone();
            let session_id = state.session_id.clone();
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
                    eprintln!("[memoryport-proxy] store assistant FAILED: {e}");
                }
                let _ = engine.flush().await;
            });
        }
    }

    let mut response = (status, body_bytes).into_response();
    if let Some(ct) = resp_headers.get("content-type") {
        response.headers_mut().insert("content-type", ct.clone());
    }

    Ok(response)
}

/// Extract text from an Anthropic response JSON.
fn extract_assistant_text(response: &serde_json::Value) -> String {
    response
        .get("content")
        .and_then(|c| c.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| {
                    if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                        b.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Extract accumulated text from SSE streaming response.
/// SSE format: lines starting with "data: " containing JSON events.
/// Text deltas are in content_block_delta events with type "text_delta".
fn extract_text_from_sse(sse_body: &str) -> String {
    let mut text = String::new();
    for line in sse_body.lines() {
        let data = match line.strip_prefix("data: ") {
            Some(d) => d.trim(),
            None => continue,
        };
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
            // content_block_delta with text_delta
            if event.get("type").and_then(|t| t.as_str()) == Some("content_block_delta") {
                if let Some(delta) = event.get("delta") {
                    if delta.get("type").and_then(|t| t.as_str()) == Some("text_delta") {
                        if let Some(t) = delta.get("text").and_then(|t| t.as_str()) {
                            text.push_str(t);
                        }
                    }
                }
            }
        }
    }
    text
}

/// Extract model name from SSE streaming response (from message_start event).
fn extract_model_from_sse(sse_body: &str) -> String {
    for line in sse_body.lines() {
        let data = match line.strip_prefix("data: ") {
            Some(d) => d.trim(),
            None => continue,
        };
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
            if event.get("type").and_then(|t| t.as_str()) == Some("message_start") {
                if let Some(model) = event
                    .get("message")
                    .and_then(|m| m.get("model"))
                    .and_then(|m| m.as_str())
                {
                    return model.to_string();
                }
            }
        }
    }
    "unknown".into()
}

// Public wrappers for shared helpers (used by routes.rs)
pub fn sanitize_query_pub(query: &str) -> String { sanitize_query(query) }
pub fn is_system_prompt_leak_pub(content: &str) -> bool { is_system_prompt_leak(content) }
pub fn sanitize_for_storage_pub(content: &str) -> String { sanitize_for_storage(content) }

pub fn format_plain_context(formatted: &str) -> String {
    formatted
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
        .join("\n")
}

fn sanitize_query(query: &str) -> String {
    let clean = if let Some(idx) = query.find("<system-reminder") {
        &query[..idx]
    } else {
        query
    };
    clean.trim().to_string()
}

fn is_system_prompt_leak(content: &str) -> bool {
    let markers = [
        "<system-reminder>",
        "</system-reminder>",
        "system-reminder",
        "IMPORTANT: this context may or may not be relevant",
        "You should not respond to this context",
        "auto-memory, persists across conversations",
        "Contents of /Users/",
        "<local-command-caveat>",
        "<command-name>",
        "<command-message>",
        "<local-command-stdout>",
    ];
    markers.iter().any(|m| content.contains(m))
}

/// Check if a message is an internal command or meta-request (not user conversation).
fn is_internal_command(content: &str) -> bool {
    let markers = [
        // Claude Code internal commands
        "<local-command-caveat>",
        "<command-name>",
        "<command-message>",
        "<command-args>",
        "<local-command-stdout>",
        "/model",
        "/help",
        "/clear",
        "/compact",
        "/config",
        // Open WebUI title/tag generation
        "\"title\":",
        "\"tags\":",
        "\"follow_ups\":",
        "Generate a concise",
        "generate a title",
        "### Task:",
        "JSON format",
    ];
    markers.iter().any(|m| content.contains(m))
}

fn sanitize_for_storage(content: &str) -> String {
    // Skip internal commands entirely
    if is_internal_command(content) {
        return String::new();
    }

    let clean = if let Some(idx) = content.find("<system-reminder") {
        &content[..idx]
    } else if let Some(idx) = content.find("</system-reminder") {
        &content[..idx]
    } else {
        content
    };
    let clean = if let Some(idx) = clean.find("Contents of /Users/") {
        &clean[..idx]
    } else {
        clean
    };
    // Also strip local-command blocks
    let clean = if let Some(idx) = clean.find("<local-command-caveat>") {
        &clean[..idx]
    } else {
        clean
    };
    clean.trim().to_string()
}
