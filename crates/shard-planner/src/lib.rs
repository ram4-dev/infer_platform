pub mod planner;
pub mod registry;

pub use planner::{plan_shards, PlanError, ShardAssignment, ShardPlan};
pub use registry::{ModelRegistry, ModelSpec};

use serde::{Deserialize, Serialize};

/// VRAM capacity and address of a single compute node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapacity {
    pub node_id: String,
    pub host: String,
    /// Ollama / inference-engine port on this node.
    pub ollama_port: u16,
    /// Agent HTTP API port (default 8181).
    pub agent_port: u16,
    pub available_vram_mb: u64,
}
