use axum::extract::State;
use axum::Extension;
use axum::Json;
use chrono::{TimeZone, Utc};
use std::sync::Arc;

use crate::auth::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::{RetrieveRequest, RetrieveResponse, RetrieveResult};
use crate::state::AppState;

pub async fn retrieve(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<RetrieveRequest>,
) -> Result<Json<RetrieveResponse>, ApiError> {
    let engine = state.pool.get_or_create(&user.user_id).await?;

    // Use direct search (bypasses gating) since the user explicitly asked to search
    let results = engine
        .search(&req.query, &user.user_id, req.top_k, req.reference_time)
        .await?;

    let results: Vec<RetrieveResult> = results
        .into_iter()
        .take(req.top_k)
        .map(|r| {
            // Optionally prepend date to content for LLM consumers.
            // Dashboard/UI should pass include_dates=false (the default).
            let content = if req.include_dates && r.timestamp > 0 {
                if let Some(dt) = Utc.timestamp_millis_opt(r.timestamp).single() {
                    format!("[{}] {}", dt.format("%B %d, %Y"), r.content)
                } else {
                    r.content
                }
            } else {
                r.content
            };
            RetrieveResult {
                chunk_id: r.chunk_id,
                session_id: r.session_id,
                chunk_type: r.chunk_type.as_str().to_string(),
                role: r.role.map(|r| r.as_str().to_string()),
                score: r.score,
                timestamp: r.timestamp,
                content,
                arweave_tx_id: r.arweave_tx_id,
            }
        })
        .collect();

    Ok(Json(RetrieveResponse { results }))
}
