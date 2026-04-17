use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub gpu_name: String,
    pub vram_mb: u64,
    pub status: NodeStatus,
    pub registered_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
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
    pub gpu_name: String,
    pub vram_mb: u64,
}
