//! Tool Executor - Bridge to rustycode-tools
//!
//! This module handles tool execution for ACP requests.

use anyhow::Result;
use rustycode_protocol::ToolCall;
use rustycode_tools::ToolContext;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Tool executor
pub struct ToolExecutor {
    tool_registry: Arc<Mutex<Option<rustycode_tools::ToolRegistry>>>,
    cwd: PathBuf,
}

impl ToolExecutor {
    /// Create a new tool executor
    pub fn new(cwd: String) -> Self {
        Self {
            tool_registry: Arc::new(Mutex::new(None)),
            cwd: PathBuf::from(cwd),
        }
    }

    /// Initialize the tool registry
    pub async fn initialize(&mut self) -> Result<()> {
        use rustycode_tools::ToolRegistry;

        let registry = ToolRegistry::new();

        // Register common tools
        // Note: Tools are discovered automatically by the registry

        *self.tool_registry.lock().await = Some(registry);

        info!("Tool executor initialized for cwd: {:?}", self.cwd);
        Ok(())
    }

    /// Execute a tool call
    pub async fn execute_tool(&self, tool_name: &str, tool_input: Value) -> Result<Value> {
        let registry_guard = self.tool_registry.lock().await;

        let registry = match registry_guard.as_ref() {
            Some(r) => r,
            None => {
                // Return mock response if no registry available
                warn!("Tool registry not available, returning mock response");
                let mut map = Map::new();
                map.insert("status".to_string(), Value::String("mock".to_string()));
                map.insert(
                    "message".to_string(),
                    Value::String(format!(
                        "Tool '{}' not available (registry not initialized)",
                        tool_name
                    )),
                );
                map.insert(
                    "output".to_string(),
                    Value::String(format!("Mock output for {}", tool_name)),
                );
                return Ok(Value::Object(map));
            }
        };

        debug!("Executing tool: {} with input: {}", tool_name, tool_input);

        // Map ACP tool names to rustycode tool names
        let mapped_name = tool_name;

        // Create tool call
        let call = ToolCall::with_generated_id(mapped_name, tool_input);

        // Create tool context
        let ctx = ToolContext::new(&self.cwd);

        // Execute the tool
        let result = registry.execute(&call, &ctx);

        if result.success {
            info!("Tool {} executed successfully", tool_name);
            // Return structured output
            let mut map = Map::new();
            map.insert("output".to_string(), Value::String(result.output));
            if let Some(data) = result.data {
                map.insert("data".to_string(), data);
            }
            Ok(Value::Object(map))
        } else {
            error!("Tool {} failed: {:?}", tool_name, result.error);
            let mut map = Map::new();
            if let Some(error_msg) = result.error {
                map.insert("error".to_string(), Value::String(error_msg));
            }
            map.insert("output".to_string(), Value::String(result.output));
            Ok(Value::Object(map))
        }
    }

    /// Check if tool is available
    pub async fn is_available(&self) -> bool {
        self.tool_registry.lock().await.is_some()
    }
}

impl Clone for ToolExecutor {
    fn clone(&self) -> Self {
        Self {
            tool_registry: self.tool_registry.clone(),
            cwd: self.cwd.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_executor_new() {
        let executor = ToolExecutor::new("/tmp/project".to_string());
        assert_eq!(executor.cwd, PathBuf::from("/tmp/project"));
    }

    #[test]
    fn test_tool_executor_new_empty_cwd() {
        let executor = ToolExecutor::new(String::new());
        assert_eq!(executor.cwd, PathBuf::from(""));
    }

    #[test]
    fn test_tool_executor_new_dot_cwd() {
        let executor = ToolExecutor::new(".".to_string());
        assert_eq!(executor.cwd, PathBuf::from("."));
    }

    #[test]
    fn test_tool_executor_clone() {
        let executor = ToolExecutor::new("/test".to_string());
        let cloned = executor.clone();
        assert_eq!(cloned.cwd, executor.cwd);
    }

    #[tokio::test]
    async fn test_tool_executor_not_available_before_init() {
        let executor = ToolExecutor::new("/tmp".to_string());
        assert!(!executor.is_available().await);
    }

    #[tokio::test]
    async fn test_tool_executor_mock_response_without_init() {
        let executor = ToolExecutor::new("/tmp".to_string());
        let result = executor
            .execute_tool("bash", serde_json::json!({"command": "ls"}))
            .await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["status"], "mock");
        assert!(val["message"].as_str().unwrap().contains("bash"));
    }

    #[tokio::test]
    async fn test_tool_executor_mock_response_includes_tool_name() {
        let executor = ToolExecutor::new("/tmp".to_string());
        let result = executor
            .execute_tool("read_file", serde_json::json!({"path": "/etc/hosts"}))
            .await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val["message"].as_str().unwrap().contains("read_file"));
        assert!(val["output"].as_str().unwrap().contains("read_file"));
    }

    #[tokio::test]
    async fn test_tool_executor_initialize_sets_available() {
        let mut executor = ToolExecutor::new("/tmp".to_string());
        let result = executor.initialize().await;
        // initialize should succeed (tools registry is created)
        assert!(result.is_ok());
        assert!(executor.is_available().await);
    }
}
