use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;

use crate::state::AppState;

fn extract_bearer(req: &Request) -> Option<&str> {
    req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

pub async fn require_api_key(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    match extract_bearer(&req) {
        Some(key) if state.is_valid_api_key(key) => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "message": "Invalid or missing API key",
                    "type": "invalid_request_error",
                    "code": "invalid_api_key"
                }
            })),
        )
            .into_response(),
    }
}

pub async fn require_internal_key(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    match extract_bearer(&req) {
        Some(key) if state.is_valid_internal_key(key) => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "message": "Invalid internal key",
                    "type": "authentication_error"
                }
            })),
        )
            .into_response(),
    }
}
