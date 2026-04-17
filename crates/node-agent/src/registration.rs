use std::time::Duration;

use serde_json::json;
use tracing::{info, warn};

use crate::AgentState;

pub async fn register_loop(state: AgentState) {
    let url = format!("{}/v1/internal/nodes", state.coordinator_url);
    let internal_key =
        std::env::var("INFER_INTERNAL_KEY").unwrap_or_else(|_| "internal_dev_secret".to_string());

    let payload = json!({
        "name": state.node_name,
        "host": local_ip(),
        "port": state.node_port,
        "agent_port": state.agent_port,
        "gpu_name": state.hardware.gpu_name,
        "vram_mb": state.hardware.vram_mb,
    });

    let client = reqwest::Client::new();
    let mut backoff_secs = 2u64;

    loop {
        match client
            .post(&url)
            .bearer_auth(&internal_key)
            .json(&payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                info!("Registered with coordinator at {}", state.coordinator_url);
                *state.registration_ok.write().await = true;
                backoff_secs = 2;

                // Heartbeat every 30s to keep registration fresh
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
            Ok(resp) => {
                warn!("Registration rejected: HTTP {}", resp.status());
                *state.registration_ok.write().await = false;
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(120);
            }
            Err(e) => {
                warn!("Could not reach coordinator: {e}");
                *state.registration_ok.write().await = false;
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(120);
            }
        }
    }
}

fn local_ip() -> String {
    std::env::var("NODE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string())
}
