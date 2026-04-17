//! OpenRouter LLM provider implementation.
//!
//! OpenRouter provides unified access to multiple LLM providers through a single API.
//! This includes free models that can be used without cost.
//!
//! ## Free Models
//!
//! OpenRouter offers several free models:
//! - `google/gemma-2-9b:free` - Google Gemma 2 9B
//! - `meta-llama/llama-3-8b:free` - Meta Llama 3 8B
//! - `microsoft/phi-3-medium-128k:free` - Microsoft Phi-3
//! - `mistralai/mistral-7b:free` - Mistral 7B
//!
//! ## Setup
//!
//! 1. Get API key from https://openrouter.ai/keys
//! 2. Set environment variable:
//!    ```bash
//!    export OPENROUTER_API_KEY=sk-or-...
//!    export OPENROUTER_MODEL=google/gemma-2-9b:free
//!    ```

use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::provider_v2::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
    Usage,
};
use crate::retry::extract_retry_after_ms;
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::time::Duration;

// Import macros exported at crate root
use crate::{build_request, get_api_key};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

#[derive(Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<OpenRouterMessage>,
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize, Deserialize, Default)]
struct OpenRouterMessage {
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    refusal: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(skip_serializing, default)]
    tool_calls: Option<Vec<OpenRouterToolCall>>,
}

/// Tool call from OpenRouter response (OpenAI-compatible format)
#[derive(Deserialize)]
struct OpenRouterToolCall {
    #[allow(dead_code)] // Kept for future use
    id: String,
    #[allow(dead_code)] // Kept for future use
    r#type: String,
    function: OpenRouterFunction,
}

/// Function call within a tool call
#[derive(Deserialize)]
struct OpenRouterFunction {
    name: String,
    arguments: String,
}

impl OpenRouterMessage {
    fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            refusal: None,
            reasoning: None,
            tool_calls: None,
        }
    }
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    choices: Vec<OpenRouterChoice>,
    usage: Option<OpenRouterUsage>,
    model: String,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    provider: Option<String>,
}

#[derive(Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterMessage,
    finish_reason: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    index: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    logprobs: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct OpenRouterUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// OpenRouter LLM provider (supports free and paid models)
pub struct OpenRouterProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    default_model: String,
}

impl OpenRouterProvider {
    pub fn new(config: ProviderConfig, default_model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        // Create HTTP/1.1-only client for OpenRouter compatibility
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .http1_only()
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .tcp_nodelay(true)
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to create HTTP client: {}", e))
            })?;

        Ok(Self {
            config,
            client,
            default_model,
        })
    }

    /// Create provider without config validation (for custom endpoints/proxies)
    pub fn new_without_validation(
        config: ProviderConfig,
        default_model: String,
    ) -> Result<Self, ProviderError> {
        // Skip validation - trust the provided config
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .http1_only()
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .tcp_nodelay(true)
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to create HTTP client: {}", e))
            })?;

        Ok(Self {
            config,
            client,
            default_model,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "openrouter".to_string(),
            display_name: "OpenRouter".to_string(),
            description: "Unified API for multiple LLM providers including free models".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your OpenRouter API key from openrouter.ai/keys".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("sk-or-...".to_string()),
                        default: None,
                        validation_pattern: Some("^sk-or-.*".to_string()),
                        validation_error: Some("API key must start with 'sk-or-'".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (defaults to OpenRouter)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://openrouter.ai/api/v1".to_string()),
                        default: Some("https://openrouter.ai/api/v1".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "OPENROUTER_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant.\n\n=== YOUR ROLE ===\n{context}\n\n=== RESPONSE GUIDELINES ===\n- Be direct and concise in your responses\n- Use bullet points or numbered lists when appropriate\n- When writing code, provide brief comments explaining complex logic\n- Focus on practical, actionable solutions\n- Validate assumptions before proceeding".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Be direct and concise in your responses.".to_string(),
                        "Use bullet points or numbered lists when appropriate.".to_string(),
                        "When writing code, provide brief comments explaining complex logic.".to_string(),
                        "Focus on practical, implementable solutions.".to_string(),
                    ],
                },
                tool_format: ToolFormat::OpenAIFunctionCalling,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: Some(128),
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![
                ModelInfo {
                    model_id: "google/gemma-2-9b:free".to_string(),
                    display_name: "Gemma 2 9B (Free)".to_string(),
                    description: "Free model with good quality and speed".to_string(),
                    context_window: 8192,
                    supports_tools: false,
                    use_cases: vec!["General tasks".to_string(), "Testing".to_string()],
                    cost_tier: 0,
                },
                ModelInfo {
                    model_id: "meta-llama/llama-3-8b:free".to_string(),
                    display_name: "Llama 3 8B (Free)".to_string(),
                    description: "Popular open-source model".to_string(),
                    context_window: 8192,
                    supports_tools: false,
                    use_cases: vec!["General tasks".to_string(), "Testing".to_string()],
                    cost_tier: 0,
                },
                ModelInfo {
                    model_id: "microsoft/phi-3-medium-128k:free".to_string(),
                    display_name: "Phi-3 Medium (Free)".to_string(),
                    description: "Large context window free model".to_string(),
                    context_window: 128_000,
                    supports_tools: false,
                    use_cases: vec!["Long context".to_string(), "Testing".to_string()],
                    cost_tier: 0,
                },
                ModelInfo {
                    model_id: "mistralai/mistral-7b:free".to_string(),
                    display_name: "Mistral 7B (Free)".to_string(),
                    description: "Good performance free model".to_string(),
                    context_window: 8192,
                    supports_tools: false,
                    use_cases: vec!["General tasks".to_string(), "Testing".to_string()],
                    cost_tier: 0,
                },
            ],
        }
    }

    pub fn endpoint(&self) -> String {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://openrouter.ai/api/v1");
        base.trim_end_matches('/').to_string()
    }
}

