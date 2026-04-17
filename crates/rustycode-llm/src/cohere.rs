//! Cohere LLM provider implementation.
//!
//! This provider supports Cohere's API which provides access to
//! language models like Command, Command R, and Command R+.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Cohere dashboard)
//! - Model name (e.g., "command", "command-r", "command-r-plus")
//!
//! ## Environment Variables
//!
//! - `COHERE_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```rust
//! use rustycode_llm::{CohereProvider, ProviderConfig};
//! use secrecy::SecretString;
//!
//! let config = ProviderConfig {
//!     api_key: Some(SecretString::new("your-api-key".to_string().into())),
//!     base_url: None, // Uses default
//!     timeout_seconds: Some(180),
//!     extra_headers: None,
//!     retry_config: None,
//! };
//! let provider = CohereProvider::new(config).unwrap();
//! ```

use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::provider_v2::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
};
use crate::retry::extract_retry_after_ms;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// Default Cohere API endpoint
const COHERE_API_ENDPOINT: &str = "https://api.cohere.ai/v1/chat";

#[derive(Serialize)]
struct CohereRequest {
    message: String,
    model: String,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_history: Option<Vec<CohereChatMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preamble: Option<String>,
}

#[derive(Serialize)]
struct CohereChatMessage {
    role: String,
    message: String,
}

#[derive(Deserialize)]
struct CohereResponse {
    text: String,
    #[allow(dead_code)] // Kept for future use
    generation_id: String,
    finish_reason: Option<String>,
    #[allow(dead_code)] // Kept for future use
    meta: CohereMeta,
}

#[derive(Deserialize)]
struct CohereMeta {
    #[allow(dead_code)] // Kept for future use
    api_version: CohereApiVersion,
}

#[derive(Deserialize)]
struct CohereApiVersion {
    #[allow(dead_code)] // Kept for future use
    version: String,
}

/// Cohere LLM provider
pub struct CohereProvider {
    config: ProviderConfig,
    client: reqwest::Client,
}

