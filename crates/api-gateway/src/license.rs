use serde::Serialize;

/// Approved model license identifiers (normalized to lowercase).
/// Covers common open-source and community licenses used by popular LLMs.
pub static APPROVED_LICENSES: &[&str] = &[
    // Standard OSI
    "apache-2.0",
    "mit",
    "gpl-3.0",
    "lgpl-3.0",
    "bsd-2-clause",
    "bsd-3-clause",
    // Creative Commons
    "cc-by-4.0",
    "cc-by-sa-4.0",
    // Hugging Face OpenRAIL variants
    "openrail",
    "openrail++",
    "bigcode-openrail-m",
    // Meta Llama community licenses
    "llama-2",
    "llama-3",
    "llama-3.1",
    "llama-3.2",
    "llama-3.3",
    // Google Gemma
    "gemma",
    "gemma-2",
    // Mistral (permissive, effectively Apache)
    "mistral",
    // Microsoft Phi
    "phi-3",
    "phi-3.5",
    // Alibaba Qwen
    "qwen",
    // DeepSeek
    "deepseek",
    // Falcon
    "falcon-llm",
];

#[derive(Debug, Serialize)]
pub struct LicenseViolation {
    pub model: String,
    pub license: String,
}

/// Returns true if the license string is in the approved list (case-insensitive).
pub fn is_approved(license: &str) -> bool {
    let normalized = license.trim().to_lowercase();
    APPROVED_LICENSES.contains(&normalized.as_str())
}

/// Checks a slice of (model_name, license) pairs and returns any violations.
pub fn find_violations(models: &[(String, String)]) -> Vec<LicenseViolation> {
    models
        .iter()
        .filter(|(_, license)| !is_approved(license))
        .map(|(model, license)| LicenseViolation {
            model: model.clone(),
            license: license.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_licenses_pass() {
        assert!(is_approved("apache-2.0"));
        assert!(is_approved("Apache-2.0")); // case-insensitive
        assert!(is_approved("MIT"));
        assert!(is_approved("llama-3.1"));
        assert!(is_approved("gemma"));
    }

    #[test]
    fn unapproved_licenses_fail() {
        assert!(!is_approved("proprietary"));
        assert!(!is_approved("custom-commercial"));
        assert!(!is_approved(""));
        assert!(!is_approved("gpt-4-license"));
    }

    #[test]
    fn find_violations_catches_bad_licenses() {
        let models = vec![
            ("llama3".to_string(), "llama-3".to_string()),
            ("gpt4".to_string(), "proprietary".to_string()),
        ];
        let v = find_violations(&models);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].model, "gpt4");
    }
}
