// rustycode-orchestra/src/engine.rs
//! Workflow engine for Orchestra v2
//!
//! Orchestrates phases, waves, and tasks with parallel execution

use tracing::debug;

use crate::{
    error::{OrchestraV2Error, Result},
    llm::LlmClient,
    state::StateManager,
    tools::ToolExecutor,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::task::JoinSet;

/// Workflow engine for Orchestra v2
pub struct WorkflowEngine {
    /// Project root directory
    project_root: PathBuf,
    /// State manager
    state_manager: StateManager,
    /// LLM client
    llm_client: LlmClient,
    /// Tool executor
    tool_executor: ToolExecutor,
    /// Auto mode enabled
    auto_mode: bool,
}

impl WorkflowEngine {
    /// Create a new workflow engine
    pub fn new(
        project_root: PathBuf,
        llm_client: LlmClient,
        tool_executor: ToolExecutor,
        auto_mode: bool,
    ) -> Result<Self> {
        let state_manager = StateManager::new(&project_root)?;

        Ok(Self {
            project_root,
            state_manager,
            llm_client,
            tool_executor,
            auto_mode,
        })
    }

    /// Execute a phase with all its waves
    pub async fn execute_phase(&self, phase_id: &str) -> Result<PhaseExecutionResult> {
        let phase = self.load_phase(phase_id).await?;
        let mut wave_results = Vec::new();

        for (wave_index, wave) in phase.waves.iter().enumerate() {
            // Check checkpoint before executing wave
            if wave.checkpoint && !self.auto_mode {
                self.confirm_checkpoint(&phase, wave_index).await?;
            }

            // Execute wave
            let wave_result = self.execute_wave(&phase, wave, wave_index).await?;
            wave_results.push(wave_result);

            // Update state after each wave
            self.update_wave_progress(&phase.id, wave_index).await?;

            // Auto-advance if enabled
            if self.auto_mode {
                continue;
            }
        }

        // Mark phase as complete
        self.complete_phase(&phase.id).await?;

        Ok(PhaseExecutionResult {
            phase_id: phase.id.clone(),
            wave_results,
            completed_at: Utc::now(),
        })
    }

    /// Execute a single wave with parallel tasks
    async fn execute_wave(
        &self,
        _phase: &Phase,
        wave: &Wave,
        wave_index: usize,
    ) -> Result<WaveExecutionResult> {
        let mut task_results = Vec::new();

        // Execute tasks in parallel
        let mut join_set = JoinSet::new();

        for task in &wave.tasks {
            let task = task.clone();
            let llm_client = self.llm_client.clone();
            let tool_executor = self.tool_executor.clone();
            let project_root = self.project_root.clone();

            join_set.spawn(async move {
                Self::execute_task_internal(project_root, task, llm_client, tool_executor).await
            });
        }

        // Collect results
        while let Some(result) = join_set.join_next().await {
            let result = result.map_err(|e| OrchestraV2Error::TaskExecution(e.to_string()))??;
            task_results.push(result);
        }

        Ok(WaveExecutionResult {
            wave_id: wave.id,
            wave_index,
            task_results,
            completed_at: Utc::now(),
        })
    }

    /// Execute a single task
    async fn execute_task_internal(
        project_root: PathBuf,
        task: Task,
        llm_client: LlmClient,
        tool_executor: ToolExecutor,
    ) -> Result<TaskExecutionResult> {
        // Build execution context
        let context = TaskContext {
            project_root,
            task_id: task.id.clone(),
            objective: task.objective.clone(),
        };

        // Execute task via LLM with tool use
        let result = if task.autonomous {
            llm_client.execute_autonomous_task(&context).await?
        } else {
            llm_client.execute_guided_task(&context).await?
        };

        // Execute tools as requested by LLM
        for tool_call in &result.tool_calls {
            // Convert BackwardCompatToolCall to protocol ToolCall
            let protocol_tool_call = rustycode_protocol::ToolCall::new(
                format!("tool-{}", task.id),
                &tool_call.tool_name,
                tool_call.parameters.clone(),
            );
            tool_executor.execute_tool(&protocol_tool_call)?;
        }

        Ok(TaskExecutionResult {
            task_id: task.id,
            status: TaskStatus::Completed,
            output: result.output,
            tool_calls: result
                .tool_calls
                .into_iter()
                .map(|tc| EngineToolCall {
                    tool_name: tc.tool_name,
                    parameters: tc.parameters,
                    result: String::new(), // Will be filled after execution
                })
                .collect(),
            completed_at: Utc::now(),
        })
    }

    /// Load phase from disk
    async fn load_phase(&self, phase_id: &str) -> Result<Phase> {
        let phase_path = self
            .project_root
            .join(".orchestra")
            .join("phases")
            .join(phase_id)
            .join("PLAN.md");

        let content = tokio::fs::read_to_string(&phase_path).await?;
        Self::parse_phase_plan(&content)
    }

    /// Parse phase plan from markdown
    fn parse_phase_plan(content: &str) -> Result<Phase> {
        // Parse frontmatter and content
        let (frontmatter, _body) = content
            .split_once("\n---\n")
            .ok_or_else(|| OrchestraV2Error::Parse("Missing frontmatter separator".to_string()))?;

        let phase_meta: PhaseMetadata = serde_yaml::from_str(frontmatter)
            .map_err(|e| OrchestraV2Error::Parse(format!("Invalid frontmatter: {}", e)))?;

        // Parse waves from content
        let waves = Self::parse_waves(content)?;

        Ok(Phase {
            id: phase_meta.id,
            name: phase_meta.name,
            goal: phase_meta.goal,
            requirements: phase_meta.requirements,
            dependencies: phase_meta.dependencies,
            waves,
        })
    }

    /// Parse waves from phase plan content
    fn parse_waves(content: &str) -> Result<Vec<Wave>> {
        // Simple wave parser - looks for "## Wave X" headings
        let mut waves = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        let mut current_wave: Option<Wave> = None;
        let mut current_tasks: Vec<Task> = Vec::new();
        let mut wave_id = 0;

        for line in lines.iter() {
            if let Some(rest) = line.strip_prefix("## Wave ") {
                // Save previous wave
                if let Some(wave) = current_wave.take() {
                    waves.push(wave);
                }

                // Parse wave number
                wave_id = rest.trim().parse().unwrap_or_else(|_| waves.len() + 1);

                current_tasks.clear();
            } else if let Some(task_line) = line.strip_prefix("- **") {
                // Parse task: "- **Task ID**: Description"
                let parts: Vec<&str> = task_line.split("**: ").collect();
                if parts.len() >= 2 {
                    let task_id = parts[0].trim().to_string();
                    let objective = parts[1].trim().to_string();

                    current_tasks.push(Task {
                        id: task_id,
                        objective,
                        autonomous: false,
                        dependencies: Vec::new(),
                        status: TaskStatus::Pending,
                    });
                }
            }
        }

        // Save last wave
        if !current_tasks.is_empty() {
            waves.push(Wave {
                id: wave_id,
                tasks: current_tasks,
                checkpoint: false,
            });
        }

        Ok(waves)
    }

    /// Confirm checkpoint with user
    async fn confirm_checkpoint(&self, phase: &Phase, wave_index: usize) -> Result<()> {
        // In interactive mode, this would pause and wait for user confirmation
        // For now, we'll just log it
        debug!(
            "Checkpoint: Phase {} Wave {} completed. Continue? (y/n)",
            phase.id, wave_index
        );
        Ok(())
    }

    /// Update wave progress in state
    async fn update_wave_progress(&self, _phase_id: &str, wave_index: usize) -> Result<()> {
        let mut state = self.state_manager.read_state()?;
        state.execution.active_wave = Some(wave_index);
        self.state_manager.write_state(&state)?;
        Ok(())
    }

    /// Mark phase as complete
    async fn complete_phase(&self, _phase_id: &str) -> Result<()> {
        let mut state = self.state_manager.read_state()?;
        state.execution.active_phase = None;
        state.execution.active_wave = None;
        self.state_manager.write_state(&state)?;
        Ok(())
    }
}

/// Phase definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    /// Phase ID (e.g., "01-foundation")
    pub id: String,
    /// Phase name
    pub name: String,
    /// Phase goal
    pub goal: String,
    /// Requirements covered by this phase
    pub requirements: Vec<String>,
    /// Dependencies on other phases
    pub dependencies: Vec<String>,
    /// Waves in this phase
    pub waves: Vec<Wave>,
}

