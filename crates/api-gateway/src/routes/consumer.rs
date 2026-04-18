use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;

// ---------- Query params ----------

#[derive(Debug, Deserialize)]
pub struct ConsumerQuery {
    pub api_key_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

// ---------- Response types ----------

#[derive(Debug, Serialize)]
pub struct ModelBreakdown {
    pub model: String,
    pub requests: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub tokens_total: i64,
    pub spend_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct DailyPoint {
    pub date: String,
    pub requests: i64,
    pub tokens: i64,
    pub spend_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct ConsumerAnalyticsResponse {
    pub total_requests: i64,
    pub total_tokens_in: i64,
    pub total_tokens_out: i64,
    pub total_tokens: i64,
    pub total_spend_usd: f64,
    pub tokens_by_model: Vec<ModelBreakdown>,
    pub daily_spend: Vec<DailyPoint>,
}

#[derive(Debug, Serialize)]
pub struct ModelBrowserEntry {
    pub name: String,
    pub license: String,
    pub node_count: i64,
    pub avg_latency_ms: Option<f64>,
    pub uptime_7d: Option<f64>,
    pub price_per_m_tokens: f64,
}

#[derive(Debug, Serialize)]
pub struct ModelBrowserResponse {
    pub models: Vec<ModelBrowserEntry>,
    pub price_per_m_tokens: f64,
}

// ---------- Handlers ----------

/// GET /v1/internal/analytics/consumer?api_key_id=&from=&to=
///
/// Returns spend, token usage, per-model breakdown, and a 30-day daily
/// time-series for all keys (default) or a single key via api_key_id.
pub async fn analytics(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ConsumerQuery>,
) -> axum::response::Response {
    let pool = match &state.db {
        Some(p) => p,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": {"message": "database not configured", "type": "server_error"}})),
            )
                .into_response()
        }
    };

    let token_rate: f64 = std::env::var("PROVIDER_TOKEN_RATE_USD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.000_001);

    let from_dt: DateTime<Utc> = params
        .from
        .as_deref()
        .and_then(parse_date)
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(30));
    let to_dt: DateTime<Utc> = params
        .to
        .as_deref()
        .and_then(parse_date)
        .unwrap_or_else(Utc::now);

    let key_filter: Option<&str> = params.api_key_id.as_deref();

    // --- Totals ---
    #[derive(sqlx::FromRow)]
    struct TotalsRow {
        total_requests: Option<i64>,
        total_tokens_in: Option<i64>,
        total_tokens_out: Option<i64>,
    }

    let totals = match sqlx::query_as::<_, TotalsRow>(
        "SELECT COUNT(*)::bigint                          AS total_requests,
                COALESCE(SUM(tokens_in),  0)::bigint      AS total_tokens_in,
                COALESCE(SUM(tokens_out), 0)::bigint      AS total_tokens_out
         FROM usage_logs
         WHERE ($1::TEXT IS NULL OR key_id = $1)
           AND timestamp >= $2
           AND timestamp <= $3",
    )
    .bind(key_filter)
    .bind(from_dt)
    .bind(to_dt)
    .fetch_one(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("consumer/analytics: totals query failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"message": "query failed", "type": "server_error"}})),
            )
                .into_response();
        }
    };

    let total_requests = totals.total_requests.unwrap_or(0);
    let total_tokens_in = totals.total_tokens_in.unwrap_or(0);
    let total_tokens_out = totals.total_tokens_out.unwrap_or(0);
    let total_tokens = total_tokens_in + total_tokens_out;
    let total_spend_usd = round4(total_tokens as f64 * token_rate);

    // --- Per-model breakdown ---
    #[derive(sqlx::FromRow)]
    struct ModelRow {
        model: String,
        requests: Option<i64>,
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
    }

    let model_rows: Vec<ModelRow> = sqlx::query_as::<_, ModelRow>(
        "SELECT model,
                COUNT(*)::bigint                          AS requests,
                COALESCE(SUM(tokens_in),  0)::bigint      AS tokens_in,
                COALESCE(SUM(tokens_out), 0)::bigint      AS tokens_out
         FROM usage_logs
         WHERE ($1::TEXT IS NULL OR key_id = $1)
           AND timestamp >= $2
           AND timestamp <= $3
         GROUP BY model
         ORDER BY SUM(tokens_in + tokens_out) DESC",
    )
    .bind(key_filter)
    .bind(from_dt)
    .bind(to_dt)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let tokens_by_model: Vec<ModelBreakdown> = model_rows
        .into_iter()
        .map(|r| {
            let ti = r.tokens_in.unwrap_or(0);
            let to_ = r.tokens_out.unwrap_or(0);
            let total = ti + to_;
            ModelBreakdown {
                model: r.model,
                requests: r.requests.unwrap_or(0),
                tokens_in: ti,
                tokens_out: to_,
                tokens_total: total,
                spend_usd: round4(total as f64 * token_rate),
            }
        })
        .collect();

