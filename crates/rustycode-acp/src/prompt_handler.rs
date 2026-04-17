//! Prompt Handler - Process user prompts with LLM
//!
//! This module handles the actual processing of user messages,
//! integration with the LLM, and tool execution.

use crate::llm_integration::LLMIntegration;
use crate::tool_executor::ToolExecutor;
use crate::types::*;
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// Result from processing a prompt
#[derive(Debug)]
pub struct PromptResult {
    pub content: String,
    pub tool_calls: Option<Vec<ToolCallResult>>,
}

/// Tool call result
#[derive(Debug)]
pub struct ToolCallResult {
    pub id: String,
    pub name: String,
    pub input: Value,
    pub output: Value,
}

/// Advanced prompt handler with LLM integration
pub struct PromptHandler {
    llm: Arc<Mutex<LLMIntegration>>,
    tool_executor: Arc<Mutex<ToolExecutor>>,
}

impl PromptHandler {
    /// Create a new prompt handler
    pub fn new(cwd: String, model: String) -> Self {
        let llm = LLMIntegration::new(model);
        let tool_executor = ToolExecutor::new(cwd);

        Self {
            llm: Arc::new(Mutex::new(llm)),
            tool_executor: Arc::new(Mutex::new(tool_executor)),
        }
    }

    /// Initialize the handler (load LLM and tools)
    pub async fn initialize(&self) -> Result<()> {
        // Initialize LLM
        {
            let mut llm = self.llm.lock().await;
            llm.initialize().await?;
        }

        // Initialize tools
        {
            let mut tools = self.tool_executor.lock().await;
            tools.initialize().await?;
        }

        info!("Prompt handler initialized");
        Ok(())
    }

    /// Process a user prompt with full LLM integration
    pub async fn process_prompt(
        &self,
        session_id: &str,
        messages: &[PromptMessage],
        cwd: &str,
    ) -> Result<PromptResult> {
        info!("Processing prompt for session {}", session_id);

        // Get LLM response
        let llm_guard = self.llm.lock().await;
        let llm_available = llm_guard.is_available().await;
        drop(llm_guard);

        let content = if llm_available {
            // Use real LLM
            let llm_guard = self.llm.lock().await;
            llm_guard.process_messages(messages, None).await?
        } else {
            // Fallback to basic pattern matching
            self.process_fallback(messages, cwd).await?
        };

        // NOTE: Tool call parsing from LLM response not yet implemented
        // The LLM integration layer would need to:
        // 1. Parse tool calls in the format the LLM returns (Anthropic/OpenAI differ)
        // 2. Execute the tools via ToolExecutor
        // 3. Return results back to the LLM for continuation
        // For now, we return text-only responses
        let tool_calls = None;

        Ok(PromptResult {
            content,
            tool_calls,
        })
    }

