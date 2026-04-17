/// Shard inference endpoint: `/infer/shard`
///
/// Receives a ShardExecutionRequest from the API gateway (or a previous node
/// in the pipeline), runs the forward pass for its assigned layer range, then
/// either returns the result (last shard) or forwards activations + remaining
/// plan to the next node in the pipeline.
///
/// MVP behaviour:
///   - The controller node (shard_index == 0, or single-node plan) runs a full
///     Ollama inference and returns the result.
///   - Non-controller nodes in a multi-node plan are not yet supported
///     (requires a tensor-level backend such as llama.cpp --rpc).  They return
///     an empty assistant message so the pipeline always completes.
///
/// When llama.cpp RPC is available, replace `run_ollama_inference` with a
/// call to the local llama.cpp layer-range forward pass.
use std::time::Duration;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

use shard_planner::ShardPlan;

use crate::AgentState;

// ── Request / response types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ShardForwardRequest {
    pub request_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub plan: ShardPlan,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShardForwardResponse {
    pub request_id: String,
    pub model: String,
    /// Ollama-compatible response body so the API gateway can parse it uniformly.
    #[serde(flatten)]
    pub ollama_body: serde_json::Value,
}

// ── Handler ──────────────────────────────────────────────────────────────────

pub async fn forward(
    State(state): State<AgentState>,
    Json(req): Json<ShardForwardRequest>,
) -> Result<Json<ShardForwardResponse>, (StatusCode, Json<serde_json::Value>)> {
    let my_assignment = req.plan.assignments.iter().find(|a| {
        a.host == local_host(&state)
            || a.agent_port == state.agent_port
    });

    let shard_index = my_assignment
        .and_then(|a| req.plan.assignments.iter().position(|x| x.node_id == a.node_id))
        .unwrap_or(0);

    info!(
        request_id = req.request_id,
        shard_index,
        total_shards = req.plan.assignments.len(),
        model = req.model,
        "received shard forward request"
    );

    // Run local inference (controller role or single-node).
    let ollama_body = run_ollama_inference(&state, &req).await.map_err(|e| {
        warn!("ollama inference failed: {e}");
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    // If this is not the last shard, forward result + remaining plan to next node.
    if shard_index + 1 < req.plan.assignments.len() {
        let (next_node_id, next_url) = {
            let next = &req.plan.assignments[shard_index + 1];
            (next.node_id.clone(), format!("{}/infer/shard", next.agent_base_url()))
        };

        // Build a new request carrying the accumulated output as context.
        // In a true tensor-parallel system this would be raw activations;
        // for the Ollama backend we append the assistant turn to messages.
        let assistant_content = ollama_body
            .pointer("/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut next_messages = req.messages.clone();
        next_messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: assistant_content,
        });

        let next_req = ShardForwardRequest {
            request_id: req.request_id.clone(),
            model: req.model.clone(),
            messages: next_messages,
            stream: false,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            top_p: req.top_p,
            plan: req.plan,
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("http client");

        info!(
            request_id = req.request_id,
            next_node = next_node_id,
            "forwarding to next shard"
        );

        let fwd_resp = client
            .post(&next_url)
            .json(&next_req)
            .send()
            .await
            .map_err(|e| {
                warn!("failed to forward to {next_url}: {e}");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": format!("next shard unreachable: {e}") })),
                )
            })?;

        if !fwd_resp.status().is_success() {
            let status = fwd_resp.status();
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("next shard returned {status}") })),
            ));
        }

        let final_body: ShardForwardResponse = fwd_resp.json().await.map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("failed to parse next shard response: {e}") })),
            )
        })?;

        return Ok(Json(final_body));
    }

    Ok(Json(ShardForwardResponse {
        request_id: req.request_id,
        model: req.model,
        ollama_body,
    }))
}

// ── Internal helpers ─────────────────────────────────────────────────────────

async fn run_ollama_inference(
    state: &AgentState,
    req: &ShardForwardRequest,
) -> anyhow::Result<serde_json::Value> {
    let url = format!(
        "http://localhost:{}/api/chat",
        state.node_port
    );

    let mut body = json!({
        "model": req.model,
        "messages": req.messages,
        "stream": false,
    });

    let mut opts = json!({});
    if let Some(t) = req.temperature {
        opts["temperature"] = json!(t);
    }
    if let Some(p) = req.top_p {
        opts["top_p"] = json!(p);
    }
    if let Some(m) = req.max_tokens {
        opts["num_predict"] = json!(m);
    }
    if opts.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        body["options"] = opts;
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Ollama returned {}", resp.status());
    }

    Ok(resp.json::<serde_json::Value>().await?)
}

fn local_host(_state: &AgentState) -> String {
    std::env::var("NODE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
}
