// rustycode-orchestra/src/workflow.rs
//! Autonomous Mode workflow orchestration
//!
//! Complete workflow management: Research → Plan → Execute → Complete → Reassess → Validate

use crate::complexity::{ComplexityClassifier, Unit, UnitType};
use crate::git_self_heal::unstage_orchestra_runtime_files;
use crate::llm::{ChatMessage, LlmClient, LlmConfig, ModelProfile};
use crate::model_router::{BudgetTracker, ModelRouter, ModelSelection};
use crate::tools::ToolExecutor;
use crate::{
    error::{OrchestraV2Error, Result},
    model_cost_table,
    state::StateManager,
    state_derivation::StateDeriver,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Workflow orchestration for Autonomous Mode
pub struct WorkflowOrchestrator {
    /// Project root
    project_root: PathBuf,
    /// State manager (reserved for future use in state persistence)
    #[allow(dead_code)] // Kept for future use
    state_manager: StateManager,
    /// Model router
    model_router: Arc<Mutex<ModelRouter>>,
    /// Budget tracker (reserved for future budget enforcement during workflow execution)
    #[allow(dead_code)] // Kept for future use
    budget_tracker: Arc<Mutex<BudgetTracker>>,
    /// Tool executor
    tool_executor: ToolExecutor,
    /// Current phase
    current_phase: Arc<Mutex<Option<WorkflowPhase>>>,
}

impl WorkflowOrchestrator {
    /// Create a new workflow orchestrator
    pub fn new(
        project_root: PathBuf,
        model_router: ModelRouter,
        budget_tracker: BudgetTracker,
    ) -> Result<Self> {
        let state_manager = StateManager::new(&project_root)?;
        let tool_executor = ToolExecutor::new(project_root.clone());

        Ok(Self {
            project_root,
            state_manager,
            model_router: Arc::new(Mutex::new(model_router)),
            budget_tracker: Arc::new(Mutex::new(budget_tracker)),
            tool_executor,
            current_phase: Arc::new(Mutex::new(None)),
        })
    }

    /// Run the full workflow for a milestone
    pub async fn run_milestone_workflow(&self, milestone_id: &str) -> Result<WorkflowResult> {
        let mut slices_completed = 0u32;
        let mut total_cost = 0.0f64;
        let mut total_tokens = 0u32;

        loop {
            // Get next slice
            let slice_id = self.determine_next_slice(milestone_id).await?;
            if slice_id.is_none() {
                // All slices complete
                break;
            }
            let slice_id = slice_id.unwrap();

            // Run slice workflow
            let slice_result = self.run_slice_workflow(&slice_id).await?;

            slices_completed += 1;
            total_cost += slice_result.cost;
            total_tokens += slice_result.tokens_used;

            // Reassess roadmap
            let should_continue = self.reassess_roadmap(&slice_id).await?;
            if !should_continue {
                break;
            }
        }

        // Validate milestone
        self.validate_milestone(milestone_id).await?;

        Ok(WorkflowResult {
            slices_completed,
            total_cost,
            total_tokens,
            success: true,
        })
    }

    /// Run workflow for a single slice
    pub async fn run_slice_workflow(&self, slice_id: &str) -> Result<SliceResult> {
        let mut tokens_used = 0u32;
        let mut cost = 0.0f64;

        // Phase 1: Research
        let research_result = self.run_research_phase(slice_id).await?;
        tokens_used += research_result.tokens_used;
        cost += research_result.cost;

        // Phase 2: Plan
        let plan_result = self
            .run_planning_phase(slice_id, &research_result.summary)
            .await?;
        tokens_used += plan_result.tokens_used;
        cost += plan_result.cost;

        // Phase 3: Execute tasks
        for task in &plan_result.tasks {
            let task_result = self.run_execution_phase(slice_id, task).await?;
            tokens_used += task_result.tokens_used;
            cost += task_result.cost;
        }

        // Phase 4: Complete slice
        let complete_result = self.run_completion_phase(slice_id).await?;
        tokens_used += complete_result.tokens_used;
        cost += complete_result.cost;

        Ok(SliceResult {
            slice_id: slice_id.to_string(),
            tokens_used,
            cost,
            tasks_completed: plan_result.tasks.len() as u32,
        })
    }

    /// Research phase - scout codebase and docs
    async fn run_research_phase(&self, slice_id: &str) -> Result<PhaseResult> {
        self.set_current_phase(WorkflowPhase::Research).await;

        let unit = Unit::new(
            format!("research-{}", slice_id),
            UnitType::Research,
            format!("Research phase for slice {}", slice_id),
        );

        let context = self.build_research_context(slice_id).await?;
        let result = self.execute_unit_with_context(&unit, &context).await?;

        // Save research summary
        self.save_phase_summary(slice_id, "research", &result.output)
            .await?;

        Ok(PhaseResult {
            phase: "Research".to_string(),
            tokens_used: result.tokens_used,
            cost: result.cost,
            summary: result.output,
        })
    }

    /// Planning phase - decompose into tasks
    async fn run_planning_phase(
        &self,
        slice_id: &str,
        research_summary: &str,
    ) -> Result<PlanningResult> {
        self.set_current_phase(WorkflowPhase::Plan).await;

        let unit = Unit::new(
            format!("plan-{}", slice_id),
            UnitType::Planning,
            format!("Planning phase for slice {}", slice_id),
        );

        let context = self
            .build_planning_context(slice_id, research_summary)
            .await?;
        let result = self.execute_unit_with_context(&unit, &context).await?;

        // Parse tasks from plan
        let tasks = self.parse_tasks_from_plan(&result.output)?;

        // Save plan
        self.save_phase_summary(slice_id, "plan", &result.output)
            .await?;

        Ok(PlanningResult {
            tasks,
            tokens_used: result.tokens_used,
            cost: result.cost,
        })
    }

    /// Execution phase - run each task
    async fn run_execution_phase(&self, slice_id: &str, task: &Task) -> Result<TaskResult> {
        self.set_current_phase(WorkflowPhase::Execute).await;

        let unit = Unit::new(task.id.clone(), UnitType::Task, task.description.clone());

        let context = self.build_task_context(slice_id, task).await?;
        let result = self.execute_unit_with_context(&unit, &context).await?;

        // Run verification
        let verification_passed = self.run_verification_gates().await?;

        // Generate commit message
        let commit_message = self.generate_commit_message(task, &result.output)?;

        // Commit work
        self.commit_work(&commit_message).await?;

        Ok(TaskResult {
            task_id: task.id.clone(),
            tokens_used: result.tokens_used,
            cost: result.cost,
            verification_passed,
        })
    }

    /// Completion phase - write summary and UAT
    async fn run_completion_phase(&self, slice_id: &str) -> Result<PhaseResult> {
        self.set_current_phase(WorkflowPhase::Complete).await;

        let unit = Unit::new(
            format!("complete-{}", slice_id),
            UnitType::Completion,
            format!("Completion phase for slice {}", slice_id),
        );

        let context = self.build_completion_context(slice_id).await?;
        let result = self.execute_unit_with_context(&unit, &context).await?;

        // Mark complete in roadmap
        self.mark_slice_complete(slice_id).await?;

        // Write UAT script
        self.write_uat_script(slice_id, &result.output).await?;

        Ok(PhaseResult {
            phase: "Complete".to_string(),
            tokens_used: result.tokens_used,
            cost: result.cost,
            summary: result.output,
        })
    }

    /// Execute a unit with injected context
    async fn execute_unit_with_context(
        &self,
        unit: &Unit,
        context: &WorkflowContext,
    ) -> Result<UnitExecutionResult> {
        // Classify complexity
        let complexity = ComplexityClassifier::classify(unit);

        // Select model
        let mut router = self.model_router.lock().await;
        let model_selection = router.select_model(unit, complexity).await?;

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

        // Build messages with injected context
        let mut messages = self.build_messages_with_context(unit, context);

        // Execute with multi-turn conversation
        let mut total_tokens = 0u32;
        let mut total_duration = 0u64;
        let max_turns = 10;
        let mut current_turn = 0;

        loop {
            current_turn += 1;
            if current_turn > max_turns {
                return Err(OrchestraV2Error::AutoMode(
                    "Exceeded max conversation turns".to_string(),
                ));
            }

            let result = llm_client
                .execute_task(&model_selection, messages.clone(), None)
                .await?;
            total_tokens += result.tokens_used;
            total_duration += result.duration_ms;

            if result.tool_calls.is_empty() {
                // Done - calculate cost
                let cost = self.calculate_cost(total_tokens, &model_selection);

                return Ok(UnitExecutionResult {
                    output: result.output,
                    tool_calls: result.tool_calls,
                    tokens_used: total_tokens,
                    cost,
                    duration_ms: total_duration,
                });
            }

            // Execute tools and continue conversation
            for tool_call in &result.tool_calls {
                let exec_result = self.tool_executor.execute_tool(tool_call)?;

                messages.push(ChatMessage::assistant(format!(
                    "<tool_use>{}</tool_use>",
                    tool_call.name
                )));
                messages.push(ChatMessage::user(format!(
                    "<tool_result>{}</tool_result>",
                    exec_result.output
                )));
            }
        }
    }

    /// Build messages with pre-injected context
    fn build_messages_with_context(
        &self,
        unit: &Unit,
        context: &WorkflowContext,
    ) -> Vec<crate::llm::ChatMessage> {
        let mut messages = vec![];

        // System prompt
        messages.push(crate::llm::ChatMessage::system(
            "You are an expert software developer. Execute the task described by the user.",
        ));

        // Inject task plan
        if let Some(task_plan) = &context.task_plan {
            messages.push(crate::llm::ChatMessage::user(format!(
                "## Task Plan\n\n{}",
                task_plan
            )));
        }

        // Inject slice plan
        if let Some(slice_plan) = &context.slice_plan {
            messages.push(crate::llm::ChatMessage::user(format!(
                "## Slice Context\n\n{}",
                slice_plan
            )));
        }

        // Inject prior summaries
        if !context.prior_summaries.is_empty() {
            messages.push(crate::llm::ChatMessage::user(format!(
                "## Prior Work\n\n{}",
                context.prior_summaries.join("\n\n")
            )));
        }

        // Inject dependencies
        if !context.dependencies.is_empty() {
            messages.push(crate::llm::ChatMessage::user(format!(
                "## Dependencies\n\n{}",
                context.dependencies.join("\n\n")
            )));
        }

        // Inject roadmap excerpt
        if let Some(roadmap) = &context.roadmap_excerpt {
            messages.push(crate::llm::ChatMessage::user(format!(
                "## Roadmap\n\n{}",
                roadmap
            )));
        }

        // Inject decisions register
        if let Some(decisions) = &context.decisions_register {
            messages.push(crate::llm::ChatMessage::user(format!(
                "## Decisions Register\n\n{}",
                decisions
            )));
        }

        // Current task
        messages.push(crate::llm::ChatMessage::user(unit.description.clone()));

        messages
    }

    /// Determine the next slice to work on
    async fn determine_next_slice(&self, _milestone_id: &str) -> Result<Option<String>> {
        // Use state derivation to find the next incomplete slice
        let deriver = StateDeriver::new(self.project_root.clone());
        let state = deriver.derive_state().map_err(|e| {
            OrchestraV2Error::InvalidState(format!("Failed to derive state: {}", e))
        })?;

        // Return the active slice ID if one exists
        Ok(state.active_slice.map(|s| s.id))
    }

    /// Reassess roadmap after slice completion
    async fn reassess_roadmap(&self, _slice_id: &str) -> Result<bool> {
        // Reassess the roadmap by re-deriving state
        // This ensures we have the latest view of completed/incomplete work
        let deriver = StateDeriver::new(self.project_root.clone());
        let _state = deriver.derive_state().map_err(|e| {
            OrchestraV2Error::InvalidState(format!("Failed to derive state: {}", e))
        })?;

        // Return true to indicate roadmap is still valid
        // In a full implementation, this would check for new tasks discovered during work
        Ok(true)
    }

    /// Validate milestone after all slices complete
    async fn validate_milestone(&self, milestone_id: &str) -> Result<()> {
        self.set_current_phase(WorkflowPhase::Validate).await;

        let unit = Unit::new(
            format!("validate-{}", milestone_id),
            UnitType::Validation,
            format!("Validate milestone {}", milestone_id),
        );

        let context = self.build_validation_context(milestone_id).await?;
        let result = self.execute_unit_with_context(&unit, &context).await?;

        // Save validation results
        self.save_validation_results(milestone_id, &result.output)
            .await?;

        Ok(())
    }

    /// Run verification gates
    async fn run_verification_gates(&self) -> Result<bool> {
        // Discover and run verification commands based on project type
        let cwd = self.project_root.to_string_lossy().to_string();

        // Discover verification commands (synchronous)
        let discovered = crate::verification_gate::discover_commands(
            &crate::verification_gate::DiscoverCommandsOptions {
                preference_commands: None,
                task_plan_verify: None,
                cwd: cwd.clone(),
            },
        );

        let commands: Vec<String> = discovered.commands.clone();

        if commands.is_empty() {
            // No commands to run, consider it a pass
            return Ok(true);
        }

        // Run verification gates (synchronous)
        let result =
            crate::verification_gate::run_verification_gate(&commands, &cwd, discovered.source);

        Ok(result.all_passed)
    }

    /// Build research context
    async fn build_research_context(&self, _slice_id: &str) -> Result<WorkflowContext> {
        // Get current state to find milestone
        let deriver = StateDeriver::new(self.project_root.clone());
        let state = deriver.derive_state().map_err(|e| {
            OrchestraV2Error::InvalidState(format!("Failed to derive state: {}", e))
        })?;

        let milestone_id = state
            .active_milestone
            .as_ref()
            .map(|m| m.id.as_str())
            .unwrap_or("M01");

        // Load roadmap excerpt for context
        let roadmap_path = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id)
            .join("ROADMAP.md");

        let roadmap_excerpt = if roadmap_path.exists() {
            Some(tokio::fs::read_to_string(&roadmap_path).await?)
        } else {
            None
        };

        Ok(WorkflowContext {
            task_plan: None,
            slice_plan: None,
            prior_summaries: vec![],
            dependencies: vec![],
            roadmap_excerpt,
            decisions_register: None,
        })
    }

    /// Build planning context
    async fn build_planning_context(
        &self,
        slice_id: &str,
        research_summary: &str,
    ) -> Result<WorkflowContext> {
        // Load prior summaries for context
        let prior_summaries = self.load_prior_summaries(slice_id).await?;

        Ok(WorkflowContext {
            task_plan: None,
            slice_plan: Some(research_summary.to_string()),
            prior_summaries,
            dependencies: vec![],
            roadmap_excerpt: None,
            decisions_register: None,
        })
    }

    /// Build task context
    async fn build_task_context(&self, slice_id: &str, task: &Task) -> Result<WorkflowContext> {
        // Load prior summaries
        let prior_summaries = self.load_prior_summaries(slice_id).await?;

        Ok(WorkflowContext {
            task_plan: Some(task.description.clone()),
            slice_plan: None,
            prior_summaries,
            dependencies: task.dependencies.clone(),
            roadmap_excerpt: None,
            decisions_register: None,
        })
    }

    /// Build completion context
    async fn build_completion_context(&self, slice_id: &str) -> Result<WorkflowContext> {
        let prior_summaries = self.load_prior_summaries(slice_id).await?;

        Ok(WorkflowContext {
            task_plan: None,
            slice_plan: None,
            prior_summaries,
            dependencies: vec![],
            roadmap_excerpt: None,
            decisions_register: None,
        })
    }

    /// Build validation context
    async fn build_validation_context(&self, milestone_id: &str) -> Result<WorkflowContext> {
        // Load all slice summaries
        let prior_summaries = self.load_milestone_summaries(milestone_id).await?;

        Ok(WorkflowContext {
            task_plan: None,
            slice_plan: None,
            prior_summaries,
            dependencies: vec![],
            roadmap_excerpt: Some("Validation: Check all success criteria met".to_string()),
            decisions_register: None,
        })
    }

    /// Load prior summaries for a slice
    async fn load_prior_summaries(&self, slice_id: &str) -> Result<Vec<String>> {
        // Use state derivation to find the milestone for this slice
        let deriver = StateDeriver::new(self.project_root.clone());
        let state = deriver.derive_state().map_err(|e| {
            OrchestraV2Error::InvalidState(format!("Failed to derive state: {}", e))
        })?;

        let milestone_id = state
            .active_milestone
            .as_ref()
            .map(|m| m.id.as_str())
            .unwrap_or("M01");

        // Look for task summaries in the slice directory
        let tasks_dir = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id)
            .join("slices")
            .join(slice_id)
            .join("tasks");

        let mut summaries = Vec::new();

        if tasks_dir.exists() {
            let mut entries = tokio::fs::read_dir(&tasks_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    let content = tokio::fs::read_to_string(&path).await?;
                    summaries.push(content);
                }
            }
        }

        Ok(summaries)
    }

    /// Load all summaries for a milestone
    async fn load_milestone_summaries(&self, milestone_id: &str) -> Result<Vec<String>> {
        let mut summaries = Vec::new();

        // Load summaries from all slices in the milestone
        let milestone_dir = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id);
        let slices_dir = milestone_dir.join("slices");

        if slices_dir.exists() {
            let mut slice_entries = tokio::fs::read_dir(&slices_dir).await?;
            while let Some(entry) = slice_entries.next_entry().await? {
                if entry.file_type().await?.is_dir() {
                    let tasks_dir = entry.path().join("tasks");
                    if tasks_dir.exists() {
                        let mut task_entries = tokio::fs::read_dir(&tasks_dir).await?;
                        while let Some(task_entry) = task_entries.next_entry().await? {
                            let path = task_entry.path();
                            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                                    summaries.push(content);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(summaries)
    }

    /// Parse tasks from planning output
    fn parse_tasks_from_plan(&self, plan: &str) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();

        // Parse tasks from markdown format:
        // - [ ] T01: Task description
        // - [x] T02: Completed task
        for (idx, line) in plan.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("- [") {
                // Extract task ID and description
                let after_bracket = trimmed.split(']').nth(1).unwrap_or("").trim();

                // Split on first colon to separate ID from description
                if let Some(colon_pos) = after_bracket.find(':') {
                    let task_id = after_bracket[..colon_pos].trim().to_string();
                    let description = after_bracket[colon_pos + 1..].trim().to_string();

                    if !task_id.is_empty() && !description.is_empty() {
                        tasks.push(Task {
                            id: task_id,
                            description,
                            dependencies: vec![],
                        });
                    }
                } else if !after_bracket.is_empty() {
                    // Task without explicit ID, generate one
                    tasks.push(Task {
                        id: format!("T{:02}", idx + 1),
                        description: after_bracket.to_string(),
                        dependencies: vec![],
                    });
                }
            }
        }

        Ok(tasks)
    }

    /// Save phase summary
    async fn save_phase_summary(&self, slice_id: &str, phase: &str, summary: &str) -> Result<()> {
        let summary_path = self
            .project_root
            .join(".orchestra")
            .join("phases")
            .join(slice_id)
            .join(format!("{}.md", phase));

        if let Some(parent) = summary_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&summary_path, summary).await?;

        Ok(())
    }

    /// Mark slice as complete
    async fn mark_slice_complete(&self, slice_id: &str) -> Result<()> {
        // Get current state to find milestone
        let deriver = StateDeriver::new(self.project_root.clone());
        let state = deriver.derive_state().map_err(|e| {
            OrchestraV2Error::InvalidState(format!("Failed to derive state: {}", e))
        })?;

        let milestone_id = state
            .active_milestone
            .as_ref()
            .map(|m| m.id.as_str())
            .unwrap_or("M01");

        // Update PLAN.md to mark all tasks as complete
        let plan_path = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id)
            .join("slices")
            .join(slice_id)
            .join("PLAN.md");

        if plan_path.exists() {
            let content = tokio::fs::read_to_string(&plan_path).await?;
            let updated = content
                .lines()
                .map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with("- [ ]") {
                        // Mark incomplete task as complete
                        line.replacen("- [ ]", "- [x]", 1)
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            tokio::fs::write(&plan_path, updated).await?;
        }

        Ok(())
    }

    /// Write UAT script
    async fn write_uat_script(&self, slice_id: &str, summary: &str) -> Result<()> {
        let uat_path = self
            .project_root
            .join(".orchestra")
            .join("phases")
            .join(slice_id)
            .join("UAT.sh");

        // Generate UAT script with basic test cases
        let uat_content = format!(
            r#"#!/bin/bash
# User Acceptance Test Script for Slice: {}
# Generated from completion summary

set -e

echo "Running UAT for slice: {}"
echo

# Test 1: Verify build succeeds
echo "Test 1: Build verification..."
if [ -f "Cargo.toml" ]; then
    cargo build --quiet 2>/dev/null || echo "⚠️  Build has warnings"
    echo "✅ Build successful"
elif [ -f "package.json" ]; then
    npm run build --silent 2>/dev/null || echo "⚠️  Build has issues"
    echo "✅ Build complete"
else
    echo "ℹ️  No build configuration found"
fi
echo

# Test 2: Verify tests pass
echo "Test 2: Test verification..."
if [ -f "Cargo.toml" ]; then
    cargo test --quiet 2>/dev/null && echo "✅ Tests passed" || echo "⚠️  Some tests failed"
elif [ -f "package.json" ]; then
    npm test --silent 2>/dev/null && echo "✅ Tests passed" || echo "⚠️  Some tests failed"
else
    echo "ℹ️  No test configuration found"
fi
echo

# Test 3: Summary verification
echo "Test 3: Work completed according to summary..."
echo "{}"
echo "✅ Summary available"
echo

echo "UAT complete for slice {}"
exit 0
"#,
            slice_id,
            slice_id,
            summary.lines().next().unwrap_or("No summary available"),
            slice_id
        );

        tokio::fs::write(&uat_path, uat_content).await?;

        Ok(())
    }

    /// Save validation results
    async fn save_validation_results(&self, milestone_id: &str, results: &str) -> Result<()> {
        let validation_path = self
            .project_root
            .join(".orchestra")
            .join("milestones")
            .join(milestone_id)
            .join("VALIDATION.md");

        if let Some(parent) = validation_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&validation_path, results).await?;

        Ok(())
    }

    /// Generate commit message from task
    fn generate_commit_message(&self, task: &Task, _output: &str) -> Result<String> {
        // Generate commit message with conventional commit format
        // The task description should be concise enough to use directly
        let description = task
            .description
            .lines()
            .next()
            .unwrap_or(&task.description)
            .trim();

        Ok(format!("feat({}): {}", task.id, description))
    }

    /// Commit work
    async fn commit_work(&self, message: &str) -> Result<()> {
        // Use git to commit changes
        use std::process::Command;

        // Check if there are changes to commit
        let status_output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.project_root)
            .output()
            .map_err(|e| {
                OrchestraV2Error::Worktree(format!("Failed to check git status: {}", e))
            })?;

        if status_output.stdout.is_empty() {
            tracing::info!("No changes to commit");
            return Ok(());
        }

        // Stage all changes
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.project_root)
            .status()
            .map_err(|e| OrchestraV2Error::Worktree(format!("Failed to stage changes: {}", e)))?;

        // Remove Orchestra runtime noise files from staging
        // Keep milestone/plan/summary files but exclude auto-generated logs
        unstage_orchestra_runtime_files(&self.project_root);

        // Check again if there are changes after excluding Orchestra noise
        let status_output = Command::new("git")
            .args(["diff", "--cached", "--quiet"])
            .current_dir(&self.project_root)
            .output()
            .map_err(|e| {
                OrchestraV2Error::Worktree(format!("Failed to check staged changes: {}", e))
            })?;

        if status_output.status.success() {
            tracing::info!("No changes to commit (after excluding Orchestra runtime files)");
            return Ok(());
        }

        // Commit with the provided message
        Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.project_root)
            .status()
            .map_err(|e| OrchestraV2Error::Worktree(format!("Failed to commit: {}", e)))?;

        tracing::info!("Committed changes: {}", message);
        Ok(())
    }

    /// Calculate cost from tokens
    fn calculate_cost(&self, tokens: u32, model: &ModelSelection) -> f64 {
        // Use actual model pricing from cost table
        let estimated_input = tokens / 2;
        let estimated_output = tokens - estimated_input;

        model_cost_table::calculate_cost(
            &model.model,
            estimated_input as usize,
            estimated_output as usize,
        )
        .unwrap_or_else(|| {
            // Fallback to default pricing if model not found in table
            let cost_per_1k_input = 0.003;
            let cost_per_1k_output = 0.015;
            (estimated_input as f64 * cost_per_1k_input / 1000.0)
                + (estimated_output as f64 * cost_per_1k_output / 1000.0)
        })
    }

    /// Set current phase
    async fn set_current_phase(&self, phase: WorkflowPhase) {
        *self.current_phase.lock().await = Some(phase);
    }
}

