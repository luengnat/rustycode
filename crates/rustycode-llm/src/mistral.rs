//! Mistral AI LLM provider implementation.
//!
//! This provider supports Mistral AI's API which provides access to
//! language models like Mistral 7B, Mixtral 8x7B, Mistral Large, etc.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Mistral AI dashboard)
//! - Model name (e.g., "mistral-large-latest", "mixtral-8x7b-2707")
//!
//! ## Environment Variables
//!
//! - `MISTRAL_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```toml
//! [ai]
//! provider = "mistral"
//! model = "mistral-large-latest"
//! api_key = "your-api-key"
//! ```

use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, PromptOptimizations, PromptTemplate,
    ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::provider_v2::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
    Usage,
};
use crate::retry::extract_retry_after_ms;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// Default Mistral AI API endpoint
const MISTRAL_API_ENDPOINT: &str = "https://api.mistral.ai/v1/chat/completions";

#[derive(Serialize)]
struct MistralRequest {
    model: String,
    messages: Vec<MistralMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct MistralMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MistralResponse {
    #[allow(dead_code)] // Kept for future use
    id: String,
    #[allow(dead_code)] // Kept for future use
    object: String,
    #[allow(dead_code)] // Kept for future use
    created: u64,
    model: String,
    choices: Vec<MistralChoice>,
    usage: MistralUsage,
}

#[derive(Deserialize)]
struct MistralChoice {
    #[allow(dead_code)] // Kept for future use
    index: usize,
    message: MistralResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct MistralResponseMessage {
    #[allow(dead_code)] // Kept for future use
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MistralUsage {
    #[allow(dead_code)] // Kept for future use
    prompt_tokens: usize,
    #[allow(dead_code)] // Kept for future use
    completion_tokens: usize,
    total_tokens: usize,
}

/// Mistral AI LLM provider
pub struct MistralProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    default_model: String,
}

impl MistralProvider {
    pub fn new(config: ProviderConfig, model: String) -> std::result::Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        // Try config first, then environment variable
        let config_key = config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string());
        let env_key = std::env::var("MISTRAL_API_KEY").ok();

        let api_key = config_key.or(env_key).ok_or_else(|| {
            ProviderError::Configuration(
                "Mistral API key is required. Set api_key in config or MISTRAL_API_KEY env var"
                    .to_string(),
            )
        })?;

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", api_key).parse().map_err(|e| {
                ProviderError::Configuration(format!("invalid API key format: {}", e))
            })?,
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(120));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .map_err(|e| ProviderError::Network(format!("failed to build HTTP client: {}", e)))?;

        Ok(Self {
            config,
            client,
            default_model: model,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        let mut env_mappings = HashMap::new();
        env_mappings.insert("api_key".to_string(), "MISTRAL_API_KEY".to_string());

        ProviderMetadata {
            provider_id: "mistral".to_string(),
            display_name: "Mistral AI".to_string(),
            description: "Advanced AI models including Mistral Large, Mixtral, and Codestral".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Mistral API key from console.mistral.ai".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("Your API key".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (optional)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api.mistral.ai/v1/chat/completions".to_string()),
                        default: Some("https://api.mistral.ai/v1/chat/completions".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings,
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant provided by Mistral AI.\n\n=== CONTEXT ===\n{context}\n\n=== GUIDELINES ===\n- Provide clear, accurate, and well-structured responses\n- When writing code, include explanations and best practices\n- Ask clarifying questions when requirements are ambiguous\n- Be thorough but concise in your answers".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "Mistral models excel at reasoning and code generation.".to_string(),
                        "Use structured formatting for complex responses.".to_string(),
                    ],
                },
                tool_format: ToolFormat::OpenAIFunctionCalling,
            },
            tool_calling: ToolCallingMetadata {
                supported: false,
                max_tools_per_call: None,
                parallel_calling: false,
                streaming_support: false,
            },
            recommended_models: vec![
            ],
        }
    }

    fn endpoint(&self) -> String {
        self.config
            .base_url
            .as_ref()
            .unwrap_or(&MISTRAL_API_ENDPOINT.to_string())
            .clone()
    }

    fn get_api_key(&self) -> Result<String, ProviderError> {
        let config_key = self
            .config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string());
        let env_key = std::env::var("MISTRAL_API_KEY").ok();
        config_key.or(env_key).ok_or_else(|| {
            ProviderError::Configuration(
                "Mistral API key is required. Set api_key in config or MISTRAL_API_KEY env var"
                    .to_string(),
            )
        })
    }
}