impl CohereProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let timeout_secs = config.timeout_seconds.unwrap_or(180);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        // Add validated extra headers
        use crate::provider_v2::validate_extra_headers;
        let validated_headers = validate_extra_headers(&config.extra_headers)?;
        for (header_name, header_value) in validated_headers {
            headers.insert(header_name, header_value);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to build HTTP client: {}", e))
            })?;

        Ok(Self { config, client })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "cohere".to_string(),
            display_name: "Cohere".to_string(),
            description: "Enterprise AI platform with Command R and Command R+ models".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![ConfigField {
                    name: "api_key".to_string(),
                    label: "API Key".to_string(),
                    description: "Your Cohere API key from dashboard.cohere.com".to_string(),
                    field_type: ConfigFieldType::APIKey,
                    placeholder: Some("your-api-key".to_string()),
                    default: None,
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: true,
                }],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "Custom API endpoint (optional)".to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some("https://api.cohere.ai/v1/chat".to_string()),
                    default: Some("https://api.cohere.ai/v1/chat".to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "COHERE_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant powered by Cohere. {context}"
                    .to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Cohere models perform well with clear, direct instructions.".to_string(),
                        "Use conversational language when appropriate.".to_string(),
                    ],
                },
                tool_format: ToolFormat::None,
            },
            tool_calling: ToolCallingMetadata {
                supported: false,
                max_tools_per_call: None,
                parallel_calling: false,
                streaming_support: true,
            },
            recommended_models: vec![
                ModelInfo {
                    model_id: "command-r-plus-08-2024".to_string(),
                    display_name: "Command R+".to_string(),
                    description:
                        "Cohere's flagship model with 128k context and strong RAG capabilities"
                            .to_string(),
                    context_window: 128000,
                    supports_tools: false,
                    use_cases: vec![
                        "Document analysis".to_string(),
                        "RAG applications".to_string(),
                        "Long-form content".to_string(),
                    ],
                    cost_tier: 4,
                },
                ModelInfo {
                    model_id: "command-r-08-2024".to_string(),
                    display_name: "Command R".to_string(),
                    description: "Balanced model with good performance and lower cost".to_string(),
                    context_window: 128000,
                    supports_tools: false,
                    use_cases: vec![
                        "Chat applications".to_string(),
                        "Content generation".to_string(),
                    ],
                    cost_tier: 3,
                },
                ModelInfo {
                    model_id: "command".to_string(),
                    display_name: "Command".to_string(),
                    description: "Fast and efficient for simple tasks".to_string(),
                    context_window: 4096,
                    supports_tools: false,
                    use_cases: vec!["Simple queries".to_string(), "Quick responses".to_string()],
                    cost_tier: 2,
                },
            ],
        }
    }

    fn endpoint(&self) -> String {
        self.config
            .base_url
            .as_ref()
            .unwrap_or(&COHERE_API_ENDPOINT.to_string())
            .clone()
    }

    fn get_api_key(&self) -> Result<String, ProviderError> {
        // Try config first, then environment variable
        let config_key = self
            .config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string());
        let env_key = std::env::var("COHERE_API_KEY").ok();

        config_key.or(env_key).ok_or_else(|| {
            ProviderError::Configuration(
                "Cohere API key is required. Set api_key in config or COHERE_API_KEY env var"
                    .to_string(),
            )
        })
    }

    async fn complete_internal(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = self.endpoint();
        let api_key = self.get_api_key()?;

        // Convert messages to Cohere format
        // For now, we'll use the last message as the prompt
        let last_message = request
            .messages
            .last()
            .ok_or_else(|| ProviderError::Api("No messages in request".to_string()))?;

        let chat_history = if request.messages.len() > 1 {
            Some(
                request.messages[..request.messages.len() - 1]
                    .iter()
                    .map(|msg| CohereChatMessage {
                        role: msg.role.as_ref().to_string(),
                        message: msg.content.to_text(),
                    })
                    .collect(),
            )
        } else {
            None
        };

        let body = CohereRequest {
            message: last_message.content.to_text(),
            model: request.model.clone(),
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            chat_history,
            preamble: request.system_prompt.clone(),
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("x-api-key", api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Network(format!("failed to send request to Cohere: {}", e))
            })?;
        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Authentication failed. Check your COHERE_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found. {}. Available: command-r-plus, command-r, command-r7b",
                    error_text
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Cohere service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("Cohere API error {}: {}", status, error_text)),
            });
        }

        let cohere_response: CohereResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse Cohere response: {}", e))
        })?;

        Ok(CompletionResponse {
            content: cohere_response.text,
            model: request.model.clone(),
            usage: None, // Cohere doesn't return token usage in basic API
            stop_reason: cohere_response.finish_reason,
            citations: None,
            thinking_blocks: None,
        })
    }
}

