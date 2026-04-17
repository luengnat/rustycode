use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::OnceLock;

/// Configuration for LLM providers
#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_type: ProviderType,
    pub model: String,
    pub temperature: f32,
    pub context_size: usize,
    pub max_tokens: Option<u32>,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,
    pub system_prompt: Option<String>,
    // Custom provider fields
    pub custom_headers: Option<std::collections::HashMap<String, String>>,
    pub models: Option<std::collections::HashMap<String, String>>, // id -> name mapping
}

// Custom Debug implementation that redacts the API key
impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("provider_type", &self.provider_type)
            .field("model", &self.model)
            .field("temperature", &self.temperature)
            .field("context_size", &self.context_size)
            .field("max_tokens", &self.max_tokens)
            .field("api_key", &self.api_key.as_ref().map(|_| "***REDACTED***"))
            .field("endpoint", &self.endpoint)
            .field("system_prompt", &self.system_prompt)
            .field("custom_headers", &self.custom_headers)
            .field("models", &self.models)
            .finish()
    }
}

/// Supported LLM provider types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderType {
    Ollama,
    OpenAI,
    Anthropic,
    Gemini,      // Google Gemini
    Copilot,     // GitHub Copilot
    Bedrock,     // AWS Bedrock
    Azure,       // Azure OpenAI
    Cohere,      // Cohere
    Mistral,     // Mistral AI
    Together,    // Together AI
    Perplexity,  // Perplexity AI
    HuggingFace, // Hugging Face Inference API
    OpenRouter,  // OpenRouter
    Custom,      // OpenAI-compatible custom provider
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAI => write!(f, "openai"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::Gemini => write!(f, "gemini"),
            Self::Copilot => write!(f, "copilot"),
            Self::Bedrock => write!(f, "bedrock"),
            Self::Azure => write!(f, "azure"),
            Self::Cohere => write!(f, "cohere"),
            Self::Mistral => write!(f, "mistral"),
            Self::Together => write!(f, "together"),
            Self::Perplexity => write!(f, "perplexity"),
            Self::HuggingFace => write!(f, "huggingface"),
            Self::OpenRouter => write!(f, "openrouter"),
            Self::Custom => write!(f, "custom"),
            #[allow(unreachable_patterns)]
            _ => write!(f, "unknown"),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_type: ProviderType::Ollama,
            model: "llama2".to_string(),
            temperature: 0.1,
            context_size: 4096,
            max_tokens: None,
            api_key: None,
            endpoint: Some("http://localhost:11434".to_string()),
            system_prompt: None,
            custom_headers: None,
            models: None,
        }
    }
}

impl ProviderConfig {
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp.clamp(0.0, 2.0);
        self
    }

    pub fn with_context_size(mut self, size: usize) -> Self {
        self.context_size = size;
        self
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Returns a copy adjusted for use when sending to providers that expect
    /// a conservative `max_output_tokens` field name (helper for provider adapters).
    pub fn into_provider_max_output_tokens(&self) -> Option<u32> {
        self.max_tokens
    }

    /// Set the maximum output tokens (model-specific output budget).
    pub fn with_max_tokens(mut self, max: Option<u32>) -> Self {
        self.max_tokens = max;
        self
    }

    /// Apply a "precision" preset tailored for deterministic, factual/code one-shot responses.
    /// Temperature is set to 0.0 and max_tokens is increased to a conservative default if unset.
    pub fn precision_preset(mut self) -> Self {
        self.temperature = 0.0;
        if self.max_tokens.is_none() {
            // Default output budget for code/factual tasks; can be tuned per-provider.
            self.max_tokens = Some(2048);
        }
        self
    }

    /// Apply a "creative" preset for generative, higher-variance responses.
    /// Uses 0.7 for more exploratory generation.
    pub fn creative_preset(mut self) -> Self {
        self.temperature = 0.7;
        if self.max_tokens.is_none() {
            self.max_tokens = Some(512);
        }
        self
    }

    pub fn validate_remote_endpoint(&self) -> Result<()> {
        if let Some(endpoint) = &self.endpoint {
            validate_endpoint(endpoint)?;
        }

        Ok(())
    }
}

/// Response from LLM completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub stop_reason: Option<String>,
    pub tokens_used: Option<usize>,
}

/// LLMProvider trait for provider abstraction
#[async_trait::async_trait]
pub trait LLMProvider: Send + Sync {
    /// Get the provider type
    fn provider_type(&self) -> ProviderType;

    /// Complete a prompt and return the response
    async fn complete(&self, prompt: &str) -> Result<CompletionResponse>;

    /// Stream a completion response, returning chunks as they arrive
    async fn complete_stream<'a>(
        &'a self,
        prompt: &'a str,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + 'a>>;

    /// Get current configuration
    fn config(&self) -> &ProviderConfig;
}

