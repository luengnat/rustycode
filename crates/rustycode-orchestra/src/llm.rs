// rustycode-orchestra/src/llm.rs
//! LLM integration for Orchestra v2
//!
//! Provides tool-call-aware task execution that:
//! - Sends tool schemas with the request so the LLM can invoke tools
//! - Parses `tool_use` content blocks from the response
//! - Returns structured `ToolCall` values ready for execution

use crate::{
    error::{OrchestraV2Error, Result},
    model_router::ModelSelection,
    request_dedup::{CachedResponse, DeduplicationConfig, RequestDeduplicator},
};
use rustycode_llm::{CompletionRequest, LLMProvider};
use rustycode_protocol::ToolCall;

// Re-export ChatMessage for use in other modules
pub use rustycode_llm::ChatMessage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

/// LLM client for Orchestra v2
#[derive(Clone)]
pub struct LlmClient {
    pub config: LlmConfig,
    deduplicator: Arc<RequestDeduplicator>,
}

impl LlmClient {
    /// Create a new LLM client with default deduplication config
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config,
            deduplicator: Arc::new(RequestDeduplicator::default()),
        }
    }

    /// Create a new LLM client with custom deduplication config
    pub fn with_dedup_config(config: LlmConfig, dedup_config: DeduplicationConfig) -> Self {
        Self {
            config,
            deduplicator: Arc::new(RequestDeduplicator::new(dedup_config)),
        }
    }

    /// Execute a task with the given model and messages
    pub async fn execute_task(
        &self,
        model_selection: &ModelSelection,
        messages: Vec<ChatMessage>,
        system_prompt: Option<String>,
    ) -> Result<TaskExecutionResult> {
        let start_time = Instant::now();

        // Serialize messages for hashing using JSON for stable serialization
        let messages_str =
            serde_json::to_string(&messages).unwrap_or_else(|_| format!("{:?}", messages));

        // Compute request hash for deduplication
        let request_hash = RequestDeduplicator::compute_hash(
            &messages_str,
            system_prompt.as_deref(),
            &model_selection.model,
        );

        // Check cache before sending request
        if let Ok(Some(cached)) = self.deduplicator.get_cached_response(&request_hash).await {
            tracing::info!(
                hash = %request_hash,
                tokens = cached.tokens_used,
                "Returning cached response (deduplication hit)"
            );
            return Ok(TaskExecutionResult {
                output: cached.response,
                tool_calls: Vec::new(),
                tokens_used: cached.tokens_used,
                duration_ms: 0, // Cache hit, no API call
            });
        }

        // Determine temperature based on tier
        let temperature = match model_selection.tier {
            crate::complexity::ModelTier::Quality => 0.1, // Low temp for consistent quality
            crate::complexity::ModelTier::Balanced => 0.3, // Medium temp for balance
            crate::complexity::ModelTier::Budget => 0.5,  // Higher temp for creative variety
        };

        // Build the completion request
        let mut request = CompletionRequest::new(model_selection.model.clone(), messages)
            .with_temperature(temperature)
            .with_max_tokens(self.config.max_tokens as u32);

        if let Some(ref system) = system_prompt {
            request = request.with_system_prompt(system.clone());
        }

        // Get provider from singleton
        let provider = rustycode_llm::singleton_provider::get_provider().map_err(|e| {
            OrchestraV2Error::LlmIntegration(format!("Failed to get provider: {}", e))
        })?;

        // Execute the request
        let response = provider
            .provider()
            .complete(request)
            .await
            .map_err(|e| OrchestraV2Error::LlmIntegration(format!("LLM call failed: {}", e)))?;

        let duration = start_time.elapsed();

        // Parse the response
        let tokens_used = response.usage.map(|u| u.total_tokens).unwrap_or(0);

        // Cache the response for future deduplication
        let cached_response = CachedResponse {
            response: response.content.clone(),
            tokens_used,
            finish_reason: None, // Could extract from response if available
            cached_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        if let Err(e) = self
            .deduplicator
            .cache_response(request_hash, cached_response)
            .await
        {
            tracing::warn!("Failed to cache response: {}", e);
        }

        Ok(TaskExecutionResult {
            output: response.content.clone(),
            tool_calls: parse_tool_calls_from_content(&response.content),
            tokens_used,
            duration_ms: duration.as_millis() as u64,
        })
    }
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub model_profile: ModelProfile,
    pub planning_temperature: f32,
    pub execution_temperature: f32,
    pub verification_temperature: f32,
    pub research_temperature: f32,
    pub max_tokens: usize,
    pub streaming: bool,
}

impl LlmClient {
    /// Execute an autonomous task (backward compatibility)
    pub async fn execute_autonomous_task(
        &self,
        _context: &crate::engine::TaskContext,
    ) -> Result<BackwardCompatTaskResult> {
        // TODO: Implement proper autonomous task execution
        Ok(BackwardCompatTaskResult {
            output: String::new(),
            tool_calls: Vec::new(),
        })
    }

    /// Execute a guided task (backward compatibility)
    pub async fn execute_guided_task(
        &self,
        _context: &crate::engine::TaskContext,
    ) -> Result<BackwardCompatTaskResult> {
        // TODO: Implement proper guided task execution
        Ok(BackwardCompatTaskResult {
            output: String::new(),
            tool_calls: Vec::new(),
        })
    }

