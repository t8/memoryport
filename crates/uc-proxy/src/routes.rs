use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use tracing::{debug, warn};

pub struct ProxyState {
    pub engine: Arc<uc_core::Engine>,
    pub http: reqwest::Client,
    pub user_id: String,
    pub session_id: String,
    pub context_budget: u32,
}

/// Detect upstream from model name in request.
fn detect_upstream(request: &serde_json::Value) -> &'static str {
    let model = request
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("");

    if model.starts_with("gpt-") || model.starts_with("o1") || model.starts_with("o3") || model.starts_with("text-") {
        "https://api.openai.com"
    } else if model.starts_with("llama")
        || model.starts_with("mistral")
        || model.starts_with("codellama")
        || model.starts_with("gemma")
        || model.starts_with("phi")
        || model.starts_with("qwen")
        || model.starts_with("deepseek")
        || model.starts_with("nomic")
        || model.contains(":")
    {
        // Ollama models — check if intercept is active (Ollama moved to 11435)
        let marker = dirs::home_dir()
            .unwrap_or_default()
            .join(".memoryport")
            .join("ollama-intercept.active");
        if marker.exists() {
            "http://localhost:11435"
        } else {
            "http://localhost:11434"
        }
    } else {
        // Default to OpenAI
        "https://api.openai.com"
    }
}

/// POST /v1/chat/completions — OpenAI-compatible proxy with auto-routing.
/// Routes to OpenAI, Ollama, or any OpenAI-compatible endpoint based on model name.
pub async fn proxy_completions(
    State(state): State<Arc<ProxyState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, StatusCode> {
    let mut request: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        warn!(error = %e, "failed to parse request body");
        StatusCode::BAD_REQUEST
    })?;

    // Detect upstream
    let upstream = detect_upstream(&request);

    // Extract last user message
    let last_user_msg = extract_last_user_text(&request);

    if last_user_msg.is_empty() {
        return forward_openai_raw(&state, &headers, &body, upstream).await;
    }

    debug!(query_len = last_user_msg.len(), upstream = upstream, "processing OpenAI-format request");

    let clean_query = crate::anthropic::sanitize_query_pub(&last_user_msg);

    // Search for context (best-effort)
    let context = match state.engine.search(&clean_query, &state.user_id, 20).await {
        Ok(ref results) => {
            let clean: Vec<_> = results
                .iter()
                .filter(|r| !crate::anthropic::is_system_prompt_leak_pub(&r.content))
                .cloned()
                .collect();
            if clean.is_empty() {
                None
            } else {
                Some(uc_core::assembler::assemble_context(&clean, state.context_budget))
            }
        }
        Err(e) => {
            warn!(error = %e, "context retrieval failed");
            None
        }
    };

    // Inject context into last user message
    if let Some(ref ctx) = context {
        if ctx.chunks_included > 0 {
            let plain_context = crate::anthropic::format_plain_context(&ctx.formatted);
            let suffix = format!(
                "\n\nFor additional context, here is information from my previous conversations:\n\n{}\n\nPlease use the above context to answer my question.",
                plain_context
            );
            append_to_last_user_message(&mut request, &suffix);
        }
    }

    // Forward
    let modified_body = serde_json::to_vec(&request).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = forward_openai_raw(&state, &headers, &modified_body, upstream).await?;

    // Store user message (async)
    let model = request.get("model").and_then(|m| m.as_str()).unwrap_or("unknown").to_string();
    {
        let engine = state.engine.clone();
        let user_id = state.user_id.clone();
        let session_id = state.session_id.clone();
        let msg = last_user_msg;
        tokio::spawn(async move {
            let msg = crate::anthropic::sanitize_for_storage_pub(&msg);
            if msg.len() < 10 { return; }
            let params = uc_core::models::StoreParams {
                user_id,
                session_id,
                chunk_type: uc_core::models::ChunkType::Conversation,
                role: Some(uc_core::models::Role::User),
                source_integration: Some("proxy".into()),
                source_model: Some(model),
            };
            let _ = engine.store(&msg, params).await;
            let _ = engine.flush().await;
        });
    }

    Ok(response)
}

fn extract_last_user_text(request: &serde_json::Value) -> String {
    let messages = match request.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return String::new(),
    };
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if let Some(s) = msg.get("content").and_then(|c| c.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

fn append_to_last_user_message(request: &mut serde_json::Value, suffix: &str) {
    let messages = match request.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(m) => m,
        None => return,
    };
    for msg in messages.iter_mut().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if let Some(s) = msg.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()) {
            msg["content"] = serde_json::Value::String(format!("{}{}", s, suffix));
            return;
        }
        break;
    }
}

async fn forward_openai_raw(
    state: &ProxyState,
    original_headers: &HeaderMap,
    body: &[u8],
    upstream: &str,
) -> Result<Response, StatusCode> {
    let url = format!("{}/v1/chat/completions", upstream);

    let mut req_builder = state
        .http
        .post(&url)
        .header("content-type", "application/json")
        .body(body.to_vec());

    // Forward auth headers
    for (name, value) in original_headers.iter() {
        let n = name.as_str();
        if n == "authorization" || n == "x-api-key" {
            req_builder = req_builder.header(name, value);
        }
    }

    let upstream_resp = req_builder.send().await.map_err(|e| {
        warn!(error = %e, upstream = upstream, "failed to forward to upstream");
        StatusCode::BAD_GATEWAY
    })?;

    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let resp_headers = upstream_resp.headers().clone();
    let body_bytes = upstream_resp.bytes().await.map_err(|e| {
        warn!(error = %e, "failed to read upstream response");
        StatusCode::BAD_GATEWAY
    })?;

    // Store assistant response (best-effort)
    if status.is_success() {
        if let Ok(resp_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            let assistant_text = resp_json
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let assistant_text = crate::anthropic::sanitize_for_storage_pub(&assistant_text);
            if assistant_text.len() >= 10 {
                let engine = state.engine.clone();
                let user_id = state.user_id.clone();
                let session_id = state.session_id.clone();
                let model = resp_json
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                tokio::spawn(async move {
                    let params = uc_core::models::StoreParams {
                        user_id,
                        session_id,
                        chunk_type: uc_core::models::ChunkType::Conversation,
                        role: Some(uc_core::models::Role::Assistant),
                        source_integration: Some("proxy".into()),
                        source_model: Some(model),
                    };
                    let _ = engine.store(&assistant_text, params).await;
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

pub async fn health() -> &'static str {
    "ok"
}
