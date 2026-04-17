use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    /// Ollama / inference-engine port.
    pub port: u16,
    /// Agent HTTP API port (default 8181).
    #[serde(default = "default_agent_port")]
    pub agent_port: u16,
    pub gpu_name: String,
    pub vram_mb: u64,
    pub status: NodeStatus,
    pub registered_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

fn default_agent_port() -> u16 {
    8181
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Online,
    Offline,
    Busy,
}

#[derive(Debug, Deserialize)]
pub struct RegisterNodeRequest {
    pub name: String,
    pub host: String,
    pub port: u16,
    #[serde(default = "default_agent_port")]
    pub agent_port: u16,
    pub gpu_name: String,
    pub vram_mb: u64,
}
