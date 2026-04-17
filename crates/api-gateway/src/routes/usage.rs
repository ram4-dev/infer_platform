use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::Json};
use serde::Serialize;
use serde_json::json;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct KeyUsageSummary {
    pub key_id: String,
    pub request_count: i64,
    pub total_tokens_in: i64,
    pub total_tokens_out: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Serialize)]
pub struct UsageResponse {
    pub by_key: Vec<KeyUsageSummary>,
    pub totals: TotalUsage,
}

#[derive(Debug, Serialize)]
pub struct TotalUsage {
    pub request_count: i64,
    pub total_tokens_in: i64,
    pub total_tokens_out: i64,
    pub total_tokens: i64,
}

pub async fn summary(State(state): State<Arc<AppState>>) -> axum::response::Response {
    let pool = match &state.db {
        Some(p) => p,
        None => return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": {"message": "database not configured", "type": "server_error"}})),
        )
            .into_response(),
    };

    #[derive(sqlx::FromRow)]
    struct Row {
        key_id: Option<String>,
        request_count: Option<i64>,
        total_tokens_in: Option<i64>,
        total_tokens_out: Option<i64>,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT key_id, \
                COUNT(*)::bigint AS request_count, \
                COALESCE(SUM(tokens_in), 0)::bigint AS total_tokens_in, \
                COALESCE(SUM(tokens_out), 0)::bigint AS total_tokens_out \
         FROM usage_logs \
         GROUP BY key_id \
         ORDER BY request_count DESC",
    )
    .fetch_all(pool)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("usage query failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"message": "query failed", "type": "server_error"}})),
            )
                .into_response();
        }
    };

    let by_key: Vec<KeyUsageSummary> = rows
        .into_iter()
        .map(|r| {
            let ti = r.total_tokens_in.unwrap_or(0);
            let to = r.total_tokens_out.unwrap_or(0);
            KeyUsageSummary {
                key_id: r.key_id.unwrap_or_default(),
                request_count: r.request_count.unwrap_or(0),
                total_tokens_in: ti,
                total_tokens_out: to,
                total_tokens: ti + to,
            }
        })
        .collect();

    let totals = TotalUsage {
        request_count: by_key.iter().map(|k| k.request_count).sum(),
        total_tokens_in: by_key.iter().map(|k| k.total_tokens_in).sum(),
        total_tokens_out: by_key.iter().map(|k| k.total_tokens_out).sum(),
        total_tokens: by_key.iter().map(|k| k.total_tokens).sum(),
    };

    Json(UsageResponse { by_key, totals }).into_response()
}

use axum::response::IntoResponse;
