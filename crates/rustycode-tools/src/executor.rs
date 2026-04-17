//! Unified tool executor with plan mode and hook gating
//!
//! This module implements a unified execution pipeline that wires together:
//! - Plan mode enforcement (read-only planning phase)
//! - Hook execution (pre/post tool lifecycle with blocking)
//! - Checkpoint creation (before destructive operations)
//! - Cost tracking integration points
//! - Rewind recording integration points
//!
//! # Execution Pipeline
//!
//! The `execute_tool` method implements a 7-step pipeline:
//!
//! 1. Check PlanMode to see if tool is allowed in current phase
//! 2. Create checkpoint before destructive operations (edit, write, bash)
//! 3. Run PreToolUse hooks, block if any hook blocks
//! 4. Execute the actual tool
//! 5. Run PostToolUse hooks
//! 6. Record cost if LLM was used
//! 7. Record interaction for rewind
//!
//! # Blocking Semantics
//!
//! - Plan mode violations block execution immediately
//! - PreToolUse hooks can block execution (block_reason returned)
//! - PostToolUse hooks run but cannot block (audit only)

use crate::hooks::{HookManager, HookTrigger};
use crate::workspace_checkpoint::CheckpointManager;
use crate::ToolOutput;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Metadata field key for LLM cost in tool output
const COST_METADATA_KEY: &str = "llm_cost";

/// Single API call record for cost tracking
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiCall {
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tool_name: Option<String>,
}

/// Trait for cost tracking to avoid circular dependencies
/// Implemented by rustycode_llm::cost_tracker::CostTracker
pub trait CostTrackerProvider: Send + Sync {
    /// Record an LLM API call
    fn record_call(&self, call: ApiCall) -> Result<()>;
}

/// Trait for plan mode checking to avoid circular dependencies
/// Implemented by rustycode_orchestra::plan_mode::PlanMode
pub trait PlanModeProvider {
    /// Check if a tool is allowed in the current execution phase
    fn is_tool_allowed(&self, tool: &str) -> Result<(), String>;
    /// Check if edit_file should run in dry-run mode
    fn is_edit_dry_run(&self) -> bool;
    /// Get current execution phase as a string
    fn current_phase(&self) -> String;
}

/// Unified tool executor with plan mode and hook gating
pub struct UnifiedToolExecutor {
    checkpoint_manager: Arc<CheckpointManager>,
    hook_manager: Arc<HookManager>,
    cost_tracker: Arc<dyn CostTrackerProvider>,
}

impl UnifiedToolExecutor {
    /// Create a new unified tool executor
    pub fn new(
        checkpoint_manager: Arc<CheckpointManager>,
        hook_manager: Arc<HookManager>,
        cost_tracker: Arc<dyn CostTrackerProvider>,
    ) -> Self {
        Self {
            checkpoint_manager,
            hook_manager,
            cost_tracker,
        }
    }

