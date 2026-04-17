// rustycode-orchestra/src/auto_mode.rs
//! Autonomous Mode style auto-mode state machine
//!
//! State machine driven by .orchestra/ files on disk. Each "unit" of work
//! gets a fresh session with clean context window.

use crate::{
    complexity::{ComplexityClassifier, Unit, UnitType},
    error::{OrchestraV2Error, Result},
    llm::{LlmClient, LlmConfig, ModelProfile},
    model_router::{
        BudgetStatus, BudgetTracker, ModelRouter, ModelSelection, OutcomeType, RoutingOutcome,
    },
    state::{OrchestraState, StateManager},
    tools::ToolExecutor,
};
use chrono::{DateTime, Utc};
use rustycode_llm::ChatMessage;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Auto-mode state machine
pub struct AutoMode {
    /// Project root
    project_root: PathBuf,
    /// State manager
    state_manager: StateManager,
    /// Model router
    model_router: Arc<Mutex<ModelRouter>>,
    /// Budget tracker
    budget_tracker: Arc<Mutex<BudgetTracker>>,
    /// Current session
    session: Arc<Mutex<Option<AutoSession>>>,
}

impl AutoMode {
    /// Create a new auto-mode instance
    pub fn new(
        project_root: PathBuf,
        model_router: ModelRouter,
        budget_tracker: BudgetTracker,
    ) -> Result<Self> {
        let state_manager = StateManager::new(&project_root)?;

        Ok(Self {
            project_root,
            state_manager,
            model_router: Arc::new(Mutex::new(model_router)),
            budget_tracker: Arc::new(Mutex::new(budget_tracker)),
            session: Arc::new(Mutex::new(None)),
        })
    }

    /// Ensure work exists - create default milestone/slice if needed
    fn ensure_work_exists(&self) -> Result<()> {
        use std::fs;

        let orchestra_dir = self.project_root.join(".orchestra");
        let state_path = orchestra_dir.join("STATE.md");

        // If STATE.md exists with valid frontmatter, we're good
        if state_path.exists() {
            let content = fs::read_to_string(&state_path)?;
            if content.starts_with("---") {
                return Ok(());
            }
        }

        // Create default Orchestra structure
        if !orchestra_dir.exists() {
            fs::create_dir_all(&orchestra_dir)?;
        }

        // Create STATE.md with proper frontmatter format
        let now = chrono::Utc::now().to_rfc3339();
        let default_state = format!(
            r#"---
updated_at: {}
milestone: M01
version: 2.0
---

# Orchestra State

## Project
* **Name:** RustyCode Project
* **Status:** In Progress

## Execution
* **Active Task:** None
* **Active Phase:** None
* **Active Wave:** None

## Progress
* **Tasks Completed:** 0
* **Total Tasks:** 1

"#,
            now
        );
        fs::write(&state_path, default_state)?;

        // Create default milestone directory
        let milestone_dir = orchestra_dir.join("milestones").join("M01");
        fs::create_dir_all(milestone_dir.join("slices").join("S01").join("tasks"))?;

        // Create ROADMAP.md
        let roadmap = r#"# Roadmap

## Milestone M01: Initial Development
- **Status:** In Progress
- **Priority:** High

### Slice S01: Core Implementation
- **Status:** In Progress
- **Tasks:**
  - [ ] T01: Define and implement initial feature

"#;
        fs::write(orchestra_dir.join("ROADMAP.md"), roadmap)?;

        // Create slice plan
        let slice_plan = r#"# Slice S01: Core Implementation

## Overview
Initial slice for getting started with development.

## Tasks

### T01: Define and implement initial feature

**Description:**
Start by understanding the project structure and defining what needs to be built.

**Steps:**
1. Explore the project to understand its structure
2. Identify the main components and their relationships
3. Define a simple feature to implement
4. Implement the feature
5. Verify the implementation works

**Verification:**
Run tests or verify the feature works as expected.

"#;
        fs::write(
            milestone_dir.join("slices").join("S01").join("S01-PLAN.md"),
            slice_plan,
        )?;

        // Create task file
        let task = r#"# Task T01: Define and implement initial feature

## Description
Start by understanding the project structure and defining what needs to be built.

## Steps
1. Explore the project to understand its structure
2. Identify the main components and their relationships
3. Define a simple feature to implement
4. Implement the feature
5. Verify the implementation works

## Verification
Run tests or verify the feature works as expected.

"#;
        fs::write(
            milestone_dir
                .join("slices")
                .join("S01")
                .join("tasks")
                .join("T01-TASK.md"),
            task,
        )?;

        tracing::info!("Created default Orchestra structure at {:?}", orchestra_dir);
        Ok(())
    }

