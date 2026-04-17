//! Code agent — uses an LLM to solve benchmark tasks with bash tool access.
//!
//! Supports multiple LLM providers via `rustycode-llm`. The tool-use loop:
//! 1. Send task instruction + bash tool schema to the LLM
//! 2. Parse `tool_use` content blocks from the response
//! 3. Execute each bash command in the benchmark container
//! 4. Feed tool results back as messages
//! 5. Repeat until no more tool calls or `max_turns` reached

use std::sync::Arc;

use super::BenchAgent;
use crate::environment::BenchEnvironment;

/// Configuration for the code agent.
#[derive(Debug, Clone)]
pub struct CodeAgentConfig {
    /// Model to use (e.g. "claude-sonnet-4-6", "gpt-4o").
    pub model: String,
    /// LLM provider name: "anthropic", "openai", "gemini", "ollama", etc.
    pub provider: String,
    /// Maximum number of tool-use turns.
    pub max_turns: usize,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
    /// System prompt for the agent.
    pub system_prompt: String,
    /// Timeout for each command execution in seconds.
    pub command_timeout_secs: u64,
}

impl Default for CodeAgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_string(),
            provider: "anthropic".to_string(),
            max_turns: 30,
            max_tokens: 16_384,
            system_prompt: "You are an expert software engineer solving a programming task inside a container.\n\
                You have access to a bash shell. Execute commands to solve the task.\n\
                Read files, write code, install packages, run scripts as needed.\n\
                When you believe the task is complete, stop making tool calls."
                .to_string(),
            command_timeout_secs: 300,
        }
    }
}

/// Agent that uses an LLM to solve benchmark tasks with bash tool access.
///
/// Supports multiple providers via `rustycode-llm`:
/// - **Anthropic**: Claude Sonnet, Opus, Haiku (`provider: "anthropic"`)
/// - **`OpenAI`**: GPT-4o, GPT-4 (`provider: "openai"`)
/// - **Google**: Gemini 2.5 Pro (`provider: "gemini"`)
/// - **Ollama**: Local models (`provider: "ollama"`)
/// - **`OpenRouter`**: Multi-provider proxy (`provider: "openrouter"`)
///
/// Creates the provider from `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.
pub struct CodeAgent {
    config: CodeAgentConfig,
    provider: Arc<dyn rustycode_llm::LLMProvider>,
}

impl CodeAgent {
    /// Create a new code agent with a specific provider.
    #[must_use]
    pub fn new(config: CodeAgentConfig, provider: Arc<dyn rustycode_llm::LLMProvider>) -> Self {
        Self { config, provider }
    }

    /// Create a code agent using the default Anthropic provider.
    ///
    /// Reads `ANTHROPIC_API_KEY` from the environment.
    pub fn with_anthropic(config: CodeAgentConfig) -> anyhow::Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

        let provider_config = rustycode_llm::ProviderConfig {
            api_key: Some(secrecy::SecretString::new(api_key.into())),
            base_url: std::env::var("ANTHROPIC_BASE_URL").ok(),
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };

        let provider =
            rustycode_llm::AnthropicProvider::new(provider_config, config.model.clone())?;

