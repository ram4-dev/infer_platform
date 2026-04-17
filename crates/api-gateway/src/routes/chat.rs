use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json, Response,
    },
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, warn};
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Deserialize, Clone)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// ── Ollama wire types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct OllamaChunk {
    message: Option<OllamaChunkMessage>,
    done: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaChunkMessage {
    content: String,
}

// ── OpenAI response types ────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatCompletionChunk {
    id: String,
    object: &'static str,
    created: i64,
    model: String,
    choices: Vec<ChunkChoice>,
}

#[derive(Debug, Serialize)]
struct ChunkChoice {
    index: u32,
    delta: Delta,
    finish_reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionResponse {
    id: String,
    object: &'static str,
    created: i64,
    model: String,
    choices: Vec<CompletionChoice>,
    usage: Usage,
}

#[derive(Debug, Serialize)]
struct CompletionChoice {
    index: u32,
    message: ChatMessage,
    finish_reason: &'static str,
}

#[derive(Debug, Serialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ── Handler ──────────────────────────────────────────────────────────────────

pub async fn completions(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> Response {
    // Resolve which node(s) should handle this request.
    let plan = {
        let nodes = state.nodes.read().await;
        state.coordinator.build_plan(&req.model, &nodes)
    };

    if req.stream {
        // Streaming: always proxy directly to the controller's Ollama.
        // Multi-node pipeline streaming is a future enhancement.
        let ollama_base = plan
            .as_ref()
            .map(|p| p.controller().ollama_base_url())
            .unwrap_or_else(|| state.ollama_url.clone());

        if let Some(ref p) = plan {
            if !p.is_single_node() {
                info!(
                    model = req.model,
                    shards = p.assignments.len(),
                    "streaming: falling back to controller node (pipeline streaming pending)"
                );
            }
        }

        stream_response(ollama_base, req).await
    } else {
        // Non-streaming: use full pipeline execution for multi-node plans.
        let request_id = Uuid::new_v4().to_string();

        match plan {
            Some(ref p) if !p.is_single_node() => {
                match state.coordinator.execute(&req, p, &request_id).await {
                    Ok(ollama_body) => {
                        build_openai_response(ollama_body, &req.model, &request_id)
                            .into_response()
                    }
                    Err(e) => {
                        warn!("pipeline execution failed: {e}");
                        error_json(StatusCode::BAD_GATEWAY, &e.to_string())
                    }
                }
            }
            _ => {
                let ollama_base = plan
                    .as_ref()
                    .map(|p| p.controller().ollama_base_url())
                    .unwrap_or_else(|| state.ollama_url.clone());
                non_stream_response(ollama_base, &request_id, req)
                    .await
                    .into_response()
            }
        }
    }
}

// ── Streaming path ───────────────────────────────────────────────────────────

async fn stream_response(ollama_base: String, req: ChatCompletionRequest) -> Response {
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4().simple());
    let created = chrono::Utc::now().timestamp();
    let model = req.model.clone();

    let ollama_req = OllamaChatRequest {
        model: &req.model,
        messages: &req.messages,
        stream: true,
        options: build_options(&req),
    };

    let url = format!("{ollama_base}/api/chat");
    let client = reqwest::Client::new();

    let ollama_resp = match client.post(&url).json(&ollama_req).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let status = r.status();
            warn!("Ollama returned {status} at {url}");
            return error_json(StatusCode::BAD_GATEWAY, &format!("backend returned {status}"));
        }
        Err(e) => {
            warn!("Failed to reach Ollama at {url}: {e}");
            return error_json(StatusCode::BAD_GATEWAY, "inference backend unavailable");
        }
    };

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(64);
    let id_clone = completion_id.clone();
    let model_clone = model.clone();

    tokio::spawn(async move {
        let role_chunk = ChatCompletionChunk {
            id: id_clone.clone(),
            object: "chat.completion.chunk",
            created,
            model: model_clone.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta { role: Some("assistant"), content: None },
                finish_reason: None,
            }],
        };
        let _ = tx
            .send(Ok(Event::default().data(
                serde_json::to_string(&role_chunk).unwrap_or_default(),
            )))
            .await;

        let mut byte_stream = ollama_resp.bytes_stream();
        let mut buf = String::new();

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    warn!("stream read error: {e}");
                    break;
                }
            };

            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(nl_pos) = buf.find('\n') {
                let line = buf[..nl_pos].trim().to_string();
                buf.drain(..=nl_pos);

                if line.is_empty() {
                    continue;
                }

                let ollama_chunk: OllamaChunk = match serde_json::from_str(&line) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("failed to parse Ollama chunk '{line}': {e}");
                        continue;
                    }
                };

                if ollama_chunk.done {
                    let final_chunk = ChatCompletionChunk {
                        id: id_clone.clone(),
                        object: "chat.completion.chunk",
                        created,
                        model: model_clone.clone(),
                        choices: vec![ChunkChoice {
                            index: 0,
                            delta: Delta { role: None, content: None },
                            finish_reason: Some("stop"),
                        }],
                    };
                    let _ = tx
                        .send(Ok(Event::default().data(
                            serde_json::to_string(&final_chunk).unwrap_or_default(),
                        )))
                        .await;
                    let _ = tx.send(Ok(Event::default().data("[DONE]"))).await;
                    return;
                }

                if let Some(msg) = ollama_chunk.message {
                    if !msg.content.is_empty() {
                        let content_chunk = ChatCompletionChunk {
                            id: id_clone.clone(),
                            object: "chat.completion.chunk",
                            created,
                            model: model_clone.clone(),
                            choices: vec![ChunkChoice {
                                index: 0,
                                delta: Delta { role: None, content: Some(msg.content) },
                                finish_reason: None,
                            }],
                        };
                        let _ = tx
                            .send(Ok(Event::default().data(
                                serde_json::to_string(&content_chunk).unwrap_or_default(),
                            )))
                            .await;
                    }
                }
            }
        }

        let _ = tx.send(Ok(Event::default().data("[DONE]"))).await;
    });

    Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ── Non-streaming path ───────────────────────────────────────────────────────

