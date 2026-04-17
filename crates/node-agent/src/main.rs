mod hardware;
mod registration;
mod shard;

use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use chrono::Utc;
use serde::Serialize;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use hardware::HardwareInfo;

#[derive(Clone)]
pub struct AgentState {
    pub hardware: Arc<HardwareInfo>,
    pub started_at: chrono::DateTime<Utc>,
    pub coordinator_url: String,
    pub node_name: String,
    pub node_port: u16,
    pub agent_port: u16,
    pub registration_ok: Arc<RwLock<bool>>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    node_name: String,
    registered: bool,
    uptime_secs: i64,
    hardware: HardwareInfo,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "node_agent=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let node_name = std::env::var("NODE_NAME").unwrap_or_else(|_| hostname());
    let node_port: u16 = std::env::var("NODE_PORT")
        .unwrap_or_else(|_| "11434".into())
        .parse()
        .unwrap_or(11434);
    let agent_port: u16 = std::env::var("AGENT_PORT")
        .unwrap_or_else(|_| "8181".into())
        .parse()
        .unwrap_or(8181);
    let coordinator_url = std::env::var("COORDINATOR_URL")
        .unwrap_or_else(|_| "http://localhost:8080".into());

    let hardware = Arc::new(hardware::collect());
    info!(
        "Node '{}' — {} ({} MB VRAM)",
        node_name, hardware.gpu_name, hardware.vram_mb
    );

    let state = AgentState {
        hardware: hardware.clone(),
        started_at: Utc::now(),
        coordinator_url: coordinator_url.clone(),
        node_name: node_name.clone(),
        node_port,
        agent_port,
        registration_ok: Arc::new(RwLock::new(false)),
    };

    // Register with coordinator in background, retry on failure
    let reg_state = state.clone();
    tokio::spawn(async move {
        registration::register_loop(reg_state).await;
    });

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/info", get(info_handler))
        .route("/ping", get(|| async { "pong" }))
        .route("/infer/shard", axum::routing::post(shard::forward))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("0.0.0.0:{agent_port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Node agent listening on {addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn health_handler(State(state): State<AgentState>) -> Json<HealthResponse> {
    let registered = *state.registration_ok.read().await;
    let uptime_secs = (Utc::now() - state.started_at).num_seconds();

    Json(HealthResponse {
        status: "ok",
        node_name: state.node_name.clone(),
        registered,
        uptime_secs,
        hardware: (*state.hardware).clone(),
    })
}

async fn info_handler(State(state): State<AgentState>) -> Json<serde_json::Value> {
    serde_json::json!({
        "name": state.node_name,
        "coordinator_url": state.coordinator_url,
        "ollama_port": state.node_port,
        "hardware": *state.hardware,
    })
    .into()
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| {
            std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|_| "unknown-node".to_string())
}
