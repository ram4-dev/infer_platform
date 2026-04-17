use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::ValidatedKey;
use crate::billing::validate_webhook_signature;
use crate::state::AppState;

type ApiError = (StatusCode, Json<serde_json::Value>);
type ApiResult<T> = Result<T, ApiError>;

// ── Consumer: POST /v1/billing/setup ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    /// Stripe PaymentMethod ID collected by the frontend via Stripe Elements.
    pub payment_method_id: String,
}

#[derive(Debug, Serialize)]
pub struct SetupResponse {
    pub customer_id: String,
    pub subscription_id: Option<String>,
}

pub async fn setup(
    State(state): State<Arc<AppState>>,
    Extension(key): Extension<ValidatedKey>,
    Json(req): Json<SetupRequest>,
) -> ApiResult<(StatusCode, Json<SetupResponse>)> {
    let pool = require_db(&state)?;
    let stripe_cfg = require_stripe(&state)?;

    // Idempotent: return existing record if already set up.
    #[derive(sqlx::FromRow)]
    struct Existing {
        stripe_customer_id: String,
        stripe_subscription_id: Option<String>,
    }

    if let Some(existing) = sqlx::query_as::<_, Existing>(
        "SELECT stripe_customer_id, stripe_subscription_id FROM billing_customers WHERE key_id = $1",
    )
    .bind(&key.key_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("billing_customers lookup failed: {e}");
        internal_error("database error")
    })? {
        return Ok((
            StatusCode::OK,
            Json(SetupResponse {
                customer_id: existing.stripe_customer_id,
                subscription_id: existing.stripe_subscription_id,
            }),
        ));
    }

    // Fetch the API key owner label for customer metadata.
    #[derive(sqlx::FromRow)]
    struct KeyOwner {
        owner: String,
    }
    let owner = sqlx::query_as::<_, KeyOwner>("SELECT owner FROM api_keys WHERE id = $1")
        .bind(&key.key_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!("api_keys lookup failed: {e}");
            internal_error("database error")
        })?
        .map(|r| r.owner)
        .unwrap_or_else(|| key.key_id.clone());

    // Create Stripe Customer.
    let customer_id = stripe_cfg
        .client
        .create_customer(&owner)
        .await
        .map_err(|e| {
            tracing::error!("Stripe create_customer failed: {e}");
            stripe_error("failed to create Stripe customer")
        })?;

    // Attach payment method.
    stripe_cfg
        .client
        .attach_payment_method(&customer_id, &req.payment_method_id)
        .await
        .map_err(|e| {
            tracing::error!("Stripe attach_payment_method failed: {e}");
            stripe_error("failed to attach payment method")
        })?;

    // Optionally create a metered subscription if STRIPE_PRICE_ID is configured.
    let subscription_id = if let Some(ref price_id) = stripe_cfg.price_id {
        let sub_id = stripe_cfg
            .client
            .create_subscription(&customer_id, price_id)
            .await
            .map_err(|e| {
                tracing::error!("Stripe create_subscription failed: {e}");
                stripe_error("failed to create subscription")
            })?;
        Some(sub_id)
    } else {
        None
    };

    // Persist billing customer record.
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO billing_customers \
         (id, key_id, stripe_customer_id, stripe_payment_method_id, stripe_subscription_id, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $6)",
    )
    .bind(&id)
    .bind(&key.key_id)
    .bind(&customer_id)
    .bind(&req.payment_method_id)
    .bind(&subscription_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("billing_customers insert failed: {e}");
        internal_error("database error")
    })?;

    tracing::info!(
        key_id = %key.key_id,
        customer_id = %customer_id,
        "Billing customer created"
    );

    Ok((
        StatusCode::CREATED,
        Json(SetupResponse {
            customer_id,
            subscription_id,
        }),
    ))
}

// ── Provider: POST /v1/billing/connect ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub node_id: String,
    pub return_url: String,
    pub refresh_url: String,
    pub email: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConnectResponse {
    pub account_id: String,
    pub onboarding_url: String,
}

pub async fn connect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConnectRequest>,
) -> ApiResult<(StatusCode, Json<ConnectResponse>)> {
    let pool = require_db(&state)?;
    let stripe_cfg = require_stripe(&state)?;

    // Return existing account link if already created but not yet complete.
    #[derive(sqlx::FromRow)]
    struct Existing {
        stripe_account_id: String,
        onboarding_complete: bool,
    }

    if let Some(existing) = sqlx::query_as::<_, Existing>(
        "SELECT stripe_account_id, onboarding_complete \
         FROM provider_stripe_accounts WHERE node_id = $1",
    )
    .bind(&req.node_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("provider_stripe_accounts lookup failed: {e}");
        internal_error("database error")
    })? {
        if existing.onboarding_complete {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": {
                        "message": "Stripe Connect onboarding already complete for this node",
                        "type": "invalid_request_error"
                    }
                })),
            ));
        }
        // Re-issue account link for incomplete onboarding.
        let url = stripe_cfg
            .client
            .create_account_link(
                &existing.stripe_account_id,
                &req.return_url,
                &req.refresh_url,
            )
            .await
            .map_err(|e| {
                tracing::error!("Stripe create_account_link failed: {e}");
                stripe_error("failed to create account link")
            })?;

        return Ok((
            StatusCode::OK,
            Json(ConnectResponse {
                account_id: existing.stripe_account_id,
                onboarding_url: url,
            }),
        ));
    }

    // Create a new Connect Express account.
    let account_id = stripe_cfg
        .client
        .create_connect_account(req.email.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Stripe create_connect_account failed: {e}");
            stripe_error("failed to create Connect account")
        })?;

    // Create onboarding link.
    let onboarding_url = stripe_cfg
        .client
        .create_account_link(&account_id, &req.return_url, &req.refresh_url)
        .await
        .map_err(|e| {
            tracing::error!("Stripe create_account_link failed: {e}");
            stripe_error("failed to create account link")
        })?;

    // Persist.
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO provider_stripe_accounts \
         (id, node_id, stripe_account_id, onboarding_complete, created_at, updated_at) \
         VALUES ($1, $2, $3, false, $4, $4)",
    )
    .bind(&id)
    .bind(&req.node_id)
    .bind(&account_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("provider_stripe_accounts insert failed: {e}");
        internal_error("database error")
    })?;

    tracing::info!(
        node_id = %req.node_id,
        account_id = %account_id,
        "Stripe Connect account created"
    );

    Ok((
        StatusCode::CREATED,
        Json(ConnectResponse {
            account_id,
            onboarding_url,
        }),
    ))
}