#[async_trait]
impl LLMProvider for CohereProvider {
    fn name(&self) -> &'static str {
        "cohere"
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }

    async fn is_available(&self) -> bool {
        // Check if API key is available
        if self.get_api_key().is_err() {
            return false;
        }

        // Try to make a simple request to check connectivity
        let api_key = match self.get_api_key() {
            Ok(key) => key,
            Err(_) => return false,
        };

        let response = self
            .client
            .get(format!("{}/models", self.endpoint().replace("/chat", "")))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("x-api-key", api_key)
            .send()
            .await;

        response.is_ok()
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return known Cohere models (as of March 2026)
        Ok(vec![
            "command-r7b-12-2024".to_string(),
            "command-r-plus-08-2024".to_string(),
            "command-r-08-2024".to_string(),
            "command".to_string(),
            "command-light".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        self.complete_internal(&request).await
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let api_key = self.get_api_key()?;
        let endpoint = self.endpoint();

        let last_message = request
            .messages
            .last()
            .ok_or_else(|| ProviderError::Api("No messages in request".to_string()))?;

        let request_body = serde_json::json!({
            "message": last_message.content,
            "model": request.model,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true,
            "preamble": request.system_prompt
        });

        let response = self
            .client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("x-api-key", api_key)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Network(format!("failed to connect to Cohere API: {}", e))
            })?;

        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Authentication failed. Check your COHERE_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found. {}. Available: command-r-plus, command-r, command-r7b",
                    error_text
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Cohere service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("Cohere API error {}: {}", status, error_text)),
            });
        }

        // Convert bytes stream to SSE stream
        let bytes_stream = response.bytes_stream();

        let sse_stream = bytes_stream.map(|chunk_result| -> StreamChunk {
            let chunk = chunk_result
                .map_err(|e| ProviderError::Network(format!("failed to read chunk: {}", e)))?;
            let text = String::from_utf8_lossy(&chunk);
            let mut chunks = Vec::new();

            for line in text.lines() {
                if line.is_empty() {
                    continue;
                }
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        // Cohere streaming format: {"text": "...", "is_finished": false}
                        if let Some(text_val) = data.get("text").and_then(|t| t.as_str()) {
                            if !text_val.is_empty() {
                                chunks.push(text_val.to_string());
                            }
                        }
                    }
                }
            }

            Ok(crate::provider_v2::SSEEvent::text(chunks.join("")))
        });

        Ok(Box::pin(sse_stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn make_config(api_key: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            api_key: api_key.map(|s| SecretString::new(s.to_string().into())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_provider_name() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        assert_eq!(provider.name(), "cohere");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = CohereProvider::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = CohereProvider::metadata();
        assert_eq!(metadata.display_name, "Cohere");
        assert_eq!(metadata.provider_id, "cohere");
    }

    #[test]
    fn test_metadata_tool_calling_not_supported() {
        let metadata = CohereProvider::metadata();
        assert!(!metadata.tool_calling.supported);
        assert!(!metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = CohereProvider::metadata();
        assert_eq!(
            metadata.config_schema.env_mappings.get("api_key"),
            Some(&"COHERE_API_KEY".to_string())
        );
    }

    #[test]
    fn test_metadata_recommended_models() {
        let metadata = CohereProvider::metadata();
        let model_ids: Vec<&str> = metadata
            .recommended_models
            .iter()
            .map(|m| m.model_id.as_str())
            .collect();
        assert!(model_ids.iter().any(|id| id.contains("command-r")));
    }

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        assert_eq!(provider.endpoint(), COHERE_API_ENDPOINT);
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("test-key"));
        config.base_url = Some("https://custom-cohere.example.com/v1/chat".to_string());
        let provider = CohereProvider::new(config).unwrap();
        assert_eq!(
            provider.endpoint(),
            "https://custom-cohere.example.com/v1/chat"
        );
    }

    #[test]
    fn test_cohere_request_serialization() {
        let request = CohereRequest {
            message: "What is Rust?".to_string(),
            model: "command-r".to_string(),
            max_tokens: 512,
            temperature: 0.3,
            chat_history: None,
            preamble: Some("You are a coding assistant".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"message\":\"What is Rust?\""));
        assert!(json.contains("\"model\":\"command-r\""));
        assert!(json.contains("\"preamble\":\"You are a coding assistant\""));
        // chat_history should be absent when None
        assert!(!json.contains("\"chat_history\""));
    }

    #[test]
    fn test_cohere_response_deserialization() {
        let json = r#"{
            "text": "Rust is a systems programming language.",
            "generation_id": "gen-123",
            "finish_reason": "COMPLETE",
            "meta": {
                "api_version": {"version": "1.0"}
            }
        }"#;
        let response: CohereResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.text, "Rust is a systems programming language.");
        assert_eq!(response.finish_reason, Some("COMPLETE".to_string()));
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.contains("command")));
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        assert!(provider.config().is_some());
    }

    #[test]
    fn test_get_api_key_from_config() {
        let config = make_config(Some("my-cohere-key"));
        let provider = CohereProvider::new(config).unwrap();
        let key = provider.get_api_key().unwrap();
        assert_eq!(key, "my-cohere-key");
    }
}