/// Wave definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wave {
    /// Wave ID (sequential number)
    pub id: usize,
    /// Tasks in this wave
    pub tasks: Vec<Task>,
    /// Whether to pause after this wave
    pub checkpoint: bool,
}

/// Task definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Task ID (e.g., "01-01")
    pub id: String,
    /// Task objective
    pub objective: String,
    /// Whether this task runs autonomously
    pub autonomous: bool,
    /// Dependencies on other tasks
    pub dependencies: Vec<String>,
    /// Task status
    pub status: TaskStatus,
}

/// Task status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum TaskStatus {
    /// Task is pending
    Pending,
    /// Task is in progress
    InProgress,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
}

/// Task execution context
#[derive(Debug, Clone)]
pub struct TaskContext {
    /// Project root directory
    pub project_root: PathBuf,
    /// Task ID
    pub task_id: String,
    /// Task objective
    pub objective: String,
}

/// Phase metadata from frontmatter
#[derive(Debug, Serialize, Deserialize)]
struct PhaseMetadata {
    id: String,
    name: String,
    goal: String,
    requirements: Vec<String>,
    dependencies: Vec<String>,
}

/// Phase execution result
#[derive(Debug, Clone)]
pub struct PhaseExecutionResult {
    pub phase_id: String,
    pub wave_results: Vec<WaveExecutionResult>,
    pub completed_at: DateTime<Utc>,
}

