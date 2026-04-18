use crate::provider_v2::{
    ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError,
    SSEEvent, StreamChunk, Usage,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use rustycode_litert::LitManager;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// Local LiteRT-LM provider.
///
/// This provider uses our own `rustycode-litert` crate for binary management,
/// process pooling, and inference, ensuring consistent cross-platform behaviour
/// and correct binary resolution (v0.10.2 from our installer).
pub struct LiteRtLmProvider {
    config: ProviderConfig,
    requested_model: String,
    model_source: String,
    runtime_model_name: String,
    manager: OnceCell<Arc<LitManager>>,
    model_ready: OnceCell<()>,
}

impl LiteRtLmProvider {
    pub fn new(config: ProviderConfig, requested_model: String) -> Result<Self, ProviderError> {
        let model_source = Self::model_source_for(&requested_model);
        let runtime_model_name = Self::runtime_model_name_for(&requested_model);

        Ok(Self {
            config,
            requested_model,
            model_source,
            runtime_model_name,
            manager: OnceCell::new(),
            model_ready: OnceCell::new(),
        })
    }

    fn is_url(value: &str) -> bool {
        let value = value.trim().to_ascii_lowercase();
        value.starts_with("http://") || value.starts_with("https://")
    }

    fn runtime_model_name_for(requested_model: &str) -> String {
        let model = requested_model.trim();
        if Self::is_url(model) {
            Path::new(model)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("litert-model")
                .to_string()
        } else if model.ends_with(".litertlm") {
            Path::new(model)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("litert-model")
                .to_string()
        } else {
            model.to_string()
        }
    }

    fn model_source_for(requested_model: &str) -> String {
        if let Ok(url) = std::env::var("LITERT_LM_MODEL_URL") {
            if !url.trim().is_empty() {
                return url;
            }
        }

        let model = requested_model.trim();
        if Self::is_url(model) {
            return model.to_string();
        }

        let model_lower = model.to_lowercase();
        if model_lower.contains("gemma-4") && model_lower.contains("e4b") {
            return "https://huggingface.co/litert-community/gemma-4-E4B-it-litert-lm/resolve/main/gemma-4-E4B-it.litertlm".to_string();
        }
        if model_lower.contains("gemma-4") && model_lower.contains("e2b") {
            return "https://huggingface.co/litert-community/gemma-4-E2B-it-litert-lm/resolve/main/gemma-4-E2B-it.litertlm".to_string();
        }
        if model_lower.contains("e4b") {
            return "https://huggingface.co/MiCkSoftware/gemma-3n-E4B-it-litert-lm/resolve/main/gemma-3n-E4B-it-int4-Web.litertlm".to_string();
        }

        if model_lower.ends_with(".litertlm") {
            return format!(
                "https://github.com/google-ai-edge/LiteRT-LM/releases/download/v0.10.2/{}",
                Path::new(model)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("gemma-3n-E4B.litertlm")
            );
        }

        format!(
            "https://github.com/google-ai-edge/LiteRT-LM/releases/download/v0.10.2/{}.litertlm",
            Self::slug_model_name(model)
        )
    }

    fn slug_model_name(value: &str) -> String {
        value
            .trim()
            .to_lowercase()
            .replace(['/', ' '], "-")
            .replace('_', "-")
    }

    fn build_prompt(request: &CompletionRequest) -> String {
        let mut sections = Vec::new();

        if let Some(system) = &request.system_prompt {
            if !system.trim().is_empty() {
                sections.push(format!("System:\n{}", system.trim()));
            }
        }

        for message in &request.messages {
            sections.push(Self::format_message(message));
        }

        sections.join("\n\n")
    }

    fn format_message(message: &ChatMessage) -> String {
        let role = match &message.role {
            crate::provider_v2::MessageRole::User => "User",
            crate::provider_v2::MessageRole::Assistant => "Assistant",
            crate::provider_v2::MessageRole::System => "System",
            crate::provider_v2::MessageRole::Tool(tool_name) => {
                return format!("Tool {}:\n{}", tool_name, message.text());
            }
        };

        format!("{}:\n{}", role, message.text())
    }

    async fn ensure_manager(&self) -> Result<Arc<LitManager>, ProviderError> {
        self.manager
            .get_or_try_init(|| async {
                LitManager::new()
                    .await
                    .map(Arc::new)
                    .map_err(|err| ProviderError::Configuration(err.to_string()))
            })
            .await
            .map(Arc::clone)
    }

    async fn ensure_model_ready(&self, manager: Arc<LitManager>) -> Result<(), ProviderError> {
        let source = self.model_source.clone();
        let alias = if source == self.runtime_model_name {
            None
        } else {
            Some(self.runtime_model_name.clone())
        };

        self.model_ready
            .get_or_try_init(|| async move {
                manager
                    .ensure_model(&source, alias.as_deref())
                    .await
                    .map_err(|err| ProviderError::Configuration(err.to_string()))
            })
            .await
            .map(|_| ())
    }

    pub fn supported_models() -> Vec<String> {
        vec![
            "gemma-4-e2b-it".to_string(),
            "gemma-4-e4b-it".to_string(),
            "gemma3-1b".to_string(),
            "gemma-3n-e2b".to_string(),
            "gemma-3n-e4b".to_string(),
            "phi-4-mini".to_string(),
            "qwen2.5-1.5b".to_string(),
            "functiongemma-270m".to_string(),
        ]
    }
}

#[async_trait]
impl LLMProvider for LiteRtLmProvider {
    fn name(&self) -> &'static str {
        "litert-lm"
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(Self::supported_models())
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let prompt = Self::build_prompt(&request);
        let manager = self.ensure_manager().await?;
        self.ensure_model_ready(Arc::clone(&manager)).await?;

        let timeout = std::time::Duration::from_secs(self.config.timeout_seconds.unwrap_or(120));
        let content = tokio::time::timeout(
            timeout,
            manager.run_completion(&self.runtime_model_name, &prompt),
        )
        .await
        .map_err(|_| {
            ProviderError::Timeout(format!(
                "LiteRT-LM completion timed out after {} seconds",
                timeout.as_secs()
            ))
        })?
        .map_err(|err| ProviderError::Unknown(err.to_string()))?;

        Ok(CompletionResponse {
            content,
            model: self.requested_model.clone(),
            usage: Some(Usage::new(0, 0)),
            stop_reason: Some("stop".to_string()),
            citations: None,
            thinking_blocks: None,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let prompt = Self::build_prompt(&request);
        let manager = self.ensure_manager().await?;
        self.ensure_model_ready(Arc::clone(&manager)).await?;

        let stream = manager
            .run_completion_stream(&self.runtime_model_name, &prompt)
            .await
            .map_err(|err| ProviderError::Unknown(err.to_string()))?
            .map(|chunk| match chunk {
                Ok(text) => Ok(SSEEvent::text(text)),
                Err(err) => Err(ProviderError::Unknown(err.to_string())),
            });

        Ok(Box::pin(stream))
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_v2::{ChatMessage, CompletionRequest, MessageRole};

    #[test]
    fn build_prompt_keeps_role_order() {
        let request = CompletionRequest {
            model: "gemma3-1b".to_string(),
            messages: vec![
                ChatMessage::system("Stay brief."),
                ChatMessage::user("Hello"),
                ChatMessage::assistant("Hi there"),
            ],
            max_tokens: None,
            temperature: None,
            stream: false,
            system_prompt: Some("Top-level system".to_string()),
            tools: None,
            extended_thinking: None,
            thinking_budget: None,
            effort: None,
            thinking: None,
            output_config: None,
        };

        let prompt = LiteRtLmProvider::build_prompt(&request);
        assert!(prompt.contains("System:\nTop-level system"));
        assert!(prompt.contains("User:\nHello"));
        assert!(prompt.contains("Assistant:\nHi there"));
    }

    #[test]
    fn format_tool_message_includes_tool_name() {
        let message = ChatMessage {
            role: MessageRole::Tool("bash".to_string()),
            content: "done".into(),
        };

        assert_eq!(
            LiteRtLmProvider::format_message(&message),
            "Tool bash:\ndone"
        );
    }

    #[test]
    fn slug_model_name_normalizes_delimiters() {
        assert_eq!(
            LiteRtLmProvider::slug_model_name("Gemma 3n_E4B"),
            "gemma-3n-e4b"
        );
    }

    #[test]
    fn runtime_model_name_for_urls_uses_filename_stem() {
        assert_eq!(
            LiteRtLmProvider::runtime_model_name_for("https://example.com/models/foo.litertlm"),
            "foo"
        );
    }
}
