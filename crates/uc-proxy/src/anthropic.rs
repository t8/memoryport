use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::models::{AnthropicRequest, AnthropicResponse, AnthropicSystem};
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

    // 2. Run retrieval pipeline
    let context = state
        .engine
        .query(
            &last_user_msg,
            &state.user_id,
            Some(&state.session_id),
            state.context_budget,
        )
        .await
        .map_err(|e| {
            warn!(error = %e, "context retrieval failed, forwarding without context");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 3. Inject context into the system field
    if context.chunks_included > 0 {
        info!(
            chunks = context.chunks_included,
            tokens = context.token_count,
            "injecting context into Anthropic request"
        );

        let context_text = format!(
            "The following is relevant context from the user's stored memory:\n\n{}",
            context.formatted
        );

        match request.system {
            Some(ref mut system) => system.prepend_text(&context_text),
            None => request.system = Some(AnthropicSystem::Text(context_text)),
        }
    }

    // 4. Forward to Anthropic
    let response = forward_anthropic(&state, &headers, &request).await?;

    // 5. Store user message (async)
    {
        let engine = state.engine.clone();
        let user_id = state.user_id.clone();
        let session_id = state.session_id.clone();
        let msg = last_user_msg.clone();
        tokio::spawn(async move {
            let params = uc_core::models::StoreParams {
                user_id,
                session_id,
                chunk_type: uc_core::models::ChunkType::Conversation,
                role: Some(uc_core::models::Role::User),
            };
            if let Err(e) = engine.store(&msg, params).await {
                warn!(error = %e, "failed to store user message");
            }
            let _ = engine.flush().await;
        });
    }

    Ok(response)
}

async fn forward_anthropic(
    state: &ProxyState,
    original_headers: &HeaderMap,
    request: &AnthropicRequest,
) -> Result<Response, StatusCode> {
    let upstream = format!("{}/v1/messages", ANTHROPIC_UPSTREAM);

    let mut req_builder = state.http.post(&upstream).json(request);

    // Forward authentication and Anthropic-specific headers
    for header_name in &[
        "x-api-key",
        "anthropic-version",
        "anthropic-beta",
        "anthropic-dangerous-direct-browser-access",
    ] {
        if let Some(val) = original_headers.get(*header_name) {
            req_builder = req_builder.header(*header_name, val);
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
                tokio::spawn(async move {
                    let params = uc_core::models::StoreParams {
                        user_id,
                        session_id,
                        chunk_type: uc_core::models::ChunkType::Conversation,
                        role: Some(uc_core::models::Role::Assistant),
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