/// Wave execution result
#[derive(Debug, Clone)]
pub struct WaveExecutionResult {
    pub wave_id: usize,
    pub wave_index: usize,
    pub task_results: Vec<TaskExecutionResult>,
    pub completed_at: DateTime<Utc>,
}

/// Task execution result
#[derive(Debug, Clone)]
pub struct TaskExecutionResult {
    pub task_id: String,
    pub status: TaskStatus,
    pub output: String,
    pub tool_calls: Vec<EngineToolCall>,
    pub completed_at: DateTime<Utc>,
}

/// Tool call made during task execution
#[derive(Debug, Clone)]
pub struct EngineToolCall {
    pub tool_name: String,
    pub parameters: serde_json::Value,
    pub result: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_status_serde_variants() {
        for ts in &[
            TaskStatus::Pending,
            TaskStatus::InProgress,
            TaskStatus::Completed,
            TaskStatus::Failed,
        ] {
            let json = serde_json::to_string(ts).unwrap();
            let back: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(ts, &back);
        }
    }

    #[test]
    fn wave_serde_roundtrip() {
        let wave = Wave {
            id: 1,
            tasks: vec![Task {
                id: "01-01".into(),
                objective: "Setup project".into(),
                autonomous: true,
                dependencies: vec![],
                status: TaskStatus::Pending,
            }],
            checkpoint: false,
        };
        let json = serde_json::to_string(&wave).unwrap();
        let decoded: Wave = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, 1);
        assert_eq!(decoded.tasks.len(), 1);
        assert!(decoded.tasks[0].autonomous);
    }

    #[test]
    fn phase_serde_roundtrip() {
        let phase = Phase {
            id: "01-foundation".into(),
            name: "Foundation".into(),
            goal: "Set up project".into(),
            requirements: vec!["R01".into()],
            dependencies: vec![],
            waves: vec![],
        };
        let json = serde_json::to_string(&phase).unwrap();
        let decoded: Phase = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "01-foundation");
        assert!(decoded.waves.is_empty());
    }

    #[test]
    fn task_context_construction() {
        let ctx = TaskContext {
            project_root: PathBuf::from("/tmp/project"),
            task_id: "01-01".into(),
            objective: "Write tests".into(),
        };
        assert_eq!(ctx.task_id, "01-01");
    }

    #[test]
    fn engine_tool_call_construction() {
        let tc = EngineToolCall {
            tool_name: "bash".into(),
            parameters: serde_json::json!({"cmd": "cargo test"}),
            result: "passed".into(),
        };
        assert_eq!(tc.tool_name, "bash");
    }
}
