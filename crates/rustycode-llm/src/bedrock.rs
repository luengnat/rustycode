//! AWS Bedrock LLM provider implementation.
//!
//! This provider supports AWS Bedrock which offers access to foundation models
//! from Anthropic, AI21, Meta, Mistral, and more through a single API.
//!
//! ## Configuration
//!
//! The provider can be configured with:
//! - Direct AWS credentials (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION)
//! - API key via the `api_key` field (for simpler setups)
//! - Custom endpoint for AWS Bedrock proxies
//!
//! ## Supported Models
//!
//! - Anthropic Claude (anthropic.claude-3-sonnet, claude-3-haiku, claude-3-opus)
//! - Meta Llama (meta.llama3-8b-instant, llama3-70b-instruct)
//! - Mistral AI (mistral.large-2407, mistral.small-2402)
//! - AI21 Jurassic (ai21.jamba-1-5-large, jamba-instruct)

use crate::provider_metadata::{ConfigField, ConfigFieldType, ConfigSchema, ProviderMetadata};
use crate::provider_v2::{
    CompletionRequest, CompletionResponse, LLMProvider, OutputConfig, ProviderConfig,
    ProviderError, StreamChunk,
};
use crate::retry::{extract_retry_after_ms, retry_with_backoff, RetryConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// Default AWS Bedrock endpoint format
#[allow(dead_code)] // Kept for future use
const BEDROCK_ENDPOINT_FORMAT: &str = "https://bedrock-runtime.{}.amazonaws.com";

#[derive(Serialize)]
struct BedrockRequest {
    model_id: String,
    messages: Vec<BedrockMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anthropic_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_config: Option<OutputConfig>,
}

#[derive(Serialize)]
struct BedrockMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct BedrockResponse {
    output: BedrockOutput,
    usage: BedrockUsage,
    model_id: String,
}

#[derive(Deserialize)]
struct BedrockOutput {
    message: BedrockResponseMessage,
}

#[derive(Deserialize)]
struct BedrockResponseMessage {
    #[allow(dead_code)] // Kept for future use
    role: String,
    content: Vec<BedrockResponseContent>,
}

#[derive(Deserialize)]
struct BedrockResponseContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Deserialize)]
struct BedrockUsage {
    #[allow(dead_code)] // Kept for future use
    input_tokens: usize,
    #[allow(dead_code)] // Kept for future use
    output_tokens: usize,
    total_tokens: usize,
}

/// AWS Bedrock LLM provider
pub struct BedrockProvider {
    config: ProviderConfig,
    region: String,
    client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    model: String,
}

