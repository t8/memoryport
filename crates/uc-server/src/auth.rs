use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;

use crate::db::hash_api_key;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    #[allow(dead_code)]
    pub key_id: String,
}

/// Auth middleware for API endpoints. Validates Bearer token.
/// If no admin_api_key is configured (local dev mode), allows unauthenticated
/// access with a default user identity.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Check for auth header
    let api_key = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    // If no auth header and no admin key configured → local dev mode (allow through)
    if api_key.is_none() && state.server_config.admin_api_key.is_none() {
        request.extensions_mut().insert(AuthenticatedUser {
            user_id: "local".into(),
            key_id: "local".into(),
        });
        return Ok(next.run(request).await);
    }

    let api_key = api_key.ok_or(StatusCode::UNAUTHORIZED)?;

    if !api_key.starts_with("uc_") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let key_hash = hash_api_key(api_key);
    let user = state
        .user_db
        .lookup_by_key_hash(&key_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Rate limit check
    if !state.rate_limiter.check(&user.user_id) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

/// Admin auth middleware. Validates against configured admin API key.
pub async fn admin_auth_middleware(
    State(state): State<Arc<AppState>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let admin_key = state
        .server_config
        .admin_api_key
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?; // admin endpoints disabled if no key configured

    let provided = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if provided != admin_key {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}
