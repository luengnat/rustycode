use crate::provider_v2::{
    ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig,
    ProviderError, SSEEvent, StreamChunk, Usage,
};
use async_trait::async_trait;
use futures::Stream;
use rustycode_litert::{
    default_gemma_e4b_model_url, default_litert_lm_binary_url, default_litert_lm_install_dir,
    ensure_litert_lm_binary, ensure_litert_lm_runtime, LiteRtLmInstallConfig,
};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use tokio::process::Command;

/// Local LiteRT-LM provider.
///
/// This provider auto-installs the LiteRT-LM desktop binary and a Gemma E4B
/// model if they are not already present on disk.
pub struct LiteRtLmProvider {
    config: ProviderConfig,
    requested_model: String,
    binary_url: String,
    model_url: String,
    install_dir: PathBuf,
    binary_filename: String,
    model_filename: String,
    backend: String,
}

impl LiteRtLmProvider {
    pub fn new(config: ProviderConfig, requested_model: String) -> Result<Self, ProviderError> {
        let backend = std::env::var("LITERT_LM_BACKEND").unwrap_or_else(|_| "cpu".to_string());
        let install_dir = std::env::var("LITERT_LM_HOME")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_litert_lm_install_dir);

        let binary_url = std::env::var("LITERT_LM_BINARY_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(default_litert_lm_binary_url);

        let model_url = Self::model_url_for(&requested_model);
        let model_filename = Self::model_filename_for(&requested_model);

        Ok(Self {
            config,
            requested_model,
            binary_url,
            model_url,
            install_dir,
            binary_filename: "litert_lm_main".to_string(),
            model_filename,
            backend,
        })
    }

    fn model_filename_for(requested_model: &str) -> String {
        let model = requested_model.trim();
        if model.ends_with(".litertlm") {
            Path::new(model)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("gemma-3n-e4b.litertlm")
                .to_string()
        } else {
            format!("{}.litertlm", Self::slug_model_name(model))
        }
    }

    fn model_url_for(requested_model: &str) -> String {
        if let Ok(url) = std::env::var("LITERT_LM_MODEL_URL") {
            if !url.trim().is_empty() {
                return url;
            }
        }

        let model = requested_model.to_lowercase();
        if model.contains("e4b") {
            return default_gemma_e4b_model_url();
        }

        if model.ends_with(".litertlm") {
            return format!(
                "https://github.com/google-ai-edge/LiteRT-LM/releases/download/v0.10.2/{}",
                Path::new(requested_model)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("gemma-3n-e4b.litertlm")
            );
        }

        format!(
            "https://github.com/google-ai-edge/LiteRT-LM/releases/download/v0.10.2/{}.litertlm",
            Self::slug_model_name(requested_model)
        )
    }

    fn slug_model_name(value: &str) -> String {
        value
            .trim()
            .to_lowercase()
            .replace(['/', ' '], "-")
            .replace('_', "-")
    }

    fn runtime_install_config(&self) -> LiteRtLmInstallConfig {
        LiteRtLmInstallConfig {
            version: "v0.10.2".to_string(),
            binary_url: self.binary_url.clone(),
            model_url: self.model_url.clone(),
            install_dir: self.install_dir.clone(),
            binary_filename: self.binary_filename.clone(),
            model_filename: self.model_filename.clone(),
        }
    }

    async fn ensure_runtime(&self) -> Result<(PathBuf, PathBuf), ProviderError> {
        if Path::new(&self.requested_model).exists() {
            let binary_path = ensure_litert_lm_binary(&self.runtime_install_config())
                .await
                .map_err(|err| ProviderError::Configuration(err.to_string()))?;
            return Ok((binary_path, PathBuf::from(&self.requested_model)));
        }

        let result = ensure_litert_lm_runtime(&self.runtime_install_config())
            .await
            .map_err(|err| ProviderError::Configuration(err.to_string()))?;
        Ok((result.binary_path, result.model_path))
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

    fn build_args(&self, prompt: &str, model_path: &Path) -> Vec<String> {
        vec![
            "--backend".to_string(),
            self.backend.clone(),
            "--model_path".to_string(),
            model_path.to_string_lossy().to_string(),
            "--input_prompt".to_string(),
            prompt.to_string(),
        ]
    }

    async fn run_prompt(
        &self,
        prompt: &str,
        model_path: &Path,
        binary_path: &Path,
    ) -> Result<String, ProviderError> {
        let mut command = Command::new(binary_path);
        command
            .args(self.build_args(prompt, model_path))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let timeout = std::time::Duration::from_secs(self.config.timeout_seconds.unwrap_or(120));
        let output = tokio::time::timeout(timeout, command.output())
            .await
            .map_err(|_| {
                ProviderError::Timeout(format!(
                    "LiteRT-LM command timed out after {} seconds",
                    timeout.as_secs()
                ))
            })?
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    ProviderError::Configuration(format!(
                        "LiteRT-LM binary not found: {}",
                        binary_path.display()
                    ))
                } else {
                    ProviderError::Unknown(format!("Failed to run LiteRT-LM binary: {}", err))
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let details = if stderr.is_empty() {
                stdout
            } else if stdout.is_empty() {
                stderr
            } else {
                format!("{}; {}", stderr, stdout)
            };
            return Err(ProviderError::Unknown(format!(
                "LiteRT-LM command failed with status {}: {}",
                output.status, details
            )));
        }

        let stdout = String::from_utf8(output.stdout).map_err(|err| {
            ProviderError::Serialization(format!("LiteRT-LM output was not valid UTF-8: {}", err))
        })?;
        let stdout = stdout.trim().to_string();

        if stdout.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if !stderr.is_empty() {
                return Ok(stderr);
            }
        }

        Ok(stdout)
    }

    pub fn supported_models() -> Vec<String> {
        vec![
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
        let (binary_path, model_path) = self.ensure_runtime().await?;
        let content = self.run_prompt(&prompt, &model_path, &binary_path).await?;

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
        let response = self.complete(request).await?;
        let stream = futures::stream::iter(vec![Ok(SSEEvent::text(response.content))]);
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

        assert_eq!(LiteRtLmProvider::format_message(&message), "Tool bash:\ndone");
    }

    #[test]
    fn slug_model_name_normalizes_delimiters() {
        assert_eq!(LiteRtLmProvider::slug_model_name("Gemma 3n_E4B"), "gemma-3n-e4b");
    }
}