    /// Fallback processing when LLM is not available
    async fn process_fallback(&self, messages: &[PromptMessage], cwd: &str) -> Result<String> {
        // Extract user message
        let user_message = messages
            .iter()
            .filter_map(|m| {
                if let PromptMessage::User { parts } = m {
                    parts.iter().find_map(|p| {
                        if let ContentPart::Text { text } = p {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let user_msg_lower = user_message.to_lowercase();

        debug!("Using fallback processing for: {}", user_message);

        // Pattern-based responses
        if user_msg_lower.contains("hello") || user_msg_lower.contains("hi ") {
            Ok("Hello! I'm RustyCode, your AI coding assistant. I can help you with:\n\n\
                  • Reading and writing files\n\
                  • Running commands\n\
                  • Searching code\n\
                  • Explaining code\n\
                  • Refactoring\n\n\
                  Note: LLM integration is not yet configured. Add an API key to enable full functionality.".to_string())
        } else if user_msg_lower.contains("help") {
            Ok("RustyCode Commands:\n\n\
                  • \"list files\" - List files in current directory\n\
                  • \"read <file>\" - Read a file\n\
                  • \"write <file> <content>\" - Write to a file\n\
                  • \"bash <command>\" - Run a shell command\n\
                  • \"grep <pattern>\" - Search for pattern\n\
                  \n\
                  For full functionality, configure an API key (Anthropic or OpenAI)."
                .to_string())
        } else if user_msg_lower.contains("list") && user_msg_lower.contains("file") {
            Ok(format!(
                "Files in {}:\n\n[File listing would be here - use 'bash ls' command]",
                cwd
            ))
        } else {
            Ok(format!("I received: {}\n\nNote: LLM not configured. Add API key for intelligent responses.", user_message))
        }
    }

    /// Check if handler is ready (LLM and tools available)
    pub async fn is_ready(&self) -> bool {
        let llm_guard = self.llm.lock().await;
        let tools_guard = self.tool_executor.lock().await;

        llm_guard.is_available().await && tools_guard.is_available().await
    }
}

impl Default for PromptHandler {
    fn default() -> Self {
        Self::new(".".to_string(), "claude-sonnet-4-6".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_handler_new() {
        let _handler = PromptHandler::new("/tmp/project".to_string(), "claude-3".to_string());
    }

    #[test]
    fn test_prompt_handler_default() {
        let _handler = PromptHandler::default();
    }

    #[tokio::test]
    async fn test_prompt_handler_not_ready_before_init() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        assert!(!handler.is_ready().await);
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_fallback_hello() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::User {
            parts: vec![ContentPart::Text {
                text: "hello there".to_string(),
            }],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp")
            .await;
        assert!(result.is_ok());
        let prompt_result = result.unwrap();
        assert!(prompt_result.content.contains("RustyCode"));
        assert!(prompt_result.tool_calls.is_none());
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_fallback_hi() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::User {
            parts: vec![ContentPart::Text {
                text: "hi everyone".to_string(),
            }],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp")
            .await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("RustyCode"));
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_fallback_help() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::User {
            parts: vec![ContentPart::Text {
                text: "help me".to_string(),
            }],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp")
            .await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("Commands") || content.contains("commands"));
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_fallback_list_files() {
        let handler = PromptHandler::new("/my/project".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::User {
            parts: vec![ContentPart::Text {
                text: "list files please".to_string(),
            }],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/my/project")
            .await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("/my/project"));
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_fallback_generic() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::User {
            parts: vec![ContentPart::Text {
                text: "explain this code".to_string(),
            }],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp")
            .await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        // Generic fallback echoes the message back
        assert!(content.contains("explain this code"));
        assert!(content.contains("not configured") || content.contains("LLM"));
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_no_user_messages() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::Assistant {
            parts: vec![ContentPart::Text {
                text: "I am assistant".to_string(),
            }],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp")
            .await;
        assert!(result.is_ok());
        // With no user text, it should still produce output (generic fallback)
        let content = result.unwrap().content;
        assert!(!content.is_empty());
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_case_insensitive() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::User {
            parts: vec![ContentPart::Text {
                text: "HELLO".to_string(),
            }],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp")
            .await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("RustyCode"));
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_mixed_parts() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::User {
            parts: vec![
                ContentPart::Text {
                    text: "hello".to_string(),
                },
                ContentPart::Tool {
                    name: "bash".to_string(),
                    input: Some(serde_json::json!({"command": "ls"})),
                },
            ],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_multiple_user_messages() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![
            PromptMessage::User {
                parts: vec![ContentPart::Text {
                    text: "first".to_string(),
                }],
            },
            PromptMessage::Assistant {
                parts: vec![ContentPart::Text {
                    text: "response".to_string(),
                }],
            },
            PromptMessage::User {
                parts: vec![ContentPart::Text {
                    text: "hello".to_string(),
                }],
            },
        ];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp")
            .await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("RustyCode"));
    }

    #[test]
    fn test_prompt_result_debug() {
        let result = PromptResult {
            content: "test".to_string(),
            tool_calls: None,
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_tool_call_result_debug() {
        let result = ToolCallResult {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"cmd": "ls"}),
            output: serde_json::json!({"stdout": "file.txt"}),
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("call-1"));
        assert!(debug_str.contains("bash"));
    }
}
