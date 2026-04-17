//! LLM Integration - Bridge to rustycode-llm providers
//!
//! This module provides the integration between ACP and the actual LLM providers.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// LLM integration manager
pub struct LLMIntegration {
    provider: Arc<Mutex<Option<Box<dyn rustycode_llm::provider_v2::LLMProvider>>>>,
    default_model: String,
}

impl LLMIntegration {
    /// Create a new LLM integration
    pub fn new(default_model: String) -> Self {
        Self {
            provider: Arc::new(Mutex::new(None)),
            default_model,
        }
    }

    /// Initialize the LLM provider from config
    pub async fn initialize(&mut self) -> Result<()> {
        use rustycode_config::Config;
        use rustycode_llm::provider_v2::{LLMProvider, ProviderConfig};
        use secrecy::SecretString;

        // Load config from current directory
        let current_dir = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;

        let config = Config::load(&current_dir)
            .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

        // Get provider from model name (e.g., "claude-3-5-sonnet-20241022" -> anthropic)
        let (provider_name, _) = self.default_model.split_once('-').unwrap_or_else(|| {
            warn!(
                "Unable to parse model name '{}', defaulting to gpt-4",
                self.default_model
            );
            ("gpt", "4")
        });

        let provider: Option<Box<dyn LLMProvider>> = match provider_name {
            "claude" | "anthropic" => {
                // Get API key from config
                let api_key = config.providers.anthropic.and_then(|p| p.api_key);

                // Check if API key exists and is not empty
                let has_key = api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false);

                if !has_key {
                    info!("No Anthropic API key found, skipping LLM initialization");
                    return Ok(());
                }

                let provider_config = ProviderConfig {
                    api_key: api_key.map(|k| SecretString::new(k.into())),
                    base_url: None,
                    timeout_seconds: Some(120),
                    extra_headers: None,
                    retry_config: None,
                };

                match rustycode_llm::AnthropicProvider::new(
                    provider_config,
                    self.default_model.clone(),
                ) {
                    Ok(p) => Some(Box::new(p)),
                    Err(e) => {
                        error!("Failed to create Anthropic provider: {}", e);
                        None
                    }
                }
            }
            "gpt" | "openai" => {
                // Get API key from config
                let api_key = config.providers.openai.and_then(|p| p.api_key);

                // Check if API key exists and is not empty
                let has_key = api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false);

                if !has_key {
                    info!("No OpenAI API key found, skipping LLM initialization");
                    return Ok(());
                }

                let provider_config = ProviderConfig {
                    api_key: api_key.map(|k| SecretString::new(k.into())),
                    base_url: None,
                    timeout_seconds: Some(120),
                    extra_headers: None,
                    retry_config: None,
                };

                match rustycode_llm::OpenAiProvider::new(
                    provider_config,
                    self.default_model.clone(),
                ) {
                    Ok(p) => Some(Box::new(p)),
                    Err(e) => {
                        error!("Failed to create OpenAI provider: {}", e);
                        None
                    }
                }
            }
            _ => {
                info!("Unknown provider: {}, using mock responses", provider_name);
                None
            }
        };

        let has_provider = provider.is_some();
        *self.provider.lock().await = provider;

        if has_provider {
            info!("LLM provider initialized: {}", self.default_model);
        } else {
            info!("LLM provider not available, will use mock responses");
        }

        Ok(())
    }

    /// Process messages with the LLM
    pub async fn process_messages(
        &self,
        messages: &[crate::types::PromptMessage],
        _system_prompt: Option<&str>,
    ) -> Result<String> {
        use rustycode_llm::provider_v2::{ChatMessage, CompletionRequest};

        let provider_guard = self.provider.lock().await;

        let provider = match provider_guard.as_ref() {
            Some(p) => p,
            None => {
                // Return mock response if no provider available
                return Ok("I'm RustyCode, but LLM integration is not yet configured. Please add an API key to config.".to_string());
            }
        };

        // Convert ACP messages to LLM messages
        let llm_messages: Vec<ChatMessage> = messages
            .iter()
            .filter_map(|m| {
                if let crate::types::PromptMessage::User { parts } = m {
                    let text = parts
                        .iter()
                        .find_map(|p| {
                            if let crate::types::ContentPart::Text { text } = p {
                                Some(text.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    Some(ChatMessage::user(text))
                } else if let crate::types::PromptMessage::Assistant { parts } = m {
                    let text = parts
                        .iter()
                        .find_map(|p| {
                            if let crate::types::ContentPart::Text { text } = p {
                                Some(text.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    Some(ChatMessage::assistant(text))
                } else {
                    None
                }
            })
            .collect();

        if llm_messages.is_empty() {
            return Ok("I couldn't find any user messages to process.".to_string());
        }

        debug!("Processing {} messages with LLM", llm_messages.len());

        // Create completion request
        let request = CompletionRequest::new(self.default_model.clone(), llm_messages);

        // Get completion from provider
        let response = provider.complete(request).await?;

        Ok(response.content)
    }

    /// Check if LLM is available
    pub async fn is_available(&self) -> bool {
        self.provider.lock().await.is_some()
    }
}

impl Clone for LLMIntegration {
    fn clone(&self) -> Self {
        Self {
            provider: self.provider.clone(),
            default_model: self.default_model.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_integration_new() {
        let llm = LLMIntegration::new("claude-3-5-sonnet-20241022".to_string());
        assert_eq!(llm.default_model, "claude-3-5-sonnet-20241022");
    }

    #[test]
    fn test_llm_integration_new_openai_model() {
        let llm = LLMIntegration::new("gpt-4".to_string());
        assert_eq!(llm.default_model, "gpt-4");
    }

    #[test]
    fn test_llm_integration_new_custom_model() {
        let llm = LLMIntegration::new("my-custom-model".to_string());
        assert_eq!(llm.default_model, "my-custom-model");
    }

    #[test]
    fn test_llm_integration_clone() {
        let llm = LLMIntegration::new("claude-3".to_string());
        let cloned = llm.clone();
        assert_eq!(cloned.default_model, "claude-3");
    }

    #[tokio::test]
    async fn test_llm_not_available_before_init() {
        let llm = LLMIntegration::new("claude-3".to_string());
        assert!(!llm.is_available().await);
    }

    #[tokio::test]
    async fn test_process_messages_returns_fallback_without_provider() {
        let llm = LLMIntegration::new("claude-3".to_string());
        let messages = vec![crate::types::PromptMessage::User {
            parts: vec![crate::types::ContentPart::Text {
                text: "Hello".to_string(),
            }],
        }];
        let result = llm.process_messages(&messages, None).await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.is_empty());
        // Without a provider initialized, returns mock/fallback message
        assert!(content.contains("not yet configured"));
    }

    #[tokio::test]
    async fn test_process_messages_empty_messages_returns_fallback() {
        let llm = LLMIntegration::new("gpt-4".to_string());
        let messages = vec![];
        let result = llm.process_messages(&messages, None).await;
        assert!(result.is_ok());
        let content = result.unwrap();
        // Without a provider, returns the "not yet configured" message
        assert!(content.contains("not yet configured"));
    }

    #[tokio::test]
    async fn test_process_messages_with_assistant_only_returns_fallback() {
        let llm = LLMIntegration::new("gpt-4".to_string());
        let messages = vec![crate::types::PromptMessage::Assistant {
            parts: vec![crate::types::ContentPart::Text {
                text: "I said this".to_string(),
            }],
        }];
        let result = llm.process_messages(&messages, None).await;
        assert!(result.is_ok());
        // Without a provider, returns the "not yet configured" message
        let content = result.unwrap();
        assert!(content.contains("not yet configured"));
    }

    #[tokio::test]
    async fn test_process_messages_with_system_prompt_none() {
        let llm = LLMIntegration::new("claude-3".to_string());
        let messages = vec![crate::types::PromptMessage::User {
            parts: vec![crate::types::ContentPart::Text {
                text: "test".to_string(),
            }],
        }];
        // Should work the same whether system_prompt is None or Some
        let result = llm.process_messages(&messages, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_messages_with_system_prompt_some() {
        let llm = LLMIntegration::new("claude-3".to_string());
        let messages = vec![crate::types::PromptMessage::User {
            parts: vec![crate::types::ContentPart::Text {
                text: "test".to_string(),
            }],
        }];
        let result = llm.process_messages(&messages, Some("Be concise")).await;
        assert!(result.is_ok());
    }
}