    /// Start auto-mode execution
    pub async fn start(&self) -> Result<AutoModeResult> {
        // Auto-initialize if no work exists
        self.ensure_work_exists()?;

        // Load current state
        let state = self.state_manager.read_state()?;

        // Determine next unit
        let next_unit = self.determine_next_unit(&state).await?;

        // Classify complexity
        let complexity = ComplexityClassifier::classify(&next_unit);

        // Select model
        let mut router = self.model_router.lock().await;
        let model_selection = router.select_model(&next_unit, complexity).await?;

        // Create session
        let session = AutoSession {
            unit: next_unit.clone(),
            model: model_selection.clone(),
            started_at: Utc::now(),
            status: SessionStatus::Running,
        };

        // Save session
        *self.session.lock().await = Some(session.clone());

        // Execute unit
        let result = self.execute_unit(&session).await?;

        // Record outcome
        let outcome = if result.success {
            RoutingOutcome {
                outcome: OutcomeType::Success,
                attempts: 1,
                error: None,
            }
        } else {
            RoutingOutcome {
                outcome: OutcomeType::Failure,
                attempts: result.retries + 1,
                error: result.error.clone(),
            }
        };

        router.record_outcome(&model_selection, &outcome);

        // Update state
        self.update_state(&next_unit, &result).await?;

        Ok(AutoModeResult {
            session,
            result,
            model_selection,
        })
    }

    /// Determine the next unit to execute
    async fn determine_next_unit(&self, state: &OrchestraState) -> Result<Unit> {
        // Check if there's an active task
        if let Some(task_id) = &state.execution.active_task {
            return Ok(Unit::new(
                task_id.clone(),
                UnitType::Task,
                format!("Execute task {}", task_id),
            ));
        }

        // Check if there's an active wave/phase
        if let Some(phase_id) = &state.execution.active_phase {
            return Ok(Unit::new(
                phase_id.clone(),
                UnitType::Slice,
                format!("Execute phase {}", phase_id),
            ));
        }

        // No active unit - try to find next pending task automatically
        if let Ok(next_task) = self.find_next_pending_task(state).await {
            return Ok(next_task);
        }

        // No active unit and no pending tasks
        Err(OrchestraV2Error::InvalidState(
            "No active unit found and no pending tasks".to_string(),
        ))
    }

    /// Find the next pending task from the roadmap
    async fn find_next_pending_task(&self, state: &OrchestraState) -> Result<Unit> {
        use std::fs;

        // Get active milestone and slice from state
        let milestone_id = state.milestone.as_str();
        if milestone_id.is_empty() {
            return Err(OrchestraV2Error::InvalidState(
                "No active milestone".to_string(),
            ));
        }

        // Try to get active slice from debug state (stored in STATE.md body)
        // For now, we'll scan for the latest slice
        let milestones_dir = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id);
        let slices_dir = milestones_dir.join("slices");

        if !slices_dir.exists() {
            return Err(OrchestraV2Error::InvalidState(
                "No slices directory found".to_string(),
            ));
        }

        // Find all slices
        let mut slices: Vec<_> = fs::read_dir(&slices_dir)
            .map_err(OrchestraV2Error::Io)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        slices.sort_by_key(|a| a.path());