#[async_trait]
impl LLMProvider for MistralProvider {
    fn name(&self) -> &'static str {
        "mistral"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .map_or(false, |k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return known Mistral models (as of March 2026)
        Ok(vec![
            "mistral-large-2407".to_string(),
            "mixtral-8x22b-2407".to_string(),
            "mixtral-8x7b-2407".to_string(),
            "mistral-medium-2312".to_string(),
            "mistral-small-2409".to_string(),
            "codestral-2405".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let messages: Vec<MistralMessage> = request
            .messages
            .into_iter()
            .map(|msg| MistralMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            })
            .collect();

        let body = MistralRequest {
            model: request.model,
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
        };

        let api_key = self.get_api_key()?;

        let response = self
            .client
            .post(self.endpoint())
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("Failed to send request: {}", e)))?;

        // Clone headers for potential retry logic (e.g., 429 with Retry-After)
        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your MISTRAL_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found. {}. Available: mistral-large-latest, mistral-medium-latest, mistral-small-latest",
                    error_text
                )),
                429 => ProviderError::RateLimited { retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis) },
                502..=504 => ProviderError::Network(format!(
                    "Mistral service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        let mistral_response: MistralResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        let choice = mistral_response
            .choices
            .first()
            .ok_or_else(|| ProviderError::Api("No choices in response".to_string()))?;

        Ok(CompletionResponse {
            content: choice.message.content.clone(),
            model: mistral_response.model,
            usage: Some(Usage {
                input_tokens: mistral_response.usage.prompt_tokens as u32,
                output_tokens: mistral_response.usage.completion_tokens as u32,
                total_tokens: mistral_response.usage.total_tokens as u32,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            }),
            stop_reason: choice.finish_reason.clone(),
            citations: None,
            thinking_blocks: None,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let messages: Vec<MistralMessage> = request
            .messages
            .into_iter()
            .map(|msg| MistralMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            })
            .collect();

        let body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true
        });

        let api_key = self.get_api_key()?;

        let response = self
            .client
            .post(self.endpoint())
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("Failed to connect to Mistral: {}", e)))?;
        // Clone headers for potential retry logic (e.g., 429 with Retry-After)
        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your MISTRAL_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found. {}. Available: mistral-large-latest, mistral-medium-latest, mistral-small-latest",
                    error_text
                )),
                429 => ProviderError::RateLimited { retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis) },
                502..=504 => ProviderError::Network(format!(
                    "Mistral service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        let bytes_stream = response.bytes_stream();

        let sse_stream = bytes_stream.map(|chunk_result| -> StreamChunk {
            let chunk = chunk_result
                .map_err(|e| ProviderError::Network(format!("Failed to read chunk: {}", e)))?;
            let text = String::from_utf8_lossy(&chunk);
            let mut chunks = Vec::new();

            for line in text.lines() {
                if line.is_empty() {
                    continue;
                }
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if json_str == "[DONE]" {
                        continue;
                    }
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content") {
                                        if let Some(content_str) = content.as_str() {
                                            chunks.push(content_str.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(crate::provider_v2::SSEEvent::text(chunks.join("")))
        });

        Ok(Box::pin(sse_stream))
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
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
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert_eq!(provider.name(), "mistral");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = MistralProvider::new(config, "mistral-large-latest".to_string());
        assert!(result.is_err());
    }
}