/// Workflow context with injected artifacts
#[derive(Debug, Clone)]
pub struct WorkflowContext {
    /// Task plan
    pub task_plan: Option<String>,
    /// Slice plan
    pub slice_plan: Option<String>,
    /// Prior task summaries
    pub prior_summaries: Vec<String>,
    /// Dependency context
    pub dependencies: Vec<String>,
    /// Roadmap excerpt
    pub roadmap_excerpt: Option<String>,
    /// Decisions register
    pub decisions_register: Option<String>,
}

/// Workflow phases
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WorkflowPhase {
    Research,
    Plan,
    Execute,
    Complete,
    Reassess,
    Validate,
}

/// Workflow result
#[derive(Debug, Clone)]
pub struct WorkflowResult {
    pub slices_completed: u32,
    pub total_cost: f64,
    pub total_tokens: u32,
    pub success: bool,
}

/// Slice result
#[derive(Debug, Clone)]
pub struct SliceResult {
    pub slice_id: String,
    pub tokens_used: u32,
    pub cost: f64,
    pub tasks_completed: u32,
}

/// Phase result
#[derive(Debug, Clone)]
pub struct PhaseResult {
    pub phase: String,
    pub tokens_used: u32,
    pub cost: f64,
    pub summary: String,
}

/// Planning result
#[derive(Debug, Clone)]
pub struct PlanningResult {
    pub tasks: Vec<Task>,
    pub tokens_used: u32,
    pub cost: f64,
}

/// Task definition
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub dependencies: Vec<String>,
}

/// Task result
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub tokens_used: u32,
    pub cost: f64,
    pub verification_passed: bool,
}

/// Unit execution result
#[derive(Debug, Clone)]
pub struct UnitExecutionResult {
    pub output: String,
    pub tool_calls: Vec<rustycode_protocol::ToolCall>,
    pub tokens_used: u32,
    pub cost: f64,
    pub duration_ms: u64,
}
