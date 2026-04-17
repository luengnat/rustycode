use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::provider_v2::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
    Usage,
};
use crate::retry::extract_retry_after_ms;
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

const ZHIPU_DEFAULT_ENDPOINT: &str = "https://api.z.ai/api/coding/paas/v4";

#[derive(Serialize)]
struct ZhipuRequest {
    model: String,
    messages: Vec<ZhipuMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ZhipuMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ZhipuResponse {
    choices: Vec<ZhipuChoice>,
    usage: Option<ZhipuUsage>,
    model: String,
}

#[derive(Deserialize)]
struct ZhipuChoice {
    message: ZhipuResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ZhipuResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ZhipuToolCall>>,
}

#[derive(Deserialize, Serialize, Clone)]
struct ZhipuToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: ZhipuFunction,
}

#[derive(Deserialize, Serialize, Clone)]
struct ZhipuFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct ZhipuUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

pub struct ZhipuProvider {
    config: ProviderConfig,
    client: Client,
}

impl ZhipuProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;

        let timeout_secs = config.timeout_seconds.unwrap_or(300);
        Ok(Self {
            config,
            client: Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .connect_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
        })
    }

    fn endpoint(&self) -> String {
        self.config.base_url.clone().unwrap_or_else(|| ZHIPU_DEFAULT_ENDPOINT.to_string())
    }

    fn get_api_key(&self) -> Result<String, ProviderError> {
        self.config.api_key.as_ref()
            .ok_or_else(|| ProviderError::auth("ZHIPU_API_KEY is required"))
            .map(|k| k.expose_secret().to_string())
    }

    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "zhipu".to_string(),
            display_name: "Zhipu AI".to_string(),
            description: "GLM models from Zhipu AI".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![ConfigField {
                    name: "api_key".to_string(),
                    label: "API Key".to_string(),
                    description: "Your Zhipu AI API key from z.ai".to_string(),
                    field_type: ConfigFieldType::APIKey,
                    placeholder: Some("...".to_string()),
                    default: None,
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: true,
                }],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "API endpoint (defaults to https://api.z.ai/api/paas/v4)".to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some(ZHIPU_DEFAULT_ENDPOINT.to_string()),
                    default: Some(ZHIPU_DEFAULT_ENDPOINT.to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "ZHIPU_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant powered by GLM.".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![],
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
                    model_id: "glm-5".to_string(),
                    display_name: "GLM-5".to_string(),
                    description: "Latest flagship model with agentic capabilities".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["Complex reasoning".to_string(), "Coding".to_string(), "Agent workflows".to_string()],
                    cost_tier: 3,
                },
                ModelInfo {
                    model_id: "glm-4-plus".to_string(),
                    display_name: "GLM-4 Plus".to_string(),
                    description: "High-capability GLM-4 model".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["General tasks".to_string(), "Coding".to_string()],
                    cost_tier: 2,
                },
                ModelInfo {
                    model_id: "glm-4-flash".to_string(),
                    display_name: "GLM-4 Flash".to_string(),
                    description: "Fast, cost-effective GLM-4 model".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["Quick tasks".to_string(), "High-volume workloads".to_string()],
                    cost_tier: 1,
                },
            ],
        }
    }
}

