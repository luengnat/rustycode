// rustycode-orchestra/src/tools.rs
//! Tool execution for Orchestra v2

use crate::error::Result;
use rustycode_protocol::ToolCall;
use rustycode_tools::ToolExecutor as RustyCodeToolExecutor;
use std::path::PathBuf;
use std::sync::Arc;

/// Tool executor for Orchestra v2
#[derive(Clone)]
pub struct ToolExecutor {
    /// Inner tool executor from rustycode-tools (wrapped in Arc for sharing)
    inner: Arc<RustyCodeToolExecutor>,
    /// Current working directory
    cwd: PathBuf,
}

impl ToolExecutor {
    /// Create a new tool executor
    pub fn new(cwd: PathBuf) -> Self {
        let inner = Arc::new(RustyCodeToolExecutor::new(cwd.clone()));
        Self { inner, cwd }
    }

    /// Set the role for this executor
    pub fn with_role(mut self, role: rustycode_protocol::AgentRole) -> Self {
        // Since inner is Arc, we need to create a new inner with the role
        // if we want to follow the builder pattern for the wrapper.
        let inner = (*self.inner).clone().with_role(role);
        self.inner = Arc::new(inner);
        self
    }

    /// Set the plan gate for this executor
    pub fn with_plan_gate(mut self, gate: Arc<dyn rustycode_tools::gate::ToolGate>) -> Self {
        let inner = (*self.inner).clone().with_plan_gate(gate);
        self.inner = Arc::new(inner);
        self
    }

    /// Execute a tool call
    pub fn execute_tool(&self, tool_call: &ToolCall) -> Result<ToolExecutionResult> {
        // Execute the tool using the rustycode-tools executor
        let result = self.inner.execute_with_session(tool_call, None);

        Ok(ToolExecutionResult {
            output: result.output.clone(),
            error: result.error.clone(),
            success: result.is_success(),
        })
    }

    /// Execute multiple tool calls in sequence
    pub fn execute_tools(&self, tool_calls: &[ToolCall]) -> Result<Vec<ToolExecutionResult>> {
        let mut results = Vec::new();
        for tool_call in tool_calls {
            results.push(self.execute_tool(tool_call)?);
        }
        Ok(results)
    }

    /// Get the current working directory
    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }
}

/// Tool execution result
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// Output from the tool
    pub output: String,
    /// Error message if tool failed
    pub error: Option<String>,
    /// Whether the tool execution succeeded
    pub success: bool,
}

/// Create a new tool executor with the given working directory
pub fn create_tool_executor(cwd: PathBuf) -> ToolExecutor {
    ToolExecutor::new(cwd)
}
