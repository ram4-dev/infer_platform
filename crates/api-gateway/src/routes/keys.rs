use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateKeyRequest {
    pub owner: String,
    #[serde(default = "default_rpm")]
    pub rate_limit_rpm: i32,
    pub daily_spend_cap_cents: Option<i32>,
}

fn default_rpm() -> i32 {
    60
}

#[derive(Debug, Serialize)]
pub struct CreateKeyResponse {
    pub id: String,
    /// Plaintext key — returned only on creation. Store it securely; it cannot be retrieved later.
    pub key: String,
    pub owner: String,
    pub rate_limit_rpm: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct KeyListItem {
    pub id: String,
    pub owner: String,
    pub rate_limit_rpm: i32,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

type ApiError = (StatusCode, Json<serde_json::Value>);
type ApiResult<T> = Result<T, ApiError>;

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateKeyRequest>,
) -> ApiResult<(StatusCode, Json<CreateKeyResponse>)> {
    let pool = require_db(&state)?;

    let random_bytes: [u8; 16] = rand::thread_rng().gen();
    let raw_key = format!("pk_{}", hex::encode(random_bytes));
    let key_id = Uuid::new_v4().to_string();
    let key_hash = hash_key(&raw_key);
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO api_keys (id, key_hash, owner, rate_limit_rpm, daily_spend_cap_cents, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&key_id)
    .bind(&key_hash)
    .bind(&req.owner)
    .bind(req.rate_limit_rpm)
    .bind(req.daily_spend_cap_cents)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("failed to create API key: {e}");
        internal_error("failed to create key")
    })?;

    tracing::info!(owner = %req.owner, key_id = %key_id, "API key created");

    Ok((
        StatusCode::CREATED,
        Json(CreateKeyResponse {
            id: key_id,
            key: raw_key,
            owner: req.owner,
            rate_limit_rpm: req.rate_limit_rpm,
            created_at: now,
        }),
    ))
}

pub async fn list(State(state): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    let pool = require_db(&state)?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        owner: String,
        rate_limit_rpm: i32,
        created_at: DateTime<Utc>,
        revoked_at: Option<DateTime<Utc>>,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, owner, rate_limit_rpm, created_at, revoked_at \
         FROM api_keys ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("failed to list API keys: {e}");
        internal_error("failed to list keys")
    })?;

    let keys: Vec<KeyListItem> = rows
        .into_iter()
        .map(|r| KeyListItem {
            id: r.id,
            owner: r.owner,
            rate_limit_rpm: r.rate_limit_rpm,
            created_at: r.created_at,
            revoked_at: r.revoked_at,
        })
        .collect();

    Ok(Json(serde_json::json!({
        "object": "list",
        "data": keys,
        "total": keys.len(),
    })))
}

pub async fn revoke(
    State(state): State<Arc<AppState>>,
    Path(key_id): Path<String>,
) -> ApiResult<StatusCode> {
    let pool = require_db(&state)?;

    let result = sqlx::query(
        "UPDATE api_keys SET revoked_at = $1 WHERE id = $2 AND revoked_at IS NULL",
    )
    .bind(Utc::now())
    .bind(&key_id)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("failed to revoke API key: {e}");
        internal_error("failed to revoke key")
    })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": {"message": "Key not found or already revoked", "type": "invalid_request_error"}
            })),
        ));
    }

    tracing::info!(key_id = %key_id, "API key revoked");
    Ok(StatusCode::NO_CONTENT)
}

fn require_db(state: &AppState) -> Result<&sqlx::PgPool, ApiError> {
    state.db.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": {
                    "message": "Key management requires DATABASE_URL to be configured",
                    "type": "service_unavailable"
                }
            })),
        )
    })
}

fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

fn internal_error(msg: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": {"message": msg, "type": "server_error"}
        })),
    )
}