#[async_trait]
impl LLMProvider for ZhipuProvider {
    fn name(&self) -> &'static str {
        "zhipu"
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/models", self.endpoint());
        match self.client.get(&url)
            .header("Authorization", format!("Bearer {}", self.get_api_key().unwrap_or_default()))
            .send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let url = format!("{}/models", self.endpoint());
        let req = self.client.get(&url)
            .header("Authorization", format!("Bearer {}", self.get_api_key()?));
        let response = req.send().await.map_err(|e| {
            ProviderError::Network(format!("Failed to connect to Zhipu: {}", e))
        })?;
        if !response.status().is_success() {
            return Err(ProviderError::Api(format!("Zhipu API returned status {}", response.status())));
        }
        #[derive(Deserialize)]
        struct ZhipuModelsResponse { data: Vec<ZhipuModel> }
        #[derive(Deserialize)]
        struct ZhipuModel { id: String }
        let models: ZhipuModelsResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;
        Ok(models.data.into_iter().map(|m| m.id).collect())
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let api_key = self.get_api_key()?;
        let url = format!("{}/chat/completions", self.endpoint());
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(ZhipuMessage { role: "system".to_string(), content: system_prompt.clone() });
        }
        for msg in &request.messages {
            let role = match msg.role.as_ref() {
                "user" => "user", "assistant" => "assistant", "system" => "system", "tool" => "tool", _ => "user",
            };
            messages.push(ZhipuMessage { role: role.to_string(), content: msg.content.to_text() });
        }
        let body = ZhipuRequest {
            model: request.model.clone(),
            messages,
            stream: false,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools: request.tools.map(|t| serde_json::json!(t)),
        };
        let req = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json");
        let response = req.json(&body).send().await.map_err(|e| {
            ProviderError::network(format!("failed to send request: {}", e))
        })?;
        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response.text().await.unwrap_or_else(|_| "unable to read error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::auth(format!("Authentication failed: {}", error_text)),
                404 => ProviderError::InvalidModel(format!("model not found: {}", error_text)),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!("Zhipu service unavailable: {}", error_text)),
                _ => ProviderError::api(format!("{}: {}", status, error_text)),
            });
        }
        let resp: ZhipuResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;
        let choice = resp.choices.into_iter().next()
            .ok_or_else(|| ProviderError::api("no choices in response"))?;
        let mut content = choice.message.content.unwrap_or_default();
        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let tool_calls_json: Vec<serde_json::Value> = tool_calls.iter().map(|tc| {
                    serde_json::json!({"id": tc.id, "type": tc.tool_type, "function": {"name": tc.function.name, "arguments": tc.function.arguments}})
                }).collect();
                let formatted = serde_json::to_string_pretty(&tool_calls_json).unwrap_or_else(|_| "[]".to_string());
                if !content.is_empty() { content.push('\n'); }
                content.push_str(&format!("```tool\n{}\n```", formatted));
            }
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

    async fn complete_stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let api_key = self.get_api_key()?;
        let url = format!("{}/chat/completions", self.endpoint());
        let mut messages = Vec::new();
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(ZhipuMessage { role: "system".to_string(), content: system_prompt.clone() });
        }
        for msg in &request.messages {
            let role = match msg.role.as_ref() {
                "user" => "user", "assistant" => "assistant", "system" => "system", "tool" => "tool", _ => "user",
            };
            messages.push(ZhipuMessage { role: role.to_string(), content: msg.content.to_text() });
        }
        let body = ZhipuRequest {
            model: request.model.clone(),
            messages,
            stream: true,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            tools: request.tools.map(|t| serde_json::json!(t)),
        };
        let req = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json");
        let response = req.json(&body).send().await.map_err(|e| {
            ProviderError::network(format!("failed to send request: {}", e))
        })?;
        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response.text().await.unwrap_or_else(|_| "unable to read error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::auth(format!("Authentication failed: {}", error_text)),
                404 => ProviderError::InvalidModel(format!("model not found: {}", error_text)),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!("Zhipu service unavailable: {}", error_text)),
                _ => ProviderError::api(format!("{}: {}", status, error_text)),
            });
        }
        let bytes_stream = response.bytes_stream();
        let done_sent = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sse_stream = bytes_stream.flat_map(move |chunk_result| {
            let done_sent = done_sent.clone();
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => return futures::stream::iter(vec![Err(ProviderError::Network(e.to_string()))]),
            };
            let text = String::from_utf8_lossy(&chunk);
            let mut events = Vec::new();
            for line in text.lines() {
                if line.is_empty() { continue; }
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if json_str == "[DONE]" {
                        if !done_sent.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            events.push(Ok(crate::provider_v2::SSEEvent::MessageStop));
                        }
                        continue;
                    }
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content") {
                                        if let Some(content_str) = content.as_str() {
                                            if !content_str.is_empty() {
                                                events.push(Ok(crate::provider_v2::SSEEvent::Text { text: content_str.to_string() }));
                                            }
                                        }
                                    }
                                }
                                if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                                    let usage = data.get("usage").and_then(|u| {
                                        let input_tokens = u.get("prompt_tokens")?.as_u64()? as u32;
                                        let output_tokens = u.get("completion_tokens")?.as_u64()? as u32;
                                        Some(Usage { input_tokens, output_tokens, total_tokens: input_tokens + output_tokens, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 })
                                    });
                                    events.push(Ok(crate::provider_v2::SSEEvent::MessageDelta { stop_reason: Some(finish_reason.to_string()), usage }));
                                    if !done_sent.swap(true, std::sync::atomic::Ordering::SeqCst) {
                                        events.push(Ok(crate::provider_v2::SSEEvent::MessageStop));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            futures::stream::iter(events)
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
    #[test]
    fn test_zhipu_provider_creation() {
        let config = ProviderConfig {
            api_key: Some(SecretString::new("test-key".into())),
            ..Default::default()
        };
        let provider = ZhipuProvider::new(config);
        assert!(provider.is_ok());
    }
    #[test]
    fn test_metadata_display_name() {
        let metadata = ZhipuProvider::metadata();
        assert_eq!(metadata.display_name, "Zhipu AI");
        assert_eq!(metadata.provider_id, "zhipu");
    }
    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = ZhipuProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
    }
}