async fn non_stream_response(
    ollama_base: String,
    request_id: &str,
    req: ChatCompletionRequest,
) -> Result<Json<ChatCompletionResponse>, Response> {
    let url = format!("{ollama_base}/api/chat");
    let client = reqwest::Client::new();

    let ollama_req = OllamaChatRequest {
        model: &req.model,
        messages: &req.messages,
        stream: false,
        options: build_options(&req),
    };

    let resp = client
        .post(&url)
        .json(&ollama_req)
        .send()
        .await
        .map_err(|e| {
            warn!("Failed to reach Ollama at {url}: {e}");
            error_json(StatusCode::BAD_GATEWAY, "inference backend unavailable")
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        warn!("Ollama returned {status} at {url}");
        return Err(error_json(
            StatusCode::BAD_GATEWAY,
            &format!("backend returned {status}"),
        ));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| {
        warn!("Failed to parse Ollama response: {e}");
        error_json(StatusCode::BAD_GATEWAY, "invalid response from backend")
    })?;

    Ok(build_openai_response(body, &req.model, request_id))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn build_openai_response(
    ollama_body: serde_json::Value,
    model: &str,
    request_id: &str,
) -> Json<ChatCompletionResponse> {
    let content = ollama_body
        .pointer("/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let prompt_tokens = ollama_body
        .pointer("/prompt_eval_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let completion_tokens = ollama_body
        .pointer("/eval_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    Json(ChatCompletionResponse {
        id: format!("chatcmpl-{request_id}"),
        object: "chat.completion",
        created: chrono::Utc::now().timestamp(),
        model: model.to_string(),
        choices: vec![CompletionChoice {
            index: 0,
            message: ChatMessage { role: "assistant".to_string(), content },
            finish_reason: "stop",
        }],
        usage: Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    })
}

fn build_options(req: &ChatCompletionRequest) -> Option<OllamaOptions> {
    if req.max_tokens.is_none() && req.temperature.is_none() && req.top_p.is_none() {
        return None;
    }
    Some(OllamaOptions {
        num_predict: req.max_tokens,
        temperature: req.temperature,
        top_p: req.top_p,
    })
}

fn error_json(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": { "message": message, "type": "server_error" }
        })),
    )
        .into_response()
}
