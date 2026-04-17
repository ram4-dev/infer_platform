use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::state::AppState;

/// Injected into request extensions after successful API key validation.
#[derive(Clone, Debug)]
pub struct ValidatedKey {
    pub key_id: String,
    pub rate_limit_rpm: i64,
}

fn extract_bearer(req: &Request) -> Option<&str> {
    req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

pub fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

pub async fn require_api_key(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let key = match extract_bearer(&req) {
        Some(k) => k.to_string(),
        None => return unauthorized("Missing or invalid Authorization header"),
    };

    let validated = if let Some(ref pool) = state.db {
        validate_from_db(pool, &key).await
    } else {
        validate_from_memory(&state, &key)
    };

    let validated = match validated {
        Ok(Some(v)) => v,
        Ok(None) => return unauthorized("Invalid API key"),
        Err(e) => {
            tracing::error!("API key validation error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"message": "internal error", "type": "server_error"}})),
            )
                .into_response();
        }
    };

    if let Some(ref limiter_arc) = state.rate_limiter {
        let mut limiter = limiter_arc.lock().await;
        match limiter
            .check_and_increment(&validated.key_id, validated.rate_limit_rpm)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({
                        "error": {
                            "message": "Rate limit exceeded",
                            "type": "rate_limit_error",
                            "code": "rate_limit_exceeded"
                        }
                    })),
                )
                    .into_response();
            }
            Err(e) => {
                // Fail open — Redis unavailability should not block inference.
                tracing::warn!("Redis rate limit check failed (failing open): {e}");
            }
        }
    }

    req.extensions_mut().insert(validated);
    next.run(req).await
}

async fn validate_from_db(
    pool: &sqlx::PgPool,
    key: &str,
) -> anyhow::Result<Option<ValidatedKey>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        rate_limit_rpm: i32,
    }

    let hash = hash_key(key);
    let row = sqlx::query_as::<_, Row>(
        "SELECT id, rate_limit_rpm FROM api_keys \
         WHERE key_hash = $1 AND revoked_at IS NULL",
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ValidatedKey {
        key_id: r.id,
        rate_limit_rpm: r.rate_limit_rpm as i64,
    }))
}

fn validate_from_memory(state: &AppState, key: &str) -> anyhow::Result<Option<ValidatedKey>> {
    if state.api_keys.contains(key) {
        Ok(Some(ValidatedKey {
            key_id: key.to_string(),
            rate_limit_rpm: 60,
        }))
    } else {
        Ok(None)
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

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": {
                "message": message,
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        })),
    )
        .into_response()
}
