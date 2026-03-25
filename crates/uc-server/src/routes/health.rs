use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::models::{HealthResponse, ReadyChecks, ReadyResponse};
use crate::state::AppState;

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub async fn ready(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReadyResponse>, StatusCode> {
    let db_ok = state.user_db.ping().await.is_ok();

    if db_ok {
        Ok(Json(ReadyResponse {
            status: "ready",
            checks: ReadyChecks { database: db_ok },
        }))
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}
