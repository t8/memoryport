use axum::extract::State;
use axum::Extension;
use axum::Json;
use std::sync::Arc;

use crate::auth::AuthenticatedUser;
use crate::error::ApiError;
use crate::state::AppState;

pub async fn session_graph(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<uc_core::graph::GraphData>, ApiError> {
    let engine = state.pool.get_or_create(&user.user_id).await?;
    let data = engine.graph(&user.user_id).await?;
    Ok(Json(data))
}
