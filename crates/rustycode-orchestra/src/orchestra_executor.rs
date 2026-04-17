//! Canonical Autonomous Mode runtime executor - State-driven autonomous development
//!
//! Matches the reference implementation at /Users/nat/dev/orchestra-2
//!
//! Key principles:
//! - State lives on disk (.orchestra/STATE.md is the source of truth)
//! - Fresh session per unit (clean context window)
//! - Read STATE.md → Load plan → Execute → Verify → Write summary → Update state → Loop
//! - Crash recovery via lock files and activity logging
//! - Verification gates ensure units actually work
//! - Timeout supervision prevents hangs
//! - Budget tracking enforces spending limits

use crate::budget::{BudgetTracker, MetricsLedger};
use crate::crash_recovery::ActivityLog;
use crate::orchestra_config::OrchestraProjectConfig;
use crate::state_derivation::StateDeriver;
use crate::timeout::{TimeoutConfig, TimeoutSupervisor};
use rustycode_llm::{provider_helpers, LLMProvider, TaskType};
use rustycode_tools::{BashTool, ReadFileTool, ToolRegistry, WriteFileTool};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Autonomous Mode Style Executor
///
/// Reads .orchestra/STATE.md to determine current unit, loads the task plan,
/// creates a fresh LLM session, executes with tools, verifies results,
/// writes summary, updates STATE.md, and loops until no more work.
pub struct Orchestra2Executor {
    pub(crate) project_root: PathBuf,
    pub(crate) provider: Arc<dyn LLMProvider>,
    pub(crate) model: Mutex<String>,
    pub(crate) default_model: String,
    pub(crate) model_context_window: usize,
    pub(crate) activity_log: ActivityLog,
    pub(crate) timeout_supervisor: TimeoutSupervisor,
    pub(crate) budget_tracker: Mutex<BudgetTracker>,
    pub(crate) metrics_ledger: MetricsLedger,
    pub(crate) state_deriver: StateDeriver,
    pub(crate) tool_registry: ToolRegistry,
    pub(crate) task_model_overrides: Option<OrchestraProjectConfig>,
}

impl Orchestra2Executor {
    pub fn new(
        project_root: PathBuf,
        provider: Arc<dyn LLMProvider>,
        model: String,
        budget: f64,
    ) -> Self {
        // Default context window (orchestra-2 D002)
        let model_context_window = 200_000;

        let activity_log = ActivityLog::new(project_root.clone());
        let timeout_supervisor =
            TimeoutSupervisor::new(project_root.clone(), TimeoutConfig::default());
        let budget_tracker = Mutex::new(BudgetTracker::new(budget));
        let metrics_ledger = MetricsLedger::new(project_root.clone());
        let state_deriver = StateDeriver::new(project_root.clone());

        // Initialize tool registry with Autonomous Mode tools
        let mut tool_registry = ToolRegistry::new();
        tool_registry.register(ReadFileTool);
        tool_registry.register(WriteFileTool);
        tool_registry.register(BashTool);

        Self {
            project_root,
            provider,
            default_model: model.clone(),
            model: Mutex::new(model),
            model_context_window,
            activity_log,
            timeout_supervisor,
            budget_tracker,
            metrics_ledger,
            state_deriver,
            tool_registry,
            task_model_overrides: None,
        }
    }