// ── Webhook: POST /v1/webhooks/stripe ───────────────────────────────────────

pub async fn stripe_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<StatusCode> {
    let stripe_cfg = require_stripe(&state)?;

    // Validate signature when a webhook secret is configured.
    if !stripe_cfg.webhook_secret.is_empty() {
        let sig = headers
            .get("stripe-signature")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": {"message": "Missing Stripe-Signature header", "type": "invalid_request_error"}
                    })),
                )
            })?;

        let valid =
            validate_webhook_signature(&body, sig, &stripe_cfg.webhook_secret).map_err(|e| {
                tracing::warn!("Webhook signature validation error: {e}");
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": {"message": "Invalid webhook signature", "type": "invalid_request_error"}
                    })),
                )
            })?;

        if !valid {
            tracing::warn!("Stripe webhook signature mismatch");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": {"message": "Webhook signature verification failed", "type": "authentication_error"}
                })),
            ));
        }
    }

    let event: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        tracing::warn!("Stripe webhook JSON parse error: {e}");
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": {"message": "Invalid JSON payload", "type": "invalid_request_error"}
            })),
        )
    })?;

    let event_type = event["type"].as_str().unwrap_or("unknown");
    tracing::info!(event_type, "Stripe webhook received");

    match event_type {
        "account.updated" => handle_account_updated(&state, &event).await,
        "invoice.payment_failed" => handle_payment_failed(&state, &event).await,
        _ => {
            tracing::debug!(event_type, "Stripe webhook event ignored");
        }
    }

    Ok(StatusCode::OK)
}

async fn handle_account_updated(state: &AppState, event: &serde_json::Value) {
    let Some(pool) = &state.db else { return };

    let account_id = match event["data"]["object"]["id"].as_str() {
        Some(id) => id,
        None => return,
    };

    let charges_enabled = event["data"]["object"]["charges_enabled"]
        .as_bool()
        .unwrap_or(false);
    let payouts_enabled = event["data"]["object"]["payouts_enabled"]
        .as_bool()
        .unwrap_or(false);
    let complete = charges_enabled && payouts_enabled;

    if let Err(e) = sqlx::query(
        "UPDATE provider_stripe_accounts \
         SET onboarding_complete = $1, updated_at = NOW() \
         WHERE stripe_account_id = $2",
    )
    .bind(complete)
    .bind(account_id)
    .execute(pool)
    .await
    {
        tracing::warn!(account_id, error = %e, "Failed to update onboarding_complete");
    } else if complete {
        tracing::info!(account_id, "Provider Connect onboarding complete");
    }
}

async fn handle_payment_failed(state: &AppState, event: &serde_json::Value) {
    let Some(pool) = &state.db else { return };

    let customer_id = match event["data"]["object"]["customer"].as_str() {
        Some(id) => id,
        None => return,
    };

    // Log the failure; in a production system you'd suspend the API key here.
    tracing::warn!(customer_id, "Stripe payment failed for customer");

    if let Err(e) = sqlx::query(
        "UPDATE billing_customers SET updated_at = NOW() WHERE stripe_customer_id = $1",
    )
    .bind(customer_id)
    .execute(pool)
    .await
    {
        tracing::warn!(error = %e, "Failed to update billing_customers on payment failure");
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn require_db(state: &AppState) -> Result<&sqlx::PgPool, ApiError> {
    state.db.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": {
                    "message": "Billing requires DATABASE_URL to be configured",
                    "type": "service_unavailable"
                }
            })),
        )
    })
}

fn require_stripe(
    state: &AppState,
) -> Result<&Arc<crate::state::StripeConfig>, ApiError> {
    state.stripe.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": {
                    "message": "Billing requires STRIPE_SECRET_KEY to be configured",
                    "type": "service_unavailable"
                }
            })),
        )
    })
}

fn internal_error(msg: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": {"message": msg, "type": "server_error"}
        })),
    )
}

fn stripe_error(msg: &str) -> ApiError {
    (
        StatusCode::BAD_GATEWAY,
        Json(serde_json::json!({
            "error": {"message": msg, "type": "stripe_error"}
        })),
    )
}
