use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use tracing::warn;

use crate::{
    models::{ModelListResponse, OllamaTagsResponse, OpenAIModel},
    state::AppState,
};

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ModelListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let url = format!("{}/api/tags", state.ollama_url);

    let resp = reqwest::get(&url).await.map_err(|e| {
        warn!("Failed to reach Ollama at {url}: {e}");
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": {
                    "message": "Inference backend unavailable",
                    "type": "server_error"
                }
            })),
        )
    })?;

    let tags: OllamaTagsResponse = resp.json().await.map_err(|e| {
        warn!("Failed to parse Ollama tags: {e}");
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": {
                    "message": "Invalid response from inference backend",
                    "type": "server_error"
                }
            })),
        )
    })?;

    let now = Utc::now().timestamp();
    let data = tags
        .models
        .into_iter()
        .map(|m| OpenAIModel {
            id: m.name,
            object: "model",
            created: now,
            owned_by: "infer-platform".to_string(),
        })
        .collect();

    Ok(Json(ModelListResponse {
        object: "list",
        data,
    }))
}