        // For each slice, check for pending tasks
        for slice_entry in slices {
            let slice_path = slice_entry.path();
            let slice_name = slice_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Read the slice plan
            let plan_path = slice_path.join(format!("{}-PLAN.md", slice_name));
            if !plan_path.exists() {
                continue;
            }

            let plan_content = fs::read_to_string(&plan_path).map_err(OrchestraV2Error::Io)?;

            // Extract tasks from plan
            if let Some(task_id) = self.extract_first_pending_task(&plan_content) {
                // Read the task file for description
                let task_path = slice_path
                    .join("tasks")
                    .join(format!("{}-TASK.md", task_id));
                if task_path.exists() {
                    let task_content =
                        fs::read_to_string(&task_path).map_err(OrchestraV2Error::Io)?;

                    // Extract title from task
                    let title = self
                        .extract_task_title(&task_content)
                        .unwrap_or_else(|| format!("Task {}", task_id));

                    return Ok(Unit::new(task_id, UnitType::Task, title));
                }
            }
        }

        Err(OrchestraV2Error::InvalidState(
            "No pending tasks found".to_string(),
        ))
    }

    /// Extract the first pending task ID from a plan
    fn extract_first_pending_task(&self, plan_content: &str) -> Option<String> {
        // Find the Tasks section
        let tasks_start = plan_content.find("## Tasks")?;
        let tasks_section = &plan_content[tasks_start..];

        // Extract task links like "- **[T01](path)** - Description"
        let lines = tasks_section.lines();
        for line in lines {
            if line.contains("**[T") && line.contains("](.orchestra/") {
                // Extract task ID
                if let Some(start) = line.find("**[T") {
                    let after_start = &line[start + 3..];
                    if let Some(end) = after_start.find(']') {
                        let task_id = &after_start[..end];
                        // Simple check: if it doesn't have ~~strikethrough~~, it's pending
                        if !line.contains("~~") {
                            return Some(task_id.to_string());
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract task title from task file
    fn extract_task_title(&self, task_content: &str) -> Option<String> {
        // Find the first line starting with "# "
        task_content
            .lines()
            .find(|line| line.starts_with("# "))
            .map(|title| title.trim_start_matches("# ").trim().to_string())
    }

    /// Execute a unit
    async fn execute_unit(&self, session: &AutoSession) -> Result<UnitResult> {
        // Create LLM client
        let llm_config = LlmConfig {
            model_profile: ModelProfile::Balanced,
            planning_temperature: 0.1,
            execution_temperature: 0.7,
            verification_temperature: 0.3,
            research_temperature: 0.5,
            max_tokens: 8192,
            streaming: false,
        };
        let llm_client = LlmClient::new(llm_config);

        // Create tool executor
        let tool_executor = ToolExecutor::new(self.project_root.clone());

        // Build initial messages
        let mut messages = vec![
            ChatMessage::system(
                "You are an expert software developer. Execute the task described by the user. \
                Use tools when necessary to read files, run commands, or make changes."
                    .to_string(),
            ),
            ChatMessage::user(session.unit.description.clone()),
        ];

        let mut total_tokens = 0u32;
        let mut total_duration = 0u64;
        let max_turns = 10; // Prevent infinite loops
        let mut current_turn = 0;

        // Multi-turn conversation loop
        loop {
            current_turn += 1;
            if current_turn > max_turns {
                return Err(OrchestraV2Error::AutoMode(format!(
                    "Exceeded maximum conversation turns ({max_turns})"
                )));
            }

            // Execute the task
            let result = llm_client
                .execute_task(&session.model, messages.clone(), None)
                .await
                .map_err(|e| OrchestraV2Error::AutoMode(format!("LLM execution failed: {}", e)))?;

            total_tokens += result.tokens_used;
            total_duration += result.duration_ms;

            // If no tool calls, we're done
            if result.tool_calls.is_empty() {
                // Calculate cost
                let cost_per_1k_input = 0.003;
                let cost_per_1k_output = 0.015;
                let estimated_input = total_tokens / 2;
                let estimated_output = total_tokens / 2;
                let total_cost = (estimated_input as f64 * cost_per_1k_input / 1000.0)
                    + (estimated_output as f64 * cost_per_1k_output / 1000.0);

                return Ok(UnitResult {
                    success: true,
                    retries: 0,
                    error: None,
                    tokens_used: total_tokens,
                    cost: total_cost,
                    duration_ms: total_duration,
                });
            }

            // Execute tool calls
            for tool_call in &result.tool_calls {
                let execution_result = tool_executor.execute_tool(tool_call).map_err(|e| {
                    OrchestraV2Error::AutoMode(format!("Tool execution failed: {}", e))
                })?;

                // Add tool result to conversation
                messages.push(ChatMessage::assistant(format!(
                    "<tool_use>{}</tool_use>",
                    tool_call.name
                )));
                messages.push(ChatMessage::user(if execution_result.success {
                    format!("<tool_result>{}</tool_result>", execution_result.output)
                } else {
                    format!(
                        "<tool_error>{}</tool_error>",
                        execution_result
                            .error
                            .unwrap_or_else(|| "Unknown error".to_string())
                    )
                }));
            }
        }
    }

    /// Update state after unit execution
    async fn update_state(&self, unit: &Unit, result: &UnitResult) -> Result<()> {
        // Update budget
        let mut budget = self.budget_tracker.lock().await;
        budget.record_spend(result.cost);

        // Mark task as complete in ROADMAP.md
        if result.success {
            self.mark_task_complete(&unit.id)?;
        }

        // Update state manager
        let mut state = self.state_manager.read_state()?;

        // Clear active unit
        state.execution.active_task = None;
        state.execution.active_phase = None;
        state.execution.active_wave = None;

        self.state_manager.write_state(&state)?;

        Ok(())
    }

    /// Mark a task as complete in ROADMAP.md
    fn mark_task_complete(&self, task_id: &str) -> Result<()> {
        use std::fs;

        let roadmap_path = self.project_root.join(".orchestra").join("ROADMAP.md");
        if !roadmap_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&roadmap_path)?;
        // Replace "- [ ] T01" with "- [x] T01"
        let updated = content.replace(&format!("- [ ] {}", task_id), &format!("- [x] {}", task_id));

        if updated != content {
            fs::write(&roadmap_path, updated)?;
            tracing::info!("Marked task {} as complete", task_id);
        }

        Ok(())
    }

    /// Check if auto-mode should continue
    pub async fn should_continue(&self) -> Result<bool> {
        let state = self.state_manager.read_state()?;
        let budget = self.budget_tracker.lock().await;
        let budget_status = budget.status().await;

        // Stop if over budget
        if budget_status == BudgetStatus::OverBudget {
            return Ok(false);
        }

        // Stop if no more work
        if state.execution.active_task.is_none() && state.execution.active_phase.is_none() {
            return Ok(false);
        }

        Ok(true)
    }

    /// Get current session
    pub async fn current_session(&self) -> Option<AutoSession> {
        self.session.lock().await.clone()
    }

    /// Get budget status
    pub async fn budget_status(&self) -> BudgetStatus {
        let budget = self.budget_tracker.lock().await;
        budget.status().await
    }

    /// Get remaining budget
    pub async fn remaining_budget(&self) -> f64 {
        let budget = self.budget_tracker.lock().await;
        budget.remaining()
    }
}

/// Auto-mode session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoSession {
    /// Unit being executed
    pub unit: Unit,
    /// Model selection for this session
    pub model: ModelSelection,
    /// When session started
    pub started_at: DateTime<Utc>,
    /// Session status
    pub status: SessionStatus,
}

/// Session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionStatus {
    /// Session is running
    Running,
    /// Session completed successfully
    Completed,
    /// Session failed
    Failed,
    /// Session timed out
    Timeout,
}

/// Auto-mode execution result
#[derive(Debug, Clone)]
pub struct AutoModeResult {
    /// Session that was executed
    pub session: AutoSession,
    /// Execution result
    pub result: UnitResult,
    /// Model that was used
    pub model_selection: ModelSelection,
}

/// Unit execution result
#[derive(Debug, Clone)]
pub struct UnitResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Number of retries
    pub retries: u32,
    /// Error message if failed
    pub error: Option<String>,
    /// Tokens used
    pub tokens_used: u32,
    /// Cost in USD
    pub cost: f64,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Auto-mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoModeConfig {
    /// Maximum unit dispatches before forced pause
    pub max_unit_dispatches: u32,
    /// Maximum consecutive skips before stopping
    pub max_consecutive_skips: u32,
    /// Dispatch timeout in milliseconds
    pub dispatch_timeout_ms: u64,
    /// New session timeout in milliseconds
    pub new_session_timeout_ms: u64,
    /// Whether to enable budget enforcement
    pub budget_enforcement: bool,
    /// Whether to enable stuck detection
    pub stuck_detection: bool,
    /// Whether to enable auto-verification
    pub auto_verification: bool,
}

impl Default for AutoModeConfig {
    fn default() -> Self {
        Self {
            max_unit_dispatches: 100,
            max_consecutive_skips: 5,
            dispatch_timeout_ms: 300000,   // 5 minutes
            new_session_timeout_ms: 60000, // 1 minute
            budget_enforcement: true,
            stuck_detection: true,
            auto_verification: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complexity::{RiskLevel, UnitType};

    fn test_unit(id: &str) -> Unit {
        Unit {
            id: id.into(),
            unit_type: UnitType::Task,
            file_count: 1,
            lines_changed: 10,
            dependencies: vec![],
            test_requirements: vec![],
            integration_points: vec![],
            risk_level: RiskLevel::Low,
            description: "test".into(),
        }
    }

    fn test_model_selection() -> ModelSelection {
        ModelSelection {
            model: "claude-3-5-sonnet-20241022".into(),
            tier: crate::complexity::ModelTier::Balanced,
            provider: "anthropic".into(),
            reasoning: "standard complexity".into(),
            selected_at: chrono::Utc::now(),
        }
    }

    // --- SessionStatus serde ---

    #[test]
    fn session_status_serde() {
        for s in &[
            SessionStatus::Running,
            SessionStatus::Completed,
            SessionStatus::Failed,
            SessionStatus::Timeout,
        ] {
            let json = serde_json::to_string(s).unwrap();
            let back: SessionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, &back);
        }
    }

    // --- AutoModeConfig ---

    #[test]
    fn auto_mode_config_default() {
        let config = AutoModeConfig::default();
        assert_eq!(config.max_unit_dispatches, 100);
        assert_eq!(config.max_consecutive_skips, 5);
        assert_eq!(config.dispatch_timeout_ms, 300000);
        assert_eq!(config.new_session_timeout_ms, 60000);
        assert!(config.budget_enforcement);
        assert!(config.stuck_detection);
        assert!(config.auto_verification);
    }

    #[test]
    fn auto_mode_config_serde_roundtrip() {
        let config = AutoModeConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: AutoModeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_unit_dispatches, 100);
        assert!(decoded.budget_enforcement);
    }

    // --- AutoSession ---

    #[test]
    fn auto_session_serde_roundtrip() {
        let session = AutoSession {
            unit: test_unit("u1"),
            model: test_model_selection(),
            started_at: chrono::Utc::now(),
            status: SessionStatus::Running,
        };
        let json = serde_json::to_string(&session).unwrap();
        let decoded: AutoSession = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.unit.id, "u1");
        assert_eq!(decoded.status, SessionStatus::Running);
    }

    // --- UnitResult ---

    #[test]
    fn unit_result_construction() {
        let result = UnitResult {
            success: true,
            retries: 0,
            error: None,
            tokens_used: 500,
            cost: 0.01,
            duration_ms: 1200,
        };
        assert!(result.success);
        assert_eq!(result.tokens_used, 500);
    }

    #[test]
    fn unit_result_with_error() {
        let result = UnitResult {
            success: false,
            retries: 3,
            error: Some("timeout".into()),
            tokens_used: 1500,
            cost: 0.05,
            duration_ms: 30000,
        };
        assert!(!result.success);
        assert_eq!(result.retries, 3);
        assert_eq!(result.error, Some("timeout".into()));
    }
}