impl BedrockProvider {
    pub fn new(config: ProviderConfig, model: String) -> Result<Self> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        // Get AWS region from config or environment
        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| {
                // Extract region from model name if possible (e.g., "us-east-1")
                if model.contains('.') {
                    model
                        .split('.')
                        .next_back()
                        .unwrap_or("us-east-1")
                        .to_string()
                } else {
                    "us-east-1".to_string()
                }
            });

        // Check for AWS credentials or API key
        let _has_aws_creds = std::env::var("AWS_ACCESS_KEY_ID").is_ok()
            && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok();

        // Use API key from config if provided
        let api_key = config.api_key.as_ref().map(|k| k.expose_secret());

        // Create HTTP client with headers
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        // Add API key header if provided (for custom endpoints/proxies)
        if let Some(key) = api_key {
            headers.insert(
                reqwest::header::HeaderName::from_static("x-api-key"),
                reqwest::header::HeaderValue::from_str(key).map_err(|e| {
                    ProviderError::Configuration(format!("invalid API key format: {}", e))
                })?,
            );
        }

        let timeout = config.timeout_seconds.unwrap_or(180);
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(timeout))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to build HTTP client: {}", e))
            })?;

        Ok(Self {
            config,
            region,
            client,
            model,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "bedrock".to_string(),
            display_name: "AWS Bedrock".to_string(),
            description: "Foundation models from Anthropic, Meta, Mistral, and more through AWS"
                .to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![],
                optional_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "AWS access key ID or custom endpoint API key".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("AKIAIOSFODNN7EXAMPLE".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Custom Endpoint".to_string(),
                        description: "Custom Bedrock endpoint (for proxies or custom deployments)"
                            .to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some(
                            "https://bedrock-runtime.us-east-1.amazonaws.com".to_string(),
                        ),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "AWS_ACCESS_KEY_ID".to_string());
                    map
                },
            },
            prompt_template: crate::provider_metadata::PromptTemplate {
                base_template: "You are an AI assistant hosted on AWS Bedrock.\n\n{context}"
                    .to_string(),
                optimizations: crate::provider_metadata::PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "Follow AWS best practices.".to_string(),
                        "Provide secure, enterprise-grade responses.".to_string(),
                    ],
                },
                tool_format: crate::provider_metadata::ToolFormat::None,
            },
            tool_calling: crate::provider_metadata::ToolCallingMetadata {
                supported: false,
                max_tools_per_call: None,
                parallel_calling: false,
                streaming_support: false,
            },
            recommended_models: vec![crate::provider_metadata::ModelInfo {
                model_id: "anthropic.claude-3-5-sonnet-20240620-v1:0".to_string(),
                display_name: "Claude 3.5 Sonnet".to_string(),
                description: "Balanced performance and speed".to_string(),
                context_window: 200_000,
                supports_tools: true,
                use_cases: vec!["General assistance".to_string(), "Coding".to_string()],
                cost_tier: 3,
            }],
        }
    }

    pub fn endpoint(&self) -> String {
        if let Some(endpoint) = &self.config.base_url {
            endpoint.clone()
        } else {
            format!("https://bedrock-runtime.{}.amazonaws.com", self.region)
        }
    }

    /// Get the AWS region for this provider
    pub fn region(&self) -> &str {
        &self.region
    }

    /// Determine if the model is an Anthropic Claude model
    fn is_anthropic_model(model: &str) -> bool {
        model.starts_with("anthropic.claude")
            || model.starts_with("claude-")
            || model.contains("claude")
    }

    /// Get the Anthropic version header for Claude models
    fn anthropic_version(model: &str) -> Option<String> {
        if Self::is_anthropic_model(model) {
            Some("bedrock-2023-05-31".to_string())
        } else {
            None
        }
    }

    async fn complete_internal(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let url = format!("{}/model/converse", self.endpoint());

        let anthropic_version = Self::anthropic_version(&request.model);

        // Convert messages to Bedrock format
        let bedrock_messages: Vec<BedrockMessage> = request
            .messages
            .iter()
            .map(|msg| BedrockMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            })
            .collect();

        let request_body = BedrockRequest {
            model_id: request.model.clone(),
            messages: bedrock_messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            system: request.system_prompt.clone(),
            anthropic_version,
            output_config: request.output_config.clone(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .context("request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Bedrock API error: {} - {}",
                status.as_u16(),
                error_text
            ));
        }

        let bedrock_response: BedrockResponse =
            response.json().await.context("failed to parse response")?;

        let content = bedrock_response
            .output
            .message
            .content
            .into_iter()
            .find_map(|c| match c.content_type.as_str() {
                "text" => Some(c.text),
                _ => None,
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            content,
            model: bedrock_response.model_id,
            usage: Some(crate::provider_v2::Usage {
                input_tokens: bedrock_response.usage.input_tokens as u32,
                output_tokens: bedrock_response.usage.output_tokens as u32,
                total_tokens: bedrock_response.usage.total_tokens as u32,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            }),
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
        })
    }

    #[allow(dead_code)] // Kept for future use
    async fn complete_v2(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/model/converse", self.endpoint());

        let anthropic_version = Self::anthropic_version(&request.model);

        let bedrock_messages: Vec<BedrockMessage> = request
            .messages
            .iter()
            .map(|msg| BedrockMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            })
            .collect();

        let request_body = BedrockRequest {
            model_id: request.model.clone(),
            messages: bedrock_messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            system: request.system_prompt.clone(),
            anthropic_version,
            output_config: request.output_config.clone(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("request failed: {}", e)))?;

        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Bedrock authentication failed. Check AWS credentials (aws configure). {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found: {}. Check model ID and region in AWS Bedrock console",
                    request.model
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Bedrock service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("Bedrock API error: {} - {}", status, error_text)),
            });
        }

        let bedrock_response: BedrockResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;

        let content = bedrock_response
            .output
            .message
            .content
            .into_iter()
            .find_map(|c| match c.content_type.as_str() {
                "text" => Some(c.text),
                _ => None,
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            content,
            model: bedrock_response.model_id,
            usage: Some(crate::provider_v2::Usage {
                input_tokens: bedrock_response.usage.input_tokens as u32,
                output_tokens: bedrock_response.usage.output_tokens as u32,
                total_tokens: bedrock_response.usage.total_tokens as u32,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            }),
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
        })
    }
}