    /// Get the current model
    pub fn get_model(&self) -> String {
        self.model.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Set the current model temporarily
    pub fn set_model(&self, model: String) {
        *self.model.lock().unwrap_or_else(|e| e.into_inner()) = model;
    }

    /// Set task-specific model overrides from a config file
    pub fn with_task_model_overrides(mut self, cfg: OrchestraProjectConfig) -> Self {
        self.task_model_overrides = Some(cfg);
        self
    }

    /// Select the appropriate model for a task type
    /// Falls back to default model if provider registry is unavailable
    pub fn select_model_for_task(&self, task_type: TaskType) -> String {
        // Check task model overrides first
        if let Some(ref overrides) = self.task_model_overrides {
            if let Some(m) = overrides.model_for_task(task_type) {
                return m.to_string();
            }
        }

        let registry = provider_helpers::get_registry();

        // Fall back to tier-based model selection
        match task_type {
            TaskType::CodeGeneration => {
                // Prefer balanced models for code generation
                registry
                    .get_models_by_tier(rustycode_llm::ModelTier::Balanced)
                    .first()
                    .map(|m| m.id.clone())
                    .unwrap_or_else(|| self.default_model.clone())
            }
            TaskType::Testing => {
                // Use cheaper models for testing
                registry
                    .get_models_by_tier(rustycode_llm::ModelTier::Budget)
                    .first()
                    .map(|m| m.id.clone())
                    .unwrap_or_else(|| self.default_model.clone())
            }
            TaskType::Planning | TaskType::Research => {
                // Use premium models for planning and research
                registry
                    .get_models_by_tier(rustycode_llm::ModelTier::Premium)
                    .first()
                    .map(|m| m.id.clone())
                    .unwrap_or_else(|| self.default_model.clone())
            }
            _ => self.default_model.clone(),
        }
    }

    /// Restore the default model
    pub fn restore_default_model(&self) {
        *self.model.lock().unwrap_or_else(|e| e.into_inner()) = self.default_model.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_llm::MockProvider;

    #[test]
    fn test_select_model_for_testing() {
        let executor = Orchestra2Executor::new(
            PathBuf::from("/tmp/test"),
            Arc::new(MockProvider::from_text("test")),
            "claude-3-5-sonnet".to_string(),
            100.0,
        );

        let testing_model = executor.select_model_for_task(TaskType::Testing);
        // Should select a budget-tier model (cheaper for testing)
        assert!(!testing_model.is_empty());
        // Budget models are usually haiku, gpt-4o-mini, llama2
        let is_budget = testing_model.contains("haiku")
            || testing_model.contains("mini")
            || testing_model.contains("llama");
        assert!(
            is_budget,
            "Testing should use budget model, got: {}",
            testing_model
        );
    }

    #[test]
    fn test_select_model_for_planning() {
        let executor = Orchestra2Executor::new(
            PathBuf::from("/tmp/test"),
            Arc::new(MockProvider::from_text("test")),
            "claude-3-5-sonnet".to_string(),
            100.0,
        );

        let planning_model = executor.select_model_for_task(TaskType::Planning);
        // Should select a premium-tier model (best reasoning)
        assert!(!planning_model.is_empty());
        // Premium models are usually opus, gpt-4, etc.
        let is_premium = planning_model.contains("opus")
            || planning_model.contains("gpt-4")
            || planning_model.contains("pro");
        assert!(
            is_premium,
            "Planning should use premium model, got: {}",
            planning_model
        );
    }

    #[test]
    fn test_model_switching() {
        let executor = Orchestra2Executor::new(
            PathBuf::from("/tmp/test"),
            Arc::new(MockProvider::from_text("test")),
            "default-model".to_string(),
            100.0,
        );

        assert_eq!(executor.get_model(), "default-model");

        executor.set_model("test-model".to_string());
        assert_eq!(executor.get_model(), "test-model");

        executor.restore_default_model();
        assert_eq!(executor.get_model(), "default-model");
    }

    #[test]
    fn test_model_restore_after_task() {
        let executor = Orchestra2Executor::new(
            PathBuf::from("/tmp/test"),
            Arc::new(MockProvider::from_text("test")),
            "claude-3-5-sonnet".to_string(),
            100.0,
        );

        let original = executor.get_model();
        let testing_model = executor.select_model_for_task(TaskType::Testing);
        executor.set_model(testing_model);

        assert_ne!(executor.get_model(), original);

        executor.restore_default_model();
        assert_eq!(executor.get_model(), original);
    }
}