    // --- Daily time-series ---
    #[derive(sqlx::FromRow)]
    struct DayRow {
        date: String,
        requests: Option<i64>,
        tokens: Option<i64>,
    }

    let day_rows: Vec<DayRow> = sqlx::query_as::<_, DayRow>(
        "SELECT TO_CHAR(DATE_TRUNC('day', timestamp), 'YYYY-MM-DD') AS date,
                COUNT(*)::bigint                                      AS requests,
                COALESCE(SUM(tokens_in + tokens_out), 0)::bigint     AS tokens
         FROM usage_logs
         WHERE ($1::TEXT IS NULL OR key_id = $1)
           AND timestamp >= $2
           AND timestamp <= $3
         GROUP BY DATE_TRUNC('day', timestamp)
         ORDER BY DATE_TRUNC('day', timestamp) ASC",
    )
    .bind(key_filter)
    .bind(from_dt)
    .bind(to_dt)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let daily_spend: Vec<DailyPoint> = day_rows
        .into_iter()
        .map(|r| {
            let tokens = r.tokens.unwrap_or(0);
            DailyPoint {
                date: r.date,
                requests: r.requests.unwrap_or(0),
                tokens,
                spend_usd: round4(tokens as f64 * token_rate),
            }
        })
        .collect();

    Json(ConsumerAnalyticsResponse {
        total_requests,
        total_tokens_in,
        total_tokens_out,
        total_tokens,
        total_spend_usd,
        tokens_by_model,
        daily_spend,
    })
    .into_response()
}

/// GET /v1/internal/models/stats
///
/// Returns all registered models with aggregated latency, uptime (last 7d),
/// and computed price per million tokens.
pub async fn models_stats(State(state): State<Arc<AppState>>) -> axum::response::Response {
    let pool = match &state.db {
        Some(p) => p,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": {"message": "database not configured", "type": "server_error"}})),
            )
                .into_response()
        }
    };

    let token_rate: f64 = std::env::var("PROVIDER_TOKEN_RATE_USD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.000_001);
    let price_per_m = round4(token_rate * 1_000_000.0);

    #[derive(sqlx::FromRow)]
    struct ModelRow {
        model_name: String,
        license: Option<String>,
        node_count: Option<i64>,
        avg_latency_ms: Option<f64>,
        uptime_7d: Option<f64>,
    }

    let rows: Vec<ModelRow> = sqlx::query_as::<_, ModelRow>(
        "SELECT nm.model_name,
                MIN(nm.license)                                                        AS license,
                COUNT(DISTINCT nm.node_id)::bigint                                     AS node_count,
                AVG(CASE WHEN nh.success THEN nh.latency_ms END)                       AS avg_latency_ms,
                SUM(CASE WHEN nh.success THEN 1 ELSE 0 END)::float8
                  / NULLIF(COUNT(nh.id), 0)::float8                                    AS uptime_7d
         FROM node_models nm
         LEFT JOIN node_health nh ON nh.node_id = nm.node_id
           AND nh.checked_at > NOW() - INTERVAL '7 days'
         GROUP BY nm.model_name
         ORDER BY nm.model_name",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let models: Vec<ModelBrowserEntry> = rows
        .into_iter()
        .map(|r| ModelBrowserEntry {
            name: r.model_name,
            license: r.license.unwrap_or_else(|| "unknown".to_string()),
            node_count: r.node_count.unwrap_or(0),
            avg_latency_ms: r.avg_latency_ms.map(|v| (v * 10.0).round() / 10.0),
            uptime_7d: r.uptime_7d.map(|v| (v * 10_000.0).round() / 10_000.0),
            price_per_m_tokens: price_per_m,
        })
        .collect();

    Json(ModelBrowserResponse {
        models,
        price_per_m_tokens: price_per_m,
    })
    .into_response()
}

// ---------- Helpers ----------

fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(nd) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return nd.and_hms_opt(0, 0, 0).map(|ndt| ndt.and_utc());
    }
    None
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
