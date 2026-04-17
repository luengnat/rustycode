//! LLM configuration types for RustyCode
//!
//! Defines which LLM provider and model to use, along with parameters
//! that control generation behavior.

use serde::{Deserialize, Serialize};

/// Configuration for LLM in a session.
///
/// Defines which LLM provider and model to use, along with parameters
/// that control generation behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    /// Type of LLM provider (e.g., "ollama", "openai", "anthropic")
    pub provider_type: String,
    /// Model name (e.g., "llama2", "gpt-4", "claude-3-opus")
    pub model: String,
    /// Temperature for generation (0.0 - 1.0)
    pub temperature: f32,
    /// Maximum context size in tokens
    pub context_size: usize,
}

impl LLMConfig {
    /// Create a new LLM config
    pub fn new(
        provider_type: impl Into<String>,
        model: impl Into<String>,
        temperature: f32,
        context_size: usize,
    ) -> Self {
        Self {
            provider_type: provider_type.into(),
            model: model.into(),
            temperature: temperature.clamp(0.0, 1.0),
            context_size,
        }
    }

    /// Create a config with just provider and model (uses defaults for other values)
    pub fn with_provider(provider_type: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider_type: provider_type.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature.clamp(0.0, 1.0);
        self
    }

    /// Set the context size
    pub fn with_context_size(mut self, context_size: usize) -> Self {
        self.context_size = context_size;
        self
    }

    /// Get the maximum tokens allowed (accounting for system prompt and response)
    pub fn max_tokens(&self) -> usize {
        // Reserve ~10% for system prompt and response overhead
        (self.context_size * 9) / 10
    }
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            provider_type: "ollama".to_string(),
            model: "llama2".to_string(),
            temperature: 0.7,
            context_size: 4096,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_config_creation() {
        let config = LLMConfig::new("anthropic", "claude-3-opus", 0.5, 200000);

        assert_eq!(config.provider_type, "anthropic");
        assert_eq!(config.model, "claude-3-opus");
        assert_eq!(config.temperature, 0.5);
        assert_eq!(config.context_size, 200000);
    }

    #[test]
    fn test_llm_config_builder() {
        let config = LLMConfig::with_provider("openai", "gpt-4")
            .with_temperature(0.8)
            .with_context_size(128000);

        assert_eq!(config.provider_type, "openai");
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.temperature, 0.8);
        assert_eq!(config.context_size, 128000);
    }

    #[test]
    fn test_llm_config_default() {
        let config = LLMConfig::default();

        assert_eq!(config.provider_type, "ollama");
        assert_eq!(config.model, "llama2");
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.context_size, 4096);
    }

    #[test]
    fn test_temperature_clamping() {
        let config = LLMConfig::new("test", "test", 1.5, 1000);
        assert_eq!(config.temperature, 1.0); // Should be clamped to 1.0

        let config2 = LLMConfig::new("test", "test", -0.5, 1000);
        assert_eq!(config2.temperature, 0.0); // Should be clamped to 0.0
    }

    #[test]
    fn test_max_tokens() {
        let config = LLMConfig::new("test", "test", 0.7, 100000);
        assert_eq!(config.max_tokens(), 90000); // 90% of context size
    }

    // --- LLMConfig serde ---

    #[test]
    fn llm_config_serde_roundtrip() {
        let config = LLMConfig::new("anthropic", "claude-3", 0.5, 200000);
        let json = serde_json::to_string(&config).unwrap();
        let decoded: LLMConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_type, "anthropic");
        assert_eq!(decoded.model, "claude-3");
        assert!((decoded.temperature - 0.5).abs() < f32::EPSILON);
        assert_eq!(decoded.context_size, 200000);
    }

    #[test]
    fn llm_config_default_serde_roundtrip() {
        let config = LLMConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: LLMConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_type, "ollama");
        assert_eq!(decoded.model, "llama2");
    }

    #[test]
    fn llm_config_with_temperature_clamps_via_builder() {
        let config = LLMConfig::with_provider("test", "m").with_temperature(2.0);
        assert!((config.temperature - 1.0).abs() < f32::EPSILON);

        let config2 = LLMConfig::with_provider("test", "m").with_temperature(-1.0);
        assert!((config2.temperature - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn max_tokens_edge_case_zero_context() {
        let config = LLMConfig::new("test", "test", 0.7, 0);
        assert_eq!(config.max_tokens(), 0);
    }
}
