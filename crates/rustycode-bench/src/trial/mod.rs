//! Trial lifecycle — orchestrates a single benchmark task run.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing;

use crate::agent::BenchAgent;
use crate::environment::docker::{DockerEnvironment, EnvironmentConfig, TrialPaths};
use crate::environment::BenchEnvironment;
use crate::task::ResolvedTask;
use crate::verifier::Verifier;

/// Result of a single benchmark trial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialResult {
    /// Task name.
    pub task_name: String,
    /// Agent that was used.
    pub agent_name: String,
    /// Reward score (0.0 to 1.0).
    pub reward: f64,
    /// Whether the trial completed without infrastructure errors.
    pub success: bool,
    /// Error message if the trial failed.
    pub error: Option<String>,
    /// Duration of the trial in seconds.
    pub duration_secs: f64,
    /// Path to the trial output directory.
    pub trial_dir: PathBuf,
}

impl TrialResult {
    /// Whether the task passed (reward >= 0.5).
    pub fn passed(&self) -> bool {
        self.reward >= 0.5
    }
}

/// Orchestrates a single benchmark trial: environment → agent → verifier → cleanup.
pub struct Trial {
    /// Unique session ID for this trial.
    session_id: String,
    /// Root directory for all trial outputs.
    jobs_dir: PathBuf,
    /// Whether to force-build images (needed for aarch64).
    force_build: bool,
    /// Whether to delete containers/images after the trial.
    cleanup: bool,
}

impl Trial {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(session_id: String, jobs_dir: PathBuf) -> Self {
        Self {
            session_id,
            jobs_dir,
            force_build: true,
            cleanup: true,
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn with_force_build(mut self, force: bool) -> Self {
        self.force_build = force;
        self
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn with_cleanup(mut self, cleanup: bool) -> Self {
        self.cleanup = cleanup;
        self
    }

    /// Run a single trial for the given task.
    pub async fn run(
        &self,
        task: &ResolvedTask,
        agent: &mut dyn BenchAgent,
        verifier: &dyn Verifier,
    ) -> TrialResult {
        let start = Instant::now();
        let trial_name = format!("{}-{}", task.name, self.session_id);
        let trial_dir = self.jobs_dir.join(&trial_name);

        tracing::info!("Starting trial: {}", trial_name);

        let trial_result = self
            .run_inner(task, agent, verifier, &trial_name, &trial_dir)
            .await;

        let duration = start.elapsed().as_secs_f64();

        match trial_result {
            Ok(reward) => TrialResult {
                task_name: task.name.clone(),
                agent_name: agent.name().to_string(),
                reward,
                success: true,
                error: None,
                duration_secs: duration,
                trial_dir,
            },
            Err(e) => {
                tracing::error!("Trial {} failed: {}", trial_name, e);
                TrialResult {
                    task_name: task.name.clone(),
                    agent_name: agent.name().to_string(),
                    reward: 0.0,
                    success: false,
                    error: Some(e.to_string()),
                    duration_secs: duration,
                    trial_dir,
                }
            }
        }
    }

    /// Inner trial execution with full lifecycle management.
    async fn run_inner(
        &self,
        task: &ResolvedTask,
        agent: &mut dyn BenchAgent,
        verifier: &dyn Verifier,
        trial_name: &str,
        trial_dir: &Path,
    ) -> anyhow::Result<f64> {
        // Set up trial paths
        let trial_paths = TrialPaths::new(trial_dir.to_path_buf());
        trial_paths.create_dirs()?;

        // Save instruction to agent logs
        let instruction_path = trial_paths.agent_dir.join("instruction.md");
        std::fs::write(&instruction_path, &task.instruction)?;

        // Create environment config from task
        let env_config = EnvironmentConfig {
            environment_dir: task.environment_dir.clone(),
            cpus: task.config.environment.cpus,
            memory: task.config.environment.memory.clone(),
            docker_image: task.config.environment.docker_image.clone(),
            build_timeout_secs: task.config.environment.build_timeout_sec as u64,
        };

        let mut env = DockerEnvironment::new(trial_name.to_string(), env_config, trial_paths);

        // Start container
        env.start(self.force_build).await?;

        // Ensure container is stopped when we're done
        let result = self.execute_phases(task, agent, verifier, &mut env).await;

        // Always stop the container
        if let Err(e) = env.stop(self.cleanup).await {
            tracing::warn!("Container cleanup failed: {}", e);
        }

        result
    }

    /// Execute the three phases: agent setup, agent run, verification.
    async fn execute_phases(
        &self,
        task: &ResolvedTask,
        agent: &mut dyn BenchAgent,
        verifier: &dyn Verifier,
        env: &mut dyn BenchEnvironment,
    ) -> anyhow::Result<f64> {
        // Phase 1: Agent setup
        tracing::info!("[{}] Agent setup ({})...", task.name, agent.name());
        agent.setup(env).await?;

        // Phase 2: Agent execution
        tracing::info!("[{}] Agent run...", task.name);
        agent.run(&task.instruction, env).await?;

        // Phase 3: Verification
        tracing::info!("[{}] Verification...", task.name);
        let reward = verifier.verify(env).await?;

        tracing::info!(
            "[{}] Complete — reward: {} ({})",
            task.name,
            reward,
            if reward >= 0.5 { "PASS" } else { "FAIL" }
        );

        Ok(reward)
    }
}
