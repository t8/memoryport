use axum::extract::State;
use axum::Extension;
use axum::Json;
use std::sync::Arc;

use crate::auth::AuthenticatedUser;
use crate::error::ApiError;
use crate::state::AppState;

pub async fn analytics(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<uc_core::analytics::AnalyticsData>, ApiError> {
    let engine = state.pool.get_or_create(&user.user_id).await?;
    let data = engine.analytics(&user.user_id).await?;
    Ok(Json(data))
}