    /// Get deduplication cache statistics
    pub async fn get_dedup_stats(&self) -> crate::request_dedup::CacheStats {
        self.deduplicator.cache_stats().await
    }

    /// Clear deduplication cache
    pub async fn clear_dedup_cache(&self) -> Result<()> {
        self.deduplicator
            .clear_cache()
            .await
            .map_err(|e| OrchestraV2Error::LlmIntegration(format!("Failed to clear cache: {}", e)))
    }

    /// Clean up expired cache entries
    pub async fn cleanup_expired_cache(&self) -> Result<usize> {
        self.deduplicator.cleanup_expired().await.map_err(|e| {
            OrchestraV2Error::LlmIntegration(format!("Failed to cleanup cache: {}", e))
        })
    }
}

/// Backward compatible task result (for engine.rs compatibility)
#[derive(Debug, Clone)]
pub struct BackwardCompatTaskResult {
    pub output: String,
    pub tool_calls: Vec<BackwardCompatToolCall>,
}

/// Backward compatible tool call
#[derive(Debug, Clone)]
pub struct BackwardCompatToolCall {
    pub tool_name: String,
    pub parameters: serde_json::Value,
}

/// Model profile for LLM selection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum ModelProfile {
    /// Quality mode - Opus everywhere except verification
    Quality,
    /// Balanced mode - Opus for planning, Sonnet for execution
    Balanced,
    /// Budget mode - Sonnet for writing, Haiku for research
    Budget,
}

/// Task execution result from LLM
#[derive(Debug, Clone)]
pub struct TaskExecutionResult {
    pub output: String,
    pub tool_calls: Vec<ToolCall>,
    pub tokens_used: u32,
    pub duration_ms: u64,
}

/// Parse tool calls embedded in LLM response content.
///
/// The Anthropic provider serialises `tool_use` content blocks as:
///
/// ```text
/// ```tool
/// [{"name": "write_file", "arguments": {"path": "...", "content": "..."}}]
/// ```
/// ```
///
/// This function extracts those JSON arrays and converts them to `ToolCall`
/// instances. Unknown or malformed entries are silently skipped.
fn parse_tool_calls_from_content(content: &str) -> Vec<ToolCall> {
    let mut tool_calls = Vec::new();

    // Find all ```tool\n...\n``` blocks
    let mut search_from = 0;
    while let Some(start) = content[search_from..].find("```tool\n") {
        let abs_start = search_from + start + "```tool\n".len();
        if let Some(end) = content[abs_start..].find("\n```") {
            let json_str = &content[abs_start..abs_start + end];

            // Parse as array of tool call objects
            if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(json_str) {
                for (i, item) in items.iter().enumerate() {
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let arguments = item
                        .get("arguments")
                        .cloned()
                        .unwrap_or(serde_json::Value::Object(Default::default()));
                    let call_id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&format!("tool_{}", i))
                        .to_string();

                    tool_calls.push(ToolCall::new(call_id, name.to_string(), arguments));
                }
            }

            search_from = abs_start + end + "\n```".len();
        } else {
            break;
        }
    }

    tool_calls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_content() {
        let calls = parse_tool_calls_from_content("");
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_text_only_response() {
        let calls = parse_tool_calls_from_content("Hello! I've completed the task.");
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_single_tool_call() {
        let content = r#"I'll create the file for you.

```tool
[{"name": "write_file", "arguments": {"path": "/tmp/test.txt", "content": "hello"}}]
```

Done!"#;

        let calls = parse_tool_calls_from_content(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(
            calls[0].arguments["path"],
            serde_json::json!("/tmp/test.txt")
        );
    }

    #[test]
    fn test_parse_multiple_tool_calls() {
        let content = r#"```tool
[{"name": "read_file", "arguments": {"path": "src/main.rs"}}, {"name": "bash", "arguments": {"command": "cargo build"}}]
```
"#;

        let calls = parse_tool_calls_from_content(content);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[1].name, "bash");
    }

    #[test]
    fn test_parse_tool_call_with_id() {
        let content = r#"```tool
[{"id": "toolu_abc123", "name": "bash", "arguments": {"command": "ls"}}]
```
"#;

        let calls = parse_tool_calls_from_content(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].call_id, "toolu_abc123");
    }

    #[test]
    fn test_parse_malformed_json_skipped() {
        let content = "```tool\nnot valid json\n```\n";
        let calls = parse_tool_calls_from_content(content);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_unclosed_block_ignored() {
        let content = "```tool\n[{\"name\": \"bash\"}]\nNo closing backticks";
        let calls = parse_tool_calls_from_content(content);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_multiple_tool_blocks() {
        let content = r#"First action:

```tool
[{"name": "read_file", "arguments": {"path": "a.rs"}}]
```

Now based on what I read:

```tool
[{"name": "write_file", "arguments": {"path": "b.rs", "content": "fn main() {}"}}]
```
"#;

        let calls = parse_tool_calls_from_content(content);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[1].name, "write_file");
    }
}