#[async_trait]
impl LLMProvider for OpenRouterProvider {
    fn name(&self) -> &'static str {
        "openrouter"
    }

    async fn is_available(&self) -> bool {
        self.config.api_key.is_some()
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Common OpenRouter models (as of March 2026)
        Ok(vec![
            // Free tier models
            "google/gemma-2-9b:free".to_string(),
            "meta-llama/llama-3-8b:free".to_string(),
            "meta-llama/llama-3.1-8b:free".to_string(),
            "microsoft/phi-3-medium-128k:free".to_string(),
            "mistralai/mistral-7b:free".to_string(),
            // Claude models
            "anthropic/claude-sonnet-4".to_string(),
            "anthropic/claude-opus-4".to_string(),
            "anthropic/claude-3.5-sonnet".to_string(),
            // OpenAI models
            "openai/gpt-4o".to_string(),
            "openai/gpt-4o-mini".to_string(),
            "openai/o1".to_string(),
            "openai/o1-mini".to_string(),
            "openai/o3-mini".to_string(),
            // Gemini models
            "google/gemini-2.5-pro".to_string(),
            "google/gemini-2.5-flash".to_string(),
            "google/gemini-pro-1.5".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = get_api_key!(self, "OPENROUTER_API_KEY")?;

        let url = format!("{}/chat/completions", self.endpoint());

        // Build messages array
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(OpenRouterMessage::new("system", system_prompt.clone()));
        }
        for msg in &request.messages {
            messages.push(OpenRouterMessage::new(
                msg.role.as_ref().to_string(),
                msg.content.to_text(),
            ));
        }

        let body = OpenRouterRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(false),
        };

        // Build request with per-request headers
        // OpenRouter requires HTTP-Referer and X-Title headers
        let req = build_request!(
            self.client.post(&url),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
                ("HTTP-Referer", "https://rustycode.ai"),
                ("X-Title", "RustyCode"),
            ],
            extra_headers = &self.config.extra_headers
        );

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 | 403 => ProviderError::auth(format!(
                    "Authentication failed. Check your OPENROUTER_API_KEY env var. {}",
                    text
                )),
                404 => ProviderError::InvalidModel(format!("model not found: {}", text)),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::network(format!(
                    "OpenRouter service temporarily unavailable ({}). Please retry in a few seconds.",
                    text
                )),
                _ => ProviderError::api(format!("{}: {}", status, text)),
            });
        }

        // Debug: capture response body for inspection
        let response_text = response
            .text()
            .await
            .map_err(|e| ProviderError::network(format!("failed to read response: {}", e)))?;

        tracing::debug!("OpenRouter response body: {}", response_text);

        let resp: OpenRouterResponse = serde_json::from_str(&response_text).map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;

        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::api("no choices in response"))?;

        // Debug: print message structure
        tracing::debug!(
            "OpenRouter message: role={}, content={:?}, refusal={:?}, reasoning={:?}",
            choice.message.role,
            choice.message.content,
            choice.message.refusal,
            choice.message.reasoning
        );

        // Build content string: handle refusal, content, reasoning, and tool calls
        let mut content = if let Some(refusal) = choice.message.refusal {
            refusal
        } else if let Some(c) = choice.message.content {
            c
        } else {
            choice.message.reasoning.unwrap_or_default()
        };

        // Append tool calls if present
        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let tool_calls_json: Vec<serde_json::Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": tc.r#type,
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments,
                            }
                        })
                    })
                    .collect();
                let formatted = serde_json::to_string_pretty(&tool_calls_json)
                    .unwrap_or_else(|_| "[]".to_string());
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(&format!("```tool\n{}\n```", formatted));
            }
        }

        if content.is_empty() {
            return Err(ProviderError::api(
                "message has no content, refusal, reasoning, or tool calls",
            ));
        }

        Ok(CompletionResponse {
            content,
            model: resp.model,
            usage: resp.usage.map(|u| Usage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            }),
            stop_reason: choice.finish_reason,
            citations: None,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| ProviderError::auth("OpenRouter API key is required. Set api_key in config or OPENROUTER_API_KEY env var"))?
            .expose_secret();

        let url = format!("{}/chat/completions", self.endpoint());

        // Build messages array
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(OpenRouterMessage::new("system", system_prompt.clone()));
        }
        for msg in &request.messages {
            messages.push(OpenRouterMessage::new(
                msg.role.as_ref().to_string(),
                msg.content.to_text(),
            ));
        }

        let body = OpenRouterRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(true),
        };

        // Build request with per-request headers
        let req = build_request!(
            self.client.post(&url),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
                ("HTTP-Referer", "https://rustycode.ai"),
                ("X-Title", "RustyCode"),
            ],
            extra_headers = &self.config.extra_headers
        );

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 | 403 => ProviderError::auth(format!(
                    "Authentication failed. Check your OPENROUTER_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!("model not found: {}", error_text)),
                429 => ProviderError::RateLimited { retry_delay: None },
                502..=504 => ProviderError::network(format!(
                    "OpenRouter service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::api(format!("{}: {}", status, error_text)),
            });
        }

        // Convert bytes stream to SSE stream
        let bytes_stream = response.bytes_stream();

        // Parse SSE events from byte stream
        let sse_stream = bytes_stream.map(|chunk_result| -> StreamChunk {
            let chunk = chunk_result.map_err(|e| ProviderError::Network(e.to_string()))?;
            let text = String::from_utf8_lossy(&chunk);
            let mut chunks = Vec::new();

            for line in text.lines() {
                if line.is_empty() {
                    continue;
                }
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    // OpenRouter sends "data: [DONE]" when complete
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

            Ok(crate::provider_v2::SSEEvent::Text {
                text: chunks.join(""),
            })
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
    fn test_creates_provider() {
        let config = make_config(Some("sk-or-test123"));
        let provider =
            OpenRouterProvider::new(config, "google/gemma-2-9b:free".to_string()).unwrap();
        assert_eq!(provider.name(), "openrouter");
        assert_eq!(provider.endpoint(), "https://openrouter.ai/api/v1");
    }

    #[test]
    fn test_default_endpoint() {
        let p = OpenRouterProvider::new(
            make_config(Some("sk-or-test")),
            "google/gemma-2-9b:free".to_string(),
        )
        .unwrap();
        assert_eq!(p.endpoint(), "https://openrouter.ai/api/v1");
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("sk-or-test"));
        config.base_url = Some("https://proxy.example.com/v1".to_string());
        let p = OpenRouterProvider::new(config, "google/gemma-2-9b:free".to_string()).unwrap();
        assert_eq!(p.endpoint(), "https://proxy.example.com/v1");
    }

    #[test]
    fn test_provider_name() {
        let p = OpenRouterProvider::new(
            make_config(Some("sk-or-test")),
            "google/gemma-2-9b:free".to_string(),
        )
        .unwrap();
        assert_eq!(p.name(), "openrouter");
    }
}
