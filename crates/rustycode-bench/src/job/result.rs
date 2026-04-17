//! Benchmark results aggregation and statistics.

use std::fmt::Write;

use serde::{Deserialize, Serialize};

use crate::trial::TrialResult;

/// Aggregated results from a benchmark job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResults {
    /// Total number of trials.
    pub total: usize,
    /// Number of trials that passed (reward >= 0.5).
    pub passed: usize,
    /// Number of trials that failed (reward < 0.5).
    pub failed: usize,
    /// Number of trials that had infrastructure errors.
    pub errors: usize,
    /// Overall accuracy (passed / total).
    pub accuracy: f64,
    /// Mean reward across all trials.
    pub mean_reward: f64,
    /// Individual trial results.
    pub trials: Vec<TrialResult>,
    /// Per-task breakdown.
    pub task_results: Vec<TaskResult>,
}

/// Aggregated result for a single task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task name.
    pub task_name: String,
    /// Agent used.
    pub agent_name: String,
    /// Reward score.
    pub reward: f64,
    /// Whether the task passed.
    pub passed: bool,
    /// Error message if any.
    pub error: Option<String>,
    /// Duration in seconds.
    pub duration_secs: f64,
}

impl BenchmarkResults {
    /// Create results from a list of trial results.
    #[allow(clippy::cast_precision_loss)]
    pub fn from_trials(trials: &[TrialResult]) -> Self {
        let total = trials.len();
        let passed = trials.iter().filter(|t| t.passed()).count();
        let failed = trials.iter().filter(|t| t.success && !t.passed()).count();
        let errors = trials.iter().filter(|t| !t.success).count();

        let accuracy = if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        };

        let mean_reward = if total > 0 {
            trials.iter().map(|t| t.reward).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let task_results = trials
            .iter()
            .map(|t| TaskResult {
                task_name: t.task_name.clone(),
                agent_name: t.agent_name.clone(),
                reward: t.reward,
                passed: t.passed(),
                error: t.error.clone(),
                duration_secs: t.duration_secs,
            })
            .collect();

        Self {
            total,
            passed,
            failed,
            errors,
            accuracy,
            mean_reward,
            trials: trials.to_vec(),
            task_results,
        }
    }

    /// Human-readable summary of results.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "Benchmark Results: {}/{} passed ({:.1}%)",
            self.passed,
            self.total,
            self.accuracy * 100.0
        );
        if self.errors > 0 {
            let _ = write!(s, ", {} failed, {} errors", self.failed, self.errors);
        } else if self.failed > 0 {
            let _ = write!(s, ", {} failed", self.failed);
        }
        let _ = write!(s, "\nMean reward: {:.3}", self.mean_reward);

        // Show failed tasks
        let failed_tasks: Vec<&TaskResult> =
            self.task_results.iter().filter(|t| !t.passed).collect();

        if !failed_tasks.is_empty() {
            s.push_str("\n\nFailed tasks:");
            for task in failed_tasks {
                let reason = task.error.as_deref().unwrap_or("reward < 0.5");
                let _ = write!(s, "\n  - {} ({})", task.task_name, reason);
            }
        }

        s
    }
}
