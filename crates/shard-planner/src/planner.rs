use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ModelSpec, NodeCapacity};

/// Which layers a single node should run and how to reach it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShardAssignment {
    pub node_id: String,
    pub host: String,
    /// Ollama / inference-engine port.
    pub ollama_port: u16,
    /// Agent HTTP API port.
    pub agent_port: u16,
    /// Inclusive start of the layer range assigned to this node.
    pub layer_start: u32,
    /// Exclusive end of the layer range (node runs layers [start, end)).
    pub layer_end: u32,
    pub vram_required_mb: u64,
}

impl ShardAssignment {
    pub fn layer_count(&self) -> u32 {
        self.layer_end - self.layer_start
    }

    pub fn ollama_base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.ollama_port)
    }

    pub fn agent_base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.agent_port)
    }
}

/// The full assignment plan for a model across one or more nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardPlan {
    pub model: String,
    pub total_layers: u32,
    pub assignments: Vec<ShardAssignment>,
}

impl ShardPlan {
    pub fn is_single_node(&self) -> bool {
        self.assignments.len() == 1
    }

    /// First node in the execution pipeline; always present if the plan is valid.
    pub fn controller(&self) -> &ShardAssignment {
        &self.assignments[0]
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum PlanError {
    #[error("no online nodes available")]
    NoNodes,
    #[error(
        "insufficient total VRAM: need {need_mb} MB, have {have_mb} MB across {node_count} node(s)"
    )]
    InsufficientVram {
        need_mb: u64,
        have_mb: u64,
        node_count: usize,
    },
}

