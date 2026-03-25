use axum::extract::{Path, State};
use axum::Json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::models::{CreateUserRequest, CreateUserResponse};
use crate::state::AppState;

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<CreateUserResponse>, ApiError> {
    let (user_id, api_key) = state
        .user_db
        .create_user(req.email.as_deref())
        .await
        .map_err(|e| ApiError::Database(e))?;

    Ok(Json(CreateUserResponse { user_id, api_key }))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state
        .user_db
        .delete_user(&user_id)
        .await
        .map_err(|e| ApiError::Database(e))?;

    if deleted {
        Ok(Json(serde_json::json!({ "deleted": true })))
    } else {
        Err(ApiError::NotFound("user not found".into()))
    }
}
