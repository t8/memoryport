use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::models::{ChatCompletionsRequest, Message};

pub struct ProxyState {
    pub engine: Arc<uc_core::Engine>,
    pub upstream: String,
    pub http: reqwest::Client,
    pub user_id: String,
    pub session_id: String,
    pub context_budget: u32,
}

/// POST /v1/chat/completions — intercept, inject context, forward, store response.
pub async fn proxy_completions(
    State(state): State<Arc<ProxyState>>,
    Json(mut request): Json<ChatCompletionsRequest>,
) -> Result<Response, StatusCode> {
    // 1. Extract the last user message
    let last_user_msg = request
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_default();

    if last_user_msg.is_empty() {
        // No user message — forward as-is
        return forward_request(&state, &request).await;
    }

    debug!(query = %last_user_msg, "extracting context for user message");

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

    // 3. Inject context as a system message at the beginning
    if context.chunks_included > 0 {
        info!(
            chunks = context.chunks_included,
            tokens = context.token_count,
            "injecting context"
        );

        let context_msg = Message {
            role: "system".into(),
            content: format!(
                "The following is relevant context from the user's stored memory:\n\n{}",
                context.formatted
            ),
        };

        // Insert after any existing system messages
        let insert_pos = request
            .messages
            .iter()
            .position(|m| m.role != "system")
            .unwrap_or(0);
        request.messages.insert(insert_pos, context_msg);
    }

    // 4. Forward to upstream
    let response = forward_request(&state, &request).await?;

    // 5. Store the user message (async, don't block response)
    let engine = state.engine.clone();
    let user_id = state.user_id.clone();
    let session_id = state.session_id.clone();
    let user_msg = last_user_msg.clone();
    tokio::spawn(async move {
        let params = uc_core::models::StoreParams {
            user_id,
            session_id,
            chunk_type: uc_core::models::ChunkType::Conversation,
            role: Some(uc_core::models::Role::User),
        };
        if let Err(e) = engine.store(&user_msg, params).await {
            warn!(error = %e, "failed to store user message");
        }
    });

    Ok(response)
}

async fn forward_request(
    state: &ProxyState,
    request: &ChatCompletionsRequest,
) -> Result<Response, StatusCode> {
    let url = format!("{}/v1/chat/completions", state.upstream);

    let upstream_resp = state
        .http
        .post(&url)
        .json(request)
        .send()
        .await
        .map_err(|e| {
            warn!(error = %e, "failed to forward to upstream");
            StatusCode::BAD_GATEWAY
        })?;

    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let headers = upstream_resp.headers().clone();
    let body = upstream_resp.bytes().await.map_err(|e| {
        warn!(error = %e, "failed to read upstream response");
        StatusCode::BAD_GATEWAY
    })?;

    let mut response = (status, body).into_response();
    // Copy content-type from upstream
    if let Some(ct) = headers.get("content-type") {
        response
            .headers_mut()
            .insert("content-type", ct.clone());
    }

    Ok(response)
}

/// Health check endpoint.
pub async fn health() -> &'static str {
    "ok"
}
