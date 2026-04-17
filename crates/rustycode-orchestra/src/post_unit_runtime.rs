//! Orchestra Post-Unit Runtime — deterministic closeout for executed units.
//!
//! Owns the runtime-side bookkeeping that should not live inline in the
//! main executor loop: execution stats aggregation, metrics/budget
//! application, and successful task finalization.

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use chrono::Utc;
use tracing::{error, info, warn};
use walkdir::WalkDir;

use crate::budget::{BudgetAction, BudgetTracker, MetricsLedger, UnitMetrics};
use crate::git_self_heal::unstage_orchestra_runtime_files;
use crate::model_cost_table::calculate_cost;
use crate::state_derivation::{OrchestraState, StateDeriver};

#[derive(Debug, Clone, Default)]
pub struct ExecutionStats {
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub total_tokens: u32,
}

impl ExecutionStats {
    pub fn add(&mut self, other: &ExecutionStats) {
        self.tokens_in = self.tokens_in.saturating_add(other.tokens_in);
        self.tokens_out = self.tokens_out.saturating_add(other.tokens_out);
        self.total_tokens = self.total_tokens.saturating_add(other.total_tokens);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TaskExecutionOutcome {
    pub passed: bool,
}

pub fn apply_metrics_and_budget(
    metrics_ledger: &MetricsLedger,
    budget_tracker: &Mutex<BudgetTracker>,
    model: &str,
    unit_id: &str,
    execution_stats: &ExecutionStats,
    duration_ms: u64,
    succeeded: bool,
) -> Result<f64> {
    let estimated_cost = calculate_cost(
        model,
        execution_stats.tokens_in as usize,
        execution_stats.tokens_out as usize,
    )
    .unwrap_or(0.0);

    let metrics = UnitMetrics {
        unit_id: unit_id.to_string(),
        timestamp: Utc::now(),
        tokens_in: execution_stats.tokens_in,
        tokens_out: execution_stats.tokens_out,
        total_tokens: execution_stats.total_tokens,
        cost: estimated_cost,
        duration_ms,
        succeeded,
    };

    if let Err(e) = metrics_ledger.record(metrics) {
        warn!("Failed to record metrics: {}", e);
    }

    let mut budget_tracker = budget_tracker.lock().unwrap_or_else(|e| e.into_inner());
    match budget_tracker.record_cost(estimated_cost) {
        BudgetAction::Continue => {
            info!(
                "💰 Budget updated: ${:.4} spent, ${:.2} remaining",
                budget_tracker.total_spent(),
                budget_tracker.remaining()
            );
        }
        BudgetAction::Warn {
            level,
            spent,
            budget,
        } => {
            warn!(
                "⚠️  Budget warning ({:?}): ${:.4} of ${:.2} spent",
                level, spent, budget
            );
        }
        BudgetAction::Stop { spent, budget } => {
            error!("🛑 Budget exceeded: ${:.4} of ${:.2} spent", spent, budget);
            return Err(anyhow!("Budget exceeded"));
        }
    }

    Ok(estimated_cost)
}

pub async fn finalize_successful_task(
    project_root: &Path,
    unit_id: &str,
    state: &OrchestraState,
) -> Result<()> {
    if let Some(ref task) = state.active_task {
        let slice_dir = task
            .path
            .parent()
            .and_then(|path| path.parent())
            .ok_or_else(|| anyhow!("Task path missing slice directory for {}", unit_id))?;
        let plan_path = slice_dir.join("PLAN.md");

        if plan_path.exists() {
            mark_task_done(&plan_path, unit_id)?;
            info!("✅ Task {} marked as done in PLAN.md", unit_id);
        }
    }

    commit_task(project_root, unit_id)?;
    write_state_cache(project_root).await?;
    Ok(())
}

pub async fn write_task_summary(project_root: &Path, unit_id: &str, summary: &str) -> Result<()> {
    let orchestra_dir = project_root.join(".orchestra");

    let summaries = WalkDir::new(&orchestra_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .to_string_lossy()
                .contains(&format!("tasks/{}", unit_id))
        })
        .collect::<Vec<_>>();

    if !summaries.is_empty() {
        let task_dir = summaries[0].path();
        let summary_path = task_dir.join(format!("{}-SUMMARY.md", unit_id));
        let content = format!(
            "# Summary: {}\n\n**Completed:** {}\n\n{}\n\n## Artifacts Created\n- Task completed successfully\n",
            unit_id,
            chrono::Utc::now().format("%Y-%m-%d"),
            summary
        );
        std::fs::write(&summary_path, content)?;
        info!("📝 Summary written: {:?}", summary_path);
    }

    Ok(())
}

fn mark_task_done(plan_path: &Path, unit_id: &str) -> Result<()> {
    let content = std::fs::read_to_string(plan_path)?;

    let mut updated = String::new();
    for line in content.lines() {
        if line.contains(&format!("- [ ] {}:", unit_id)) {
            updated.push_str(&line.replace("- [ ]", "- [x]"));
        } else if line.contains(&format!("- [{}](", unit_id)) {
            updated.push_str(&line.replacen(
                &format!("- [{}](", unit_id),
                &format!("- [x][{}](", unit_id),
                1,
            ));
        } else {
            updated.push_str(line);
        }
        updated.push('\n');
    }

    std::fs::write(plan_path, updated)?;
    Ok(())
}

fn commit_task(project_root: &Path, unit_id: &str) -> Result<()> {
    let git_dir = project_root.join(".git");
    if !git_dir.exists() {
        info!("Not a git repository, skipping commit");
        return Ok(());
    }

    // Stage all files (git will respect .gitignore for untracked files)
    let output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Remove Orchestra runtime noise files from staging
    // Keep milestone/plan/summary files but exclude auto-generated logs
    unstage_orchestra_runtime_files(project_root);

    let output = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(project_root)
        .output()?;

    if output.status.success() {
        info!("No changes to commit");
        return Ok(());
    }

    let commit_message = format!("feat({}): Complete task", unit_id);
    let output = Command::new("git")
        .args(["commit", "-m", &commit_message])
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    info!("✅ Committed changes for task: {}", unit_id);
    Ok(())
}

async fn write_state_cache(project_root: &Path) -> Result<()> {
    let deriver = StateDeriver::new(project_root.to_path_buf());
    let state = deriver.derive_state()?;

    let state_path = project_root.join(".orchestra/STATE.md");
    let mut content = String::new();

    content.push_str("# Orchestra State\n\n");
    content.push_str("<!-- Auto-generated. Updated by deriveState(). -->\n\n");
    content.push_str(&format!("**Phase:** {}\n\n", state.phase.as_str()));

    for milestone in &state.milestones {
        let done_slices = milestone.slices.iter().filter(|s| s.done).count();
        let total_slices = milestone.slices.len();
        content.push_str(&format!("## {} {}\n", milestone.id, milestone.title));
        content.push_str(&format!("- Slices: {}/{}\n", done_slices, total_slices));
        content.push('\n');
    }

    if let Some(ref task) = state.active_task {
        content.push_str(&format!("## Active Task: {}\n\n", task.id));
        content.push_str(&format!("**Title:** {}\n\n", task.title));
        if task.done {
            content.push_str("**Status:** Complete ✅\n\n");
        } else {
            content.push_str("**Status:** In progress 🔄\n\n");
        }
    } else if let Some(ref slice) = state.active_slice {
        content.push_str(&format!("## Active Slice: {}\n\n", slice.id));
        content.push_str(&format!("**Title:** {}\n\n", slice.title));
        content.push_str("**Status:** Awaiting task execution\n\n");
    }

    tokio::fs::write(&state_path, content).await?;
    info!("📝 STATE.md updated");
    Ok(())
}
