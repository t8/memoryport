use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;
use std::sync::Arc;

use crate::auth::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::*;
use crate::state::AppState;

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<SessionListResponse>, ApiError> {
    let engine = state.pool.get_or_create(&user.user_id).await?;

    let sessions = engine.list_sessions(&user.user_id).await?;

    Ok(Json(SessionListResponse {
        sessions: sessions
            .into_iter()
            .map(|s| SessionInfo {
                session_id: s.session_id,
                chunk_count: s.chunk_count,
                first_timestamp: s.first_timestamp,
                last_timestamp: s.last_timestamp,
            })
            .collect(),
    }))
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionDetailResponse>, ApiError> {
    let engine = state.pool.get_or_create(&user.user_id).await?;

    let chunks = engine.get_session(&user.user_id, &session_id).await?;

    Ok(Json(SessionDetailResponse {
        session_id,
        chunks: chunks
            .into_iter()
            .map(|c| SessionChunk {
                chunk_id: c.chunk_id,
                role: c.role.map(|r| r.as_str().to_string()),
                content: c.content,
                timestamp: c.timestamp,
                source_integration: c.source_integration,
                source_model: c.source_model,
            })
            .collect(),
    }))
}