/// Greedy layer-assignment planner.
///
/// Nodes are sorted by available VRAM (largest first) so that the biggest
/// node becomes the controller and hosts the initial layers plus the context
/// overhead.  Layers are assigned left-to-right until all `model.total_layers`
/// are covered.
///
/// The `context_vram_mb` overhead is deducted from the first (controller) node
/// because that is where the KV cache and embedding table reside.
pub fn plan_shards(model: &ModelSpec, nodes: &[NodeCapacity]) -> Result<ShardPlan, PlanError> {
    if nodes.is_empty() {
        return Err(PlanError::NoNodes);
    }

    let total_available: u64 = nodes.iter().map(|n| n.available_vram_mb).sum();
    let total_needed = model.total_vram_mb();

    if total_available < total_needed {
        return Err(PlanError::InsufficientVram {
            need_mb: total_needed,
            have_mb: total_available,
            node_count: nodes.len(),
        });
    }

    // Sort largest VRAM first so the controller has the most headroom.
    let mut sorted: Vec<&NodeCapacity> = nodes.iter().collect();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.available_vram_mb));

    let mut assignments: Vec<ShardAssignment> = Vec::new();
    let mut layer_cursor = 0u32;
    // Context overhead is reserved on the controller (first node only).
    let mut context_reserve = model.context_vram_mb;

    for node in &sorted {
        if layer_cursor >= model.total_layers {
            break;
        }

        let usable = node.available_vram_mb.saturating_sub(context_reserve);
        context_reserve = 0;

        if usable < model.vram_per_layer_mb {
            // Node can't hold even one layer — skip.
            continue;
        }

        let max_layers = (usable / model.vram_per_layer_mb) as u32;
        let remaining = model.total_layers - layer_cursor;
        let assigned = max_layers.min(remaining);

        assignments.push(ShardAssignment {
            node_id: node.node_id.clone(),
            host: node.host.clone(),
            ollama_port: node.ollama_port,
            agent_port: node.agent_port,
            layer_start: layer_cursor,
            layer_end: layer_cursor + assigned,
            vram_required_mb: assigned as u64 * model.vram_per_layer_mb,
        });

        layer_cursor += assigned;
    }

    // Verify all layers are covered (should be guaranteed by the VRAM check above).
    if layer_cursor < model.total_layers {
        return Err(PlanError::InsufficientVram {
            need_mb: total_needed,
            have_mb: total_available,
            node_count: nodes.len(),
        });
    }

    Ok(ShardPlan {
        model: model.name.clone(),
        total_layers: model.total_layers,
        assignments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ModelRegistry;

    fn node(id: &str, vram_mb: u64) -> NodeCapacity {
        NodeCapacity {
            node_id: id.to_string(),
            host: format!("{id}.local"),
            ollama_port: 11434,
            agent_port: 8181,
            available_vram_mb: vram_mb,
        }
    }

    fn llama3b() -> ModelSpec {
        ModelRegistry::get("llama3.2:3b").unwrap()
    }

    fn llama70b() -> ModelSpec {
        ModelRegistry::get("llama3.1:70b").unwrap()
    }

    #[test]
    fn single_node_fits() {
        let model = llama3b(); // 28 layers × 70 MB + 200 MB = 2160 MB
        let nodes = [node("gpu0", 8192)];
        let plan = plan_shards(&model, &nodes).unwrap();

        assert!(plan.is_single_node());
        assert_eq!(plan.assignments[0].layer_start, 0);
        assert_eq!(plan.assignments[0].layer_end, 28);
        assert_eq!(plan.total_layers, 28);
    }

    #[test]
    fn two_nodes_split_layers() {
        // 80 layers × 480 MB + 2048 MB = 40480 MB needed
        // node0: 24576 MB (24 GB), node1: 24576 MB (24 GB) → total 49152 MB, OK
        let model = llama70b();
        let nodes = [node("gpu0", 24576), node("gpu1", 24576)];
        let plan = plan_shards(&model, &nodes).unwrap();

        assert_eq!(plan.assignments.len(), 2);
        assert_eq!(plan.assignments[0].layer_start, 0);
        // All 80 layers should be covered
        let covered: u32 = plan.assignments.iter().map(|a| a.layer_count()).sum();
        assert_eq!(covered, 80);
        // Assignments must be contiguous
        assert_eq!(
            plan.assignments[1].layer_start,
            plan.assignments[0].layer_end
        );
    }

    #[test]
    fn no_nodes_returns_error() {
        let err = plan_shards(&llama3b(), &[]).unwrap_err();
        assert_eq!(err, PlanError::NoNodes);
    }

    #[test]
    fn insufficient_vram_returns_error() {
        // llama3b needs ~2160 MB; give only 1 GB
        let err = plan_shards(&llama3b(), &[node("gpu0", 1024)]).unwrap_err();
        assert!(matches!(err, PlanError::InsufficientVram { .. }));
    }

    #[test]
    fn controller_is_largest_vram_node() {
        let model = llama70b();
        // node1 has more VRAM — it should become the controller
        let nodes = [node("small", 8192), node("big", 32768)];
        let plan = plan_shards(&model, &nodes).unwrap();
        assert_eq!(plan.controller().node_id, "big");
    }

    #[test]
    fn three_node_shard_contiguous() {
        // Use a model that requires more VRAM than any single node
        let model = llama70b(); // 40480 MB total
        let nodes = [node("a", 16384), node("b", 16384), node("c", 16384)];
        let plan = plan_shards(&model, &nodes).unwrap();

        // Verify contiguous layer coverage with no overlaps
        let mut cursor = 0u32;
        for a in &plan.assignments {
            assert_eq!(a.layer_start, cursor, "gap or overlap at layer {cursor}");
            cursor = a.layer_end;
        }
        assert_eq!(cursor, model.total_layers);
    }

    #[test]
    fn model_registry_lookup() {
        assert!(ModelRegistry::get("llama3.2:3b").is_some());
        assert!(ModelRegistry::get("llama3.2:3b-instruct-q4_K_M").is_some()); // prefix match
        assert!(ModelRegistry::get("unknown-model-xyz").is_none());
    }

    #[test]
    fn model_spec_total_vram() {
        let spec = llama3b();
        // 28 * 70 + 200 = 2160
        assert_eq!(spec.total_vram_mb(), 2160);
    }
}