pub fn validate_endpoint(endpoint: &str) -> Result<()> {
    let url = reqwest::Url::parse(endpoint)?;

    match url.scheme() {
        "https" => {}
        "http" if matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1")) => {}
        scheme => anyhow::bail!("unsupported endpoint scheme: {}", scheme),
    }

    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("endpoint must not embed credentials");
    }

    if url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!("endpoint must not include query strings or fragments");
    }

    Ok(())
}

pub fn sanitize_http_status_error(status: reqwest::StatusCode, provider_name: &str) -> String {
    format!(
        "{provider_name} API request failed with HTTP {}",
        status.as_u16()
    )
}

pub fn sanitize_error_message(message: &str) -> String {
    static QUERY_SECRET_RE: OnceLock<Regex> = OnceLock::new();
    static BEARER_RE: OnceLock<Regex> = OnceLock::new();
    static API_KEY_RE: OnceLock<Regex> = OnceLock::new();

    let query_secret_re = QUERY_SECRET_RE.get_or_init(|| {
        Regex::new(r"(?i)([?&](?:key|api[-_]?key|token|access_token)=)[^&\s]+")
            .expect("valid regex")
    });
    let bearer_re = BEARER_RE
        .get_or_init(|| Regex::new(r"(?i)(bearer\s+)[A-Za-z0-9._~-]+").expect("valid regex"));
    let api_key_re = API_KEY_RE
        .get_or_init(|| Regex::new(r"(?i)(x-api-key[:=]\s*)[^\s,;]+").expect("valid regex"));

    let redacted = query_secret_re.replace_all(message, "$1[REDACTED]");
    let redacted = bearer_re.replace_all(&redacted, "$1[REDACTED]");
    api_key_re
        .replace_all(&redacted, "$1[REDACTED]")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_default() {
        let config = ProviderConfig::default();
        assert_eq!(config.provider_type, ProviderType::Ollama);
        assert_eq!(config.model, "llama2");
        assert_eq!(config.temperature, 0.1);
    }

    #[test]
    fn test_provider_config_builder() {
        let config = ProviderConfig::default()
            .with_model("mistral")
            .with_temperature(0.5)
            .with_context_size(8192);

        assert_eq!(config.model, "mistral");
        assert_eq!(config.temperature, 0.5);
        assert_eq!(config.context_size, 8192);
    }

    #[test]
    fn test_temperature_clamping() {
        let config = ProviderConfig::default().with_temperature(3.5);
        assert_eq!(config.temperature, 2.0);

        let config = ProviderConfig::default().with_temperature(-0.5);
        assert_eq!(config.temperature, 0.0);
    }

    #[test]
    fn test_provider_type_display() {
        assert_eq!(ProviderType::Ollama.to_string(), "ollama");
        assert_eq!(ProviderType::OpenAI.to_string(), "openai");
    }

    #[test]
    fn test_validate_endpoint_rejects_query_and_credentials() {
        assert!(validate_endpoint("https://user:pass@example.com/v1").is_err());
        assert!(validate_endpoint("https://example.com/v1?key=secret").is_err());
    }

    #[test]
    fn test_validate_endpoint_allows_loopback_http_only() {
        assert!(validate_endpoint("http://localhost:11434").is_ok());
        assert!(validate_endpoint("http://example.com").is_err());
    }

    #[test]
    fn test_sanitize_error_message_redacts_credentials() {
        let sanitized =
            sanitize_error_message("Bearer sk-secret https://example.com?key=abc123 x-api-key: 42");
        assert!(!sanitized.contains("sk-secret"));
        assert!(!sanitized.contains("abc123"));
        assert!(!sanitized.contains("x-api-key: 42"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_provider_config_debug_redacts_api_key() {
        let config = ProviderConfig {
            provider_type: ProviderType::OpenAI,
            model: "gpt-4".to_string(),
            temperature: 0.7,
            context_size: 128000,
            max_tokens: Some(4096),
            api_key: Some("sk-secret-key-12345".to_string()),
            endpoint: Some("https://api.openai.com/v1".to_string()),
            system_prompt: None,
            custom_headers: None,
            models: None,
        };

        let debug_str = format!("{:?}", config);

        // Verify API key is redacted
        assert!(debug_str.contains("***REDACTED***"));
        assert!(!debug_str.contains("sk-secret-key-12345"));

        // Verify other fields are present
        assert!(debug_str.contains("OpenAI"));
        assert!(debug_str.contains("gpt-4"));
        assert!(debug_str.contains("https://api.openai.com/v1"));
    }

    #[test]
    fn test_provider_config_debug_with_none_api_key() {
        let config = ProviderConfig {
            provider_type: ProviderType::Anthropic,
            model: "claude-3-opus-20240229".to_string(),
            temperature: 0.5,
            context_size: 200000,
            max_tokens: Some(4096),
            api_key: None,
            endpoint: None,
            system_prompt: None,
            custom_headers: None,
            models: None,
        };

        let debug_str = format!("{:?}", config);

        // Should still work when api_key is None
        assert!(debug_str.contains("ProviderConfig"));
        assert!(debug_str.contains("Anthropic"));
        assert!(debug_str.contains("claude-3-opus"));
    }
}