#[async_trait]
impl LLMProvider for BedrockProvider {
    fn name(&self) -> &'static str {
        "bedrock"
    }

    async fn is_available(&self) -> bool {
        // We'll do a simple check - if we have AWS creds or API key, consider it available
        let has_credentials = std::env::var("AWS_ACCESS_KEY_ID").is_ok()
            && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok();

        let has_api_key = self
            .config
            .api_key
            .as_ref()
            .map_or(false, |k| !k.expose_secret().is_empty());

        has_credentials || has_api_key
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return a list of commonly available Bedrock models (as of March 2026)
        // In a real implementation, this would call the Bedrock ListFoundationModels API
        Ok(vec![
            // Claude 4.x series (latest)
            "anthropic.claude-opus-v4:0".to_string(),
            "anthropic.claude-sonnet-v4:0".to_string(),
            "anthropic.claude-haiku-v4:0".to_string(),
            // Claude 3.7 (latest Claude 3)
            "anthropic.claude-3-7-sonnet-20250219-v1:0".to_string(),
            // Claude 3.5 (stable)
            "anthropic.claude-3-5-sonnet-20241022-v2:0".to_string(),
            "anthropic.claude-3-5-haiku-20241022-v1:0".to_string(),
            // Claude 3 Opus
            "anthropic.claude-3-opus-20240229-v1:0".to_string(),
            // Llama 4.x
            "meta.llama4-8b-instruct-v1:0".to_string(),
            "meta.llama4-405b-instruct-v1:0".to_string(),
            // Llama 3.x
            "meta.llama3-3-70b-instruct-v1:0".to_string(),
            "meta.llama3-1-405b-instruct-v1:0".to_string(),
            "meta.llama3-8b-instruct-v1:0".to_string(),
            "meta.llama3-70b-instruct-v1:0".to_string(),
            // Mistral
            "mistral.mistral-large-2407-v1:0".to_string(),
            "mistral.mistral-small-2402-v1:0".to_string(),
            // AI21
            "ai21.jamba-1-5-large-v1:0".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let retry_config = RetryConfig::new()
            .with_max_attempts(5) // AWS needs more retries
            .with_base_delay(Duration::from_millis(800))
            .with_max_delay(Duration::from_secs(45))
            .with_jitter_factor(0.15);

        // We need to convert between anyhow::Error and ProviderError
        let result = retry_with_backoff(retry_config, || async {
            self.complete_internal(&request).await
        })
        .await;

        match result {
            Ok(response) => Ok(response),
            Err(e) => {
                let error_msg = e.to_string();
                // Parse status code from "Bedrock API error: XXX - ..." format
                let status_code = error_msg
                    .strip_prefix("Bedrock API error: ")
                    .and_then(|rest| rest.split(" - ").next())
                    .and_then(|code| code.parse::<u16>().ok());

                match status_code {
                    Some(401) | Some(403) => Err(ProviderError::Auth(error_msg)),
                    Some(404) => Err(ProviderError::InvalidModel(request.model)),
                    Some(429) => Err(ProviderError::RateLimited { retry_delay: None }),
                    _ => Err(ProviderError::Api(error_msg)),
                }
            }
        }
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let url = format!("{}/model/converse-stream", self.endpoint());

        let anthropic_version = Self::anthropic_version(&request.model);

        // Convert messages to Bedrock format
        let bedrock_messages: Vec<BedrockMessage> = request
            .messages
            .iter()
            .map(|msg| BedrockMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            })
            .collect();

        let request_body = BedrockRequest {
            model_id: request.model.clone(),
            messages: bedrock_messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            system: request.system_prompt.clone(),
            anthropic_version,
            output_config: request.output_config.clone(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("request failed: {}", e)))?;
        let headers = response.headers().clone();

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();

            return Err(match status.as_u16() {
                401 => ProviderError::Auth(format!(
                    "Bedrock authentication failed. Check AWS credentials (aws configure). {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!(
                    "model not found: {}. Check model ID and region in AWS Bedrock console",
                    request.model
                )),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Bedrock service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("Bedrock API error: {} - {}", status, error_text)),
            });
        }

        // Convert bytes stream to SSE stream
        let bytes_stream = response.bytes_stream();

        // Parse Bedrock's streaming response format
        let sse_stream = bytes_stream.map(|chunk_result| -> StreamChunk {
            let chunk = chunk_result
                .map_err(|e| ProviderError::Network(format!("failed to read chunk: {}", e)))?;
            let text = String::from_utf8_lossy(&chunk);

            // Bedrock streams use a custom event format
            // Each line is either "event: <type>" or "data: <json>"
            let mut current_text = String::new();

            for line in text.lines() {
                if line.is_empty() {
                    continue;
                }

                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();

                    // Check for stream end marker
                    if json_str == "[DONE]" {
                        continue;
                    }

                    // Parse the JSON response
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        // Extract content from the response
                        // Bedrock streaming format varies by model, but typically:
                        // - Anthropic Claude: data.output.message.content[0].text
                        // - Other models: similar structure with variations

                        if let Some(output) = data.get("output") {
                            if let Some(message) = output.get("message") {
                                if let Some(content) = message.get("content") {
                                    if let Some(content_arr) = content.as_array() {
                                        for content_item in content_arr {
                                            if let Some(text_val) = content_item.get("text") {
                                                if let Some(text_str) = text_val.as_str() {
                                                    current_text.push_str(text_str);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Also check for completion delta (non-Anthropic models)
                        if let Some(completion) = data.get("completion") {
                            if let Some(text_val) = completion.get("data") {
                                // Some models use "data" field instead
                                if let Some(text_str) = text_val.as_str() {
                                    current_text.push_str(text_str);
                                }
                            }
                        }
                    }
                }
            }

            if !current_text.is_empty() {
                Ok(crate::provider_v2::SSEEvent::text(current_text))
            } else {
                // Return empty event for keep-alive chunks
                Ok(crate::provider_v2::SSEEvent::text(""))
            }
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
        let provider =
            BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        assert_eq!(provider.name(), "bedrock");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_creates_without_api_key() {
        // Bedrock uses AWS credentials, not API key
        let config = make_config(None);
        let provider = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string());
        assert!(provider.is_ok());
    }
}