    /// Execute a tool with full gating and lifecycle management
    ///
    /// Implements the 7-step execution pipeline:
    /// 1. Plan mode check (blocking)
    /// 2. Checkpoint creation (destructive tools only)
    /// 3. PreToolUse hooks (can block)
    /// 4. Tool execution
    /// 5. PostToolUse hooks (audit only)
    /// 6. Cost tracking
    /// 7. Rewind recording
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        args: Value,
        plan_mode: &dyn PlanModeProvider,
    ) -> Result<ToolOutput> {
        info!(
            "Executing tool '{}' ({} bytes of args)",
            tool_name,
            args.to_string().len()
        );

        // Step 1: Check plan mode — block if tool not allowed in current phase
        if let Err(e) = plan_mode.is_tool_allowed(tool_name) {
            let blocked_msg = format!("Tool execution blocked: {}", e);
            error!("{}", blocked_msg);
            return Err(anyhow!("{}", blocked_msg));
        }

        // Step 2: Create checkpoint before destructive operations
        let should_checkpoint = self.should_checkpoint(tool_name);
        if should_checkpoint {
            match self
                .checkpoint_manager
                .create_checkpoint(&format!("Before {} execution", tool_name))
            {
                Ok(checkpoint) => {
                    info!(
                        "Created checkpoint {} before {} execution",
                        checkpoint.id, tool_name
                    );
                }
                Err(e) => {
                    warn!("Failed to create checkpoint: {}", e);
                    // Don't block execution if checkpoint fails, just warn
                }
            }
        }

        // Step 3: Run PreToolUse hooks, block if any hook blocks
        let hook_context = json!({
            "tool": tool_name,
            "args_summary": format!(
                "{}",
                args.to_string().chars().take(200).collect::<String>()
            ),
            "phase": plan_mode.current_phase().to_string(),
        });

        let pre_hooks = self
            .hook_manager
            .execute(HookTrigger::PreToolUse, hook_context.clone())
            .await
            .context("PreToolUse hook execution failed")?;

        if pre_hooks.should_block {
            let block_reason = pre_hooks
                .block_reason
                .unwrap_or_else(|| "Hook blocked execution".to_string());
            error!("PreToolUse hook blocked execution: {}", block_reason);
            return Err(anyhow!(
                "PreToolUse hook blocked execution: {}",
                block_reason
            ));
        }

        // Step 4: Execute the actual tool
        let result = self
            .execute_tool_impl(tool_name, args, plan_mode)
            .await
            .context("Tool execution failed")?;

        // Step 5: Run PostToolUse hooks (audit only, don't block)
        let post_hooks = self
            .hook_manager
            .execute(
                HookTrigger::PostToolUse,
                json!({
                    "tool": tool_name,
                    "success": true,
                    "output_summary": format!(
                        "{}",
                        result.text.chars().take(200).collect::<String>()
                    ),
                }),
            )
            .await;

        if let Err(e) = post_hooks {
            warn!("PostToolUse hook execution failed: {}", e);
            // Don't block execution on PostToolUse hook failures
        }

        // Step 6: Record cost if LLM was used
        let cost = self.extract_cost_from_output(&result);
        if let Some(cost_usd) = cost {
            info!("Tool '{}' execution cost: ${:.4}", tool_name, cost_usd);
            let api_call = ApiCall {
                model: "claude-3-opus".to_string(),
                input_tokens: 0,
                output_tokens: 0,
                cost_usd,
                timestamp: Utc::now(),
                tool_name: Some(tool_name.to_string()),
            };
            if let Err(e) = self.cost_tracker.record_call(api_call) {
                warn!("Failed to record cost: {}", e);
            }
        }

        // Step 7: Record interaction for rewind
        // Rewind recording integration point — caller can use checkpoint_manager
        info!("Tool '{}' executed successfully", tool_name);

        Ok(result)
    }

    /// Check if a tool should trigger auto-checkpoint before execution
    fn should_checkpoint(&self, tool_name: &str) -> bool {
        matches!(
            tool_name,
            "edit"
                | "edit_file"
                | "write"
                | "write_file"
                | "bash"
                | "multi_edit"
                | "search_replace"
                | "apply_patch"
                | "text_editor_20250728"
                | "text_editor_20250124"
        )
    }

    /// Execute the tool (dispatcher to actual tool implementations)
    ///
    /// For now, this returns placeholder results for testing.
    /// In production, this would dispatch to actual tool implementations
    /// via a ToolRegistry or similar mechanism.
    async fn execute_tool_impl(
        &self,
        tool_name: &str,
        _args: Value,
        plan_mode: &dyn PlanModeProvider,
    ) -> Result<ToolOutput> {
        // Placeholder implementation for testing
        // In production, dispatch to actual tools based on tool_name

        match tool_name {
            "read" => {
                // Read tool always succeeds with minimal output
                Ok(ToolOutput {
                    text: "[read tool not fully implemented]".to_string(),
                    structured: None,
                })
            }
            "edit" | "edit_file" => {
                if plan_mode.is_edit_dry_run() {
                    // In planning phase, return dry-run preview
                    Ok(ToolOutput {
                        text: "[edit tool dry-run preview - planning phase]".to_string(),
                        structured: None,
                    })
                } else {
                    // In implementation phase, would execute actual edit
                    Ok(ToolOutput {
                        text: "[edit tool not fully implemented]".to_string(),
                        structured: None,
                    })
                }
            }
            "write" => {
                // Write tool execution
                Ok(ToolOutput {
                    text: "[write tool not fully implemented]".to_string(),
                    structured: None,
                })
            }
            "bash" => {
                // Bash tool execution
                Ok(ToolOutput {
                    text: "[bash tool not fully implemented]".to_string(),
                    structured: None,
                })
            }
            _ => {
                // Unknown tool
                Err(anyhow!("Tool '{}' not implemented", tool_name))
            }
        }
    }

    /// Extract LLM cost from tool output metadata
    fn extract_cost_from_output(&self, output: &ToolOutput) -> Option<f64> {
        output
            .structured
            .as_ref()
            .and_then(|obj| obj.get(COST_METADATA_KEY))
            .and_then(|v| v.as_f64())
    }

    /// List available checkpoints
    pub async fn list_checkpoints(
        &self,
    ) -> Result<Vec<crate::workspace_checkpoint::WorkspaceCheckpoint>> {
        Ok(self.checkpoint_manager.list_checkpoints())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Mock PlanMode for testing
    struct MockPlanMode {
        phase: &'static str,
        allowed_tools: &'static [&'static str],
        edit_dry_run: bool,
    }

    impl MockPlanMode {
        fn planning() -> Self {
            Self {
                phase: "planning",
                allowed_tools: &[
                    "read",
                    "grep",
                    "glob",
                    "list_dir",
                    "lsp",
                    "web_search",
                    "web_fetch",
                    "edit_file",
                ],
                edit_dry_run: true,
            }
        }

        fn implementation() -> Self {
            Self {
                phase: "implementation",
                allowed_tools: &[
                    "read",
                    "edit_file",
                    "write",
                    "bash",
                    "grep",
                    "glob",
                    "list_dir",
                    "lsp",
                    "web_search",
                    "web_fetch",
                ],
                edit_dry_run: false,
            }
        }
    }

    impl PlanModeProvider for MockPlanMode {
        fn is_tool_allowed(&self, tool: &str) -> Result<(), String> {
            if self.allowed_tools.iter().any(|t| t == &tool) {
                Ok(())
            } else {
                Err(format!(
                    "Tool '{}' not allowed in {} phase",
                    tool, self.phase
                ))
            }
        }

        fn is_edit_dry_run(&self) -> bool {
            self.edit_dry_run
        }

        fn current_phase(&self) -> String {
            self.phase.to_string()
        }
    }

    /// Mock cost tracker for testing
    struct MockCostTracker;

    impl CostTrackerProvider for MockCostTracker {
        fn record_call(&self, _call: ApiCall) -> Result<()> {
            // No-op for testing
            Ok(())
        }
    }

    fn setup_executor(workspace_path: PathBuf, hooks_dir: &TempDir) -> Result<UnifiedToolExecutor> {
        let checkpoint_config = crate::workspace_checkpoint::CheckpointConfig::default();
        let checkpoint_mgr = CheckpointManager::new(workspace_path.clone(), checkpoint_config)?;

        let hook_mgr = HookManager::new(
            hooks_dir.path().to_path_buf(),
            crate::hooks::HookProfile::Standard,
            "test-session".to_string(),
        );

        let cost_tracker: Arc<dyn CostTrackerProvider> = Arc::new(MockCostTracker);

        Ok(UnifiedToolExecutor::new(
            Arc::new(checkpoint_mgr),
            Arc::new(hook_mgr),
            cost_tracker,
        ))
    }

    #[test]
    fn should_checkpoint_for_edit_tools() {
        let checkpoint_dir = TempDir::new().unwrap();
        let hooks_dir = TempDir::new().unwrap();
        let executor = setup_executor(checkpoint_dir.path().to_path_buf(), &hooks_dir).unwrap();

        assert!(executor.should_checkpoint("edit"));
        assert!(executor.should_checkpoint("write"));
        assert!(executor.should_checkpoint("write_file"));
        assert!(executor.should_checkpoint("bash"));
        assert!(executor.should_checkpoint("edit_file"));
        assert!(executor.should_checkpoint("multi_edit"));
        assert!(executor.should_checkpoint("search_replace"));
        assert!(executor.should_checkpoint("apply_patch"));
        assert!(executor.should_checkpoint("text_editor_20250728"));
        assert!(executor.should_checkpoint("text_editor_20250124"));
    }

    #[test]
    fn should_not_checkpoint_for_read_tools() {
        let checkpoint_dir = TempDir::new().unwrap();
        let hooks_dir = TempDir::new().unwrap();
        let executor = setup_executor(checkpoint_dir.path().to_path_buf(), &hooks_dir).unwrap();

        assert!(!executor.should_checkpoint("read"));
        assert!(!executor.should_checkpoint("grep"));
        assert!(!executor.should_checkpoint("lsp"));
    }

    #[test]
    fn extract_cost_from_output_missing() {
        let checkpoint_dir = TempDir::new().unwrap();
        let hooks_dir = TempDir::new().unwrap();
        let executor = setup_executor(checkpoint_dir.path().to_path_buf(), &hooks_dir).unwrap();

        let output = ToolOutput {
            text: "test".to_string(),
            structured: None,
        };

        assert_eq!(executor.extract_cost_from_output(&output), None);
    }

    #[test]
    fn extract_cost_from_output_present() {
        let checkpoint_dir = TempDir::new().unwrap();
        let hooks_dir = TempDir::new().unwrap();
        let executor = setup_executor(checkpoint_dir.path().to_path_buf(), &hooks_dir).unwrap();

        let output = ToolOutput {
            text: "test".to_string(),
            structured: Some(json!({ "llm_cost": 0.05 })),
        };

        assert_eq!(executor.extract_cost_from_output(&output), Some(0.05));
    }

    #[test]
    fn mock_plan_mode_planning_blocks_write() {
        let plan = MockPlanMode::planning();
        assert!(plan.is_tool_allowed("read").is_ok());
        assert!(plan.is_tool_allowed("write").is_err());
    }

    #[test]
    fn mock_plan_mode_implementation_allows_write() {
        let plan = MockPlanMode::implementation();
        assert!(plan.is_tool_allowed("read").is_ok());
        assert!(plan.is_tool_allowed("write").is_ok());
    }
}
