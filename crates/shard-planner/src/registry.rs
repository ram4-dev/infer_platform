use serde::{Deserialize, Serialize};

/// Per-model metadata needed to plan a shard assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub name: String,
    /// Number of transformer layers in the model.
    pub total_layers: u32,
    /// Approximate VRAM consumed by a single transformer layer (weights only).
    pub vram_per_layer_mb: u64,
    /// VRAM for embeddings + KV cache overhead (always on the controller node).
    pub context_vram_mb: u64,
}

impl ModelSpec {
    pub fn total_vram_mb(&self) -> u64 {
        self.total_layers as u64 * self.vram_per_layer_mb + self.context_vram_mb
    }
}

/// Static table of known models with approximate VRAM figures.
struct KnownModel {
    name: &'static str,
    total_layers: u32,
    vram_per_layer_mb: u64,
    context_vram_mb: u64,
}

impl From<&KnownModel> for ModelSpec {
    fn from(k: &KnownModel) -> Self {
        ModelSpec {
            name: k.name.to_string(),
            total_layers: k.total_layers,
            vram_per_layer_mb: k.vram_per_layer_mb,
            context_vram_mb: k.context_vram_mb,
        }
    }
}

static KNOWN_MODELS: &[KnownModel] = &[
    KnownModel {
        name: "llama3.2:1b",
        total_layers: 16,
        vram_per_layer_mb: 80,
        context_vram_mb: 150,
    },
    KnownModel {
        name: "llama3.2:3b",
        total_layers: 28,
        vram_per_layer_mb: 70,
        context_vram_mb: 200,
    },
    KnownModel {
        name: "llama3.1:8b",
        total_layers: 32,
        vram_per_layer_mb: 145,
        context_vram_mb: 512,
    },
    KnownModel {
        name: "llama3.1:70b",
        total_layers: 80,
        vram_per_layer_mb: 480,
        context_vram_mb: 2048,
    },
    KnownModel {
        name: "llama3.1:405b",
        total_layers: 126,
        vram_per_layer_mb: 1820,
        context_vram_mb: 4096,
    },
    KnownModel {
        name: "llama3.3:70b",
        total_layers: 80,
        vram_per_layer_mb: 480,
        context_vram_mb: 2048,
    },
    KnownModel {
        name: "mistral:7b",
        total_layers: 32,
        vram_per_layer_mb: 120,
        context_vram_mb: 400,
    },
    KnownModel {
        name: "mistral-nemo",
        total_layers: 40,
        vram_per_layer_mb: 175,
        context_vram_mb: 600,
    },
    KnownModel {
        name: "qwen2.5:7b",
        total_layers: 28,
        vram_per_layer_mb: 150,
        context_vram_mb: 400,
    },
    KnownModel {
        name: "qwen2.5:14b",
        total_layers: 48,
        vram_per_layer_mb: 175,
        context_vram_mb: 600,
    },
    KnownModel {
        name: "qwen2.5:32b",
        total_layers: 64,
        vram_per_layer_mb: 300,
        context_vram_mb: 1024,
    },
    KnownModel {
        name: "qwen2.5:72b",
        total_layers: 80,
        vram_per_layer_mb: 540,
        context_vram_mb: 2048,
    },
    KnownModel {
        name: "phi4:14b",
        total_layers: 40,
        vram_per_layer_mb: 200,
        context_vram_mb: 600,
    },
    KnownModel {
        name: "deepseek-r1:7b",
        total_layers: 28,
        vram_per_layer_mb: 145,
        context_vram_mb: 400,
    },
    KnownModel {
        name: "deepseek-r1:14b",
        total_layers: 48,
        vram_per_layer_mb: 175,
        context_vram_mb: 600,
    },
    KnownModel {
        name: "deepseek-r1:32b",
        total_layers: 64,
        vram_per_layer_mb: 300,
        context_vram_mb: 1024,
    },
    KnownModel {
        name: "deepseek-r1:70b",
        total_layers: 80,
        vram_per_layer_mb: 480,
        context_vram_mb: 2048,
    },
    KnownModel {
        name: "gemma3:9b",
        total_layers: 46,
        vram_per_layer_mb: 120,
        context_vram_mb: 400,
    },
    KnownModel {
        name: "gemma3:27b",
        total_layers: 62,
        vram_per_layer_mb: 260,
        context_vram_mb: 1024,
    },
];

pub struct ModelRegistry;

impl ModelRegistry {
    /// Look up a model by name. Matches on prefix (e.g. "llama3.1:8b-instruct" → "llama3.1:8b").
    pub fn get(model_name: &str) -> Option<ModelSpec> {
        let lower = model_name.to_lowercase();
        KNOWN_MODELS
            .iter()
            .find(|m| lower == m.name || lower.starts_with(m.name))
            .map(ModelSpec::from)
    }

    /// Fall back to a generic estimate when the model is not in the registry.
    /// `total_vram_mb` should be the caller's best guess at the full model size.
    pub fn estimate(model_name: &str, total_vram_mb: u64) -> ModelSpec {
        let layers = 32u32;
        let context = total_vram_mb / 8;
        let weight_vram = total_vram_mb.saturating_sub(context);
        ModelSpec {
            name: model_name.to_string(),
            total_layers: layers,
            vram_per_layer_mb: weight_vram / layers as u64,
            context_vram_mb: context,
        }
    }
}
