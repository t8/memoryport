use axum::middleware;
use axum::routing::{delete, get, post};
use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use std::sync::Arc;
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::auth;
use crate::metrics;
use crate::routes;
use crate::state::AppState;

pub fn build_router(state: Arc<AppState>, metrics_handle: PrometheusHandle) -> Router {
    // Public routes (no auth)
    let public = Router::new()
        .route("/health", get(routes::health::health))
        .route("/ready", get(routes::health::ready).with_state(state.clone()))
        .route(
            "/metrics",
            get(metrics::metrics_handler).with_state(metrics_handle),
        );

    // Authenticated API routes
    let api = Router::new()
        .route("/store", post(routes::store::store))
        .route("/query", post(routes::query::query))
        .route("/retrieve", post(routes::retrieve::retrieve))
        .route("/sessions", get(routes::sessions::list_sessions))
        .route("/sessions/{id}", get(routes::sessions::get_session))
        .route("/status", get(routes::status::status))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .with_state(state.clone());

    // Admin routes (separate auth)
    let admin = Router::new()
        .route("/users", post(routes::admin::create_user))
        .route("/users/{id}", delete(routes::admin::delete_user))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::admin_auth_middleware,
        ))
        .with_state(state.clone());

    // Serve the React UI (fallback to index.html for SPA client-side routing)
    let ui_service = ServeDir::new("ui/dist")
        .not_found_service(ServeFile::new("ui/dist/index.html"));

    Router::new()
        .merge(public)
        .nest("/v1", api)
        .nest("/admin", admin)
        .fallback_service(ui_service)
        .layer(middleware::from_fn(metrics::metrics_middleware))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(state.server_config.request_timeout_secs),
        ))
        .layer(RequestBodyLimitLayer::new(
            state.server_config.request_body_limit,
        ))
        .layer(TraceLayer::new_for_http())
}
