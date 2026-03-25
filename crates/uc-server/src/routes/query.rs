use axum::extract::State;
use axum::Extension;
use axum::Json;
use std::sync::Arc;

use crate::auth::AuthenticatedUser;
use crate::error::ApiError;
use crate::models::{QueryRequest, QueryResponse};
use crate::state::AppState;

pub async fn query(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, ApiError> {
    let engine = state.pool.get_or_create(&user.user_id).await?;

    let context = engine
        .query(&req.query, &user.user_id, req.session_id.as_deref(), req.max_tokens)
        .await?;

    Ok(Json(QueryResponse {
        context: context.formatted,
        token_count: context.token_count,
        chunks_included: context.chunks_included,
    }))
}
