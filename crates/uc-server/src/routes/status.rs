use axum::extract::State;
use axum::Extension;
use axum::Json;
use std::sync::Arc;

use crate::auth::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::StatusResponse;
use crate::state::AppState;

pub async fn status(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<StatusResponse>, ApiError> {
    let engine = state.pool.get_or_create(&user.user_id).await?;

    let s = engine.status().await?;

    Ok(Json(StatusResponse {
        pending_chunks: s.pending_chunks,
        indexed_chunks: s.indexed_chunks,
        index_path: s.index_path,
        embedding_model: s.embedding_model,
        embedding_dimensions: s.embedding_dimensions,
    }))
}

pub async fn compact(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let engine = state.pool.get_or_create(&user.user_id).await?;
    engine.optimize().await?;
    Ok(Json(serde_json::json!({ "status": "compaction complete" })))
}