        Ok(Self {
            config,
            provider: Arc::new(provider),
        })
    }

    /// Create a code agent using the `OpenAI` provider.
    ///
    /// Reads `OPENAI_API_KEY` from the environment.
    pub fn with_openai(config: CodeAgentConfig) -> anyhow::Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;

        let provider_config = rustycode_llm::ProviderConfig {
            api_key: Some(secrecy::SecretString::new(api_key.into())),
            base_url: std::env::var("OPENAI_BASE_URL").ok(),
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };

        let provider = rustycode_llm::OpenAiProvider::new(provider_config, config.model.clone())?;

        Ok(Self {
            config,
            provider: Arc::new(provider),
        })
    }

    /// Create a code agent auto-detected from the config's provider field.
    ///
    /// Supports: "anthropic", "openai", "gemini", "ollama"
    pub fn auto(config: CodeAgentConfig) -> anyhow::Result<Self> {
        match config.provider.as_str() {
            "anthropic" | "claude" => Self::with_anthropic(config),
            "openai" | "gpt" => Self::with_openai(config),
            other => {
                anyhow::bail!("Unsupported provider: '{other}'. Supported: anthropic, openai")
            }
        }
    }

    /// Bash tool schema for Anthropic-style tool-use.
    fn bash_tool_schema() -> serde_json::Value {
        serde_json::json!({
            "name": "bash",
            "description": "Execute a bash command in the container.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The bash command to execute"
                    }
                },
                "required": ["command"]
            }
        })
    }

    /// Parse `tool_use` blocks from an Anthropic response content string.
    fn parse_tool_uses(content: &str) -> Vec<ToolUse> {
        let mut tool_uses = Vec::new();

        if let Ok(blocks) = serde_json::from_str::<Vec<serde_json::Value>>(content) {
            for block in &blocks {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let command = block
                        .get("input")
                        .and_then(|i| i.get("command"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();

                    if !command.is_empty() {
                        tool_uses.push(ToolUse { id, name, command });
                    }
                }
            }
        }

        tool_uses
    }

    /// Extract text content blocks from response.
    fn extract_text(content: &str) -> String {
        serde_json::from_str::<Vec<serde_json::Value>>(content).map_or_else(
            |_| content.to_string(),
            |blocks| {
                blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        )
    }
}

/// A parsed `tool_use` block from the LLM response.
struct ToolUse {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    name: String,
    command: String,
}

#[async_trait::async_trait]
impl BenchAgent for CodeAgent {
    fn name(&self) -> &'static str {
        "code"
    }

    async fn setup(&mut self, _env: &mut dyn BenchEnvironment) -> anyhow::Result<()> {
        Ok(())
    }

    async fn run(
        &mut self,
        instruction: &str,
        env: &mut dyn BenchEnvironment,
    ) -> anyhow::Result<()> {
        let tools = vec![Self::bash_tool_schema()];

        // Build initial messages
        let mut messages = vec![rustycode_llm::ChatMessage::user(instruction.to_string())];

        for turn in 0..self.config.max_turns {
            let request =
                rustycode_llm::CompletionRequest::new(&self.config.model, messages.clone())
                    .with_system_prompt(self.config.system_prompt.clone())
                    .with_max_tokens(self.config.max_tokens)
                    .with_tools(tools.clone());

            tracing::info!(
                "[code] Turn {}/{} (provider: {})",
                turn + 1,
                self.config.max_turns,
                self.provider.name()
            );

            let response = rustycode_llm::LLMProvider::complete(&*self.provider, request).await?;

            let text = Self::extract_text(&response.content);
            if !text.is_empty() {
                tracing::info!("[code] LLM: {}", truncate(&text, 200));
            }

            // Parse tool_use blocks from response
            let tool_uses = Self::parse_tool_uses(&response.content);

            if tool_uses.is_empty() {
                tracing::info!("[code] No more tool calls — agent finished");
                break;
            }

            // Add assistant message with content blocks
            messages.push(rustycode_llm::ChatMessage::assistant(
                response.content.clone(),
            ));

            // Execute each tool call
            for tool_use in &tool_uses {
                tracing::info!("[code] Executing: {}", truncate(&tool_use.command, 100));

                let result = env
                    .exec_with_timeout(&tool_use.command, self.config.command_timeout_secs)
                    .await;

                let output = match result {
                    Ok(r) => {
                        let stdout = r.stdout.trim();
                        let stderr = r.stderr.trim();
                        let mut out = String::new();
                        if !stdout.is_empty() {
                            out.push_str(stdout);
                        }
                        if !stderr.is_empty() {
                            if !out.is_empty() {
                                out.push('\n');
                            }
                            out.push_str("STDERR: ");
                            out.push_str(stderr);
                        }
                        if out.is_empty() {
                            out = "(no output)".to_string();
                        }
                        out
                    }
                    Err(e) => format!("ERROR: {e}"),
                };

                tracing::info!("[code] Output: {}", truncate(&output, 200));

                let tool_result_msg =
                    rustycode_llm::ChatMessage::tool_result(output, tool_use.id.clone());
                messages.push(tool_result_msg);
            }
        }

        Ok(())
    }
}

/// Truncate a string for logging.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{truncated}...")
    }
}
