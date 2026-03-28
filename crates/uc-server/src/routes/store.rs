use axum::extract::State;
use axum::Extension;
use axum::Json;
use std::sync::Arc;

use crate::auth::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::{StoreRequest, StoreResponse};
use crate::state::AppState;

pub async fn store(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<StoreRequest>,
) -> Result<Json<StoreResponse>, ApiError> {
    let engine = state.pool.get_or_create(&user.user_id).await?;

    let chunk_type = req.chunk_type.parse().unwrap_or(uc_core::models::ChunkType::Conversation);
    let role = req.role.as_deref().and_then(|r| r.parse().ok());

    let params = uc_core::models::StoreParams {
        user_id: user.user_id.clone(),
        session_id: req.session_id,
        chunk_type,
        role,
        source_integration: Some("api".into()),
        source_model: None,
        timestamp: req.timestamp,
    };

    let ids = engine.store(&req.text, params).await?;
    engine.flush().await?;

    Ok(Json(StoreResponse {
        chunks_stored: ids.len(),
        chunk_ids: ids.iter().map(|id| id.to_string()).collect(),
    }))
}
