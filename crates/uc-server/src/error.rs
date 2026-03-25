use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum ApiError {
    #[error("engine error: {0}")]
    Engine(#[from] uc_core::EngineError),
    #[error("database error: {0}")]
    Database(String),
    #[error("invalid request: {0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("rate limited")]
    RateLimited,
    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Engine(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("engine error: {e}")),
            ApiError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database error".into()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate limited".into()),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
