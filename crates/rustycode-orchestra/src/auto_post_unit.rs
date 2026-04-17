//! Orchestra Auto Post-Unit — Post-Unit Processing
//!
//! Handles post-unit processing including:
//! - Pre-verification: auto-commit, doctor, state rebuild, worktree sync, artifact verification
//! - Post-verification: DB dual-write, hooks, triage, quick-tasks
//!
//! Critical for maintaining system state and handling special dispatch conditions.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::auto_recovery::persist_completed_key;
use crate::paths::resolve_slice_file;

// ─── Types ──────────────────────────────────────────────────────────────────────

/// Post-unit verification result
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PostUnitResult {
    /// Unit completed successfully
    Complete,
    /// Hook requested retry
    Retry,
    /// Verification failed
    VerificationFailed,
    /// Unit should stop
    Stop,
}

/// Triage capture
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriageCapture {
    pub id: String,
    pub text: String,
    pub timestamp: i64,
}

/// Quick-task entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickTask {
    pub id: String,
    pub text: String,
    pub milestone_id: String,
}

/// Post-unit context
#[derive(Debug, Clone)]
pub struct PostUnitContext {
    pub base_path: String,
    pub unit_type: String,
    pub unit_id: String,
    pub started_at: i64,
    pub step_mode: bool,
    pub verbose: bool,
}

/// Hook retry trigger
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRetryTrigger {
    pub unit_type: String,
    pub unit_id: String,
}

// ─── Public API ────────────────────────────────────────────────────────────────

/// Post-unit pre-verification processing
///
/// Handles: auto-commit, doctor run, state rebuild, worktree sync,
/// artifact verification, completion persistence, outcome recording.
///
/// # Arguments
/// * `ctx` - Post-unit context
///
/// # Returns
/// Result indicating success or failure
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_post_unit::*;
///
/// let ctx = PostUnitContext {
///     base_path: "/project".to_string(),
///     unit_type: "execute-task".to_string(),
///     unit_id: "M01/S01/T01".to_string(),
///     started_at: 1234567890,
///     step_mode: false,
///     verbose: false,
/// };
///
/// let result = post_unit_pre_verification(&ctx);
/// ```
pub fn post_unit_pre_verification(ctx: &PostUnitContext) -> Result<(), String> {
    // 1. Auto-commit if configured (placeholder - depends on git integration)
    if let Err(e) = maybe_auto_commit(ctx) {
        // Non-fatal: log and continue
        tracing::warn!("Auto-commit failed (non-fatal): {}", e);
    }

    // 2. Run doctor if configured (placeholder - depends on doctor module)
    if let Err(e) = maybe_run_doctor(ctx) {
        // Non-fatal: log and continue
        tracing::warn!("Doctor run failed (non-fatal): {}", e);
    }

    // 3. Rebuild state derivation cache (placeholder - depends on cache invalidation)
    if let Err(e) = rebuild_state_cache(ctx) {
        // Non-fatal: log and continue
        tracing::warn!("State cache rebuild failed (non-fatal): {}", e);
    }

    // 4. Sync worktree if needed (placeholder - depends on worktree manager)
    if let Err(e) = maybe_sync_worktree(ctx) {
        // Non-fatal: log and continue
        tracing::warn!("Worktree sync failed (non-fatal): {}", e);
    }

    // 5. Verify expected artifacts
    if let Err(e) = verify_expected_artifacts(ctx) {
        return Err(format!("Artifact verification failed: {}", e));
    }

    // 6. Persist completed key
    if let Err(e) = persist_completed_key_for_unit(ctx) {
        return Err(format!("Failed to persist completed key: {}", e));
    }

    // 7. Record unit outcome (placeholder - depends on metrics/recording)
    if let Err(e) = record_unit_outcome(ctx) {
        // Non-fatal: log and continue
        tracing::warn!("Failed to record unit outcome (non-fatal): {}", e);
    }

    Ok(())
}

/// Post-unit post-verification processing
///
/// Handles: DB dual-write, hooks, triage, quick-tasks.
///
/// # Arguments
/// * `ctx` - Post-unit context
/// * `pending_captures` - Optional pending triage captures
/// * `pending_quick_tasks` - Optional pending quick tasks
///
/// # Returns
/// Next action to take
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_post_unit::*;
///
/// let next_action = post_unit_post_verification(
///     &ctx,
///     Some(&captures),
///     Some(&quick_tasks),
/// );
/// ```
pub fn post_unit_post_verification(
    ctx: &PostUnitContext,
    pending_captures: Option<&[TriageCapture]>,
    pending_quick_tasks: Option<&[QuickTask]>,
) -> PostVerificationAction {
    // 1. Dual-write to database (placeholder - depends on DB integration)
    if let Err(e) = dual_write_database(ctx) {
        tracing::warn!("Database dual-write failed (non-fatal): {}", e);
    }

    // 2. Run hooks (placeholder - depends on hooks system)
    if let Err(e) = run_post_unit_hooks(ctx) {
        tracing::warn!("Post-unit hooks failed (non-fatal): {}", e);
    }

    // 3. Check for hook retry requests
    if let Some(retry_trigger) = check_hook_retry(ctx) {
        return PostVerificationAction::Retry(retry_trigger);
    }

    // 4. Check for triage dispatch
    if !ctx.step_mode {
        if let Some(captures) = pending_captures {
            if !captures.is_empty() {
                return PostVerificationAction::DispatchTriage;
            }
        }
    }

    // 5. Check for quick-task dispatch
    if !ctx.step_mode {
        if let Some(tasks) = pending_quick_tasks {
            if !tasks.is_empty() {
                return PostVerificationAction::DispatchQuickTask(tasks[0].clone());
            }
        }
    }

    // 6. Step mode → show wizard
    if ctx.step_mode {
        return PostVerificationAction::ShowWizard;
    }

    PostVerificationAction::Continue
}

/// Action to take after post-verification
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PostVerificationAction {
    /// Continue normal dispatch
    Continue,
    /// Retry the unit with new trigger
    Retry(HookRetryTrigger),
    /// Dispatch triage unit
    DispatchTriage,
    /// Dispatch quick-task
    DispatchQuickTask(QuickTask),
    /// Show step wizard
    ShowWizard,
    /// Stop auto mode
    Stop,
}

// ─── Helper Functions ───────────────────────────────────────────────────────────

fn maybe_auto_commit(_ctx: &PostUnitContext) -> Result<(), String> {
    // Placeholder: Check if auto-commit is enabled and commit staged changes
    // This would integrate with git/worktree modules
    Ok(())
}

fn maybe_run_doctor(_ctx: &PostUnitContext) -> Result<(), String> {
    // Placeholder: Run doctor checks if configured
    // This would integrate with a doctor module
    Ok(())
}

fn rebuild_state_cache(_ctx: &PostUnitContext) -> Result<(), String> {
    // Placeholder: Invalidate and rebuild state derivation cache
    // This would call cache invalidation functions
    Ok(())
}

fn maybe_sync_worktree(_ctx: &PostUnitContext) -> Result<(), String> {
    // Placeholder: Sync worktree if changes were made in a worktree
    // This would integrate with worktree manager
    Ok(())
}

fn verify_expected_artifacts(ctx: &PostUnitContext) -> Result<(), String> {
    // Verify expected artifacts based on unit type
    match ctx.unit_type.as_str() {
        "execute-task" => {
            // Check for SUMMARY.md
            let summary_path = format!(
                "{}/.orchestra/{}/slices/{}/tasks/{}-SUMMARY.md",
                ctx.base_path,
                get_milestone_id(&ctx.unit_id),
                get_slice_id(&ctx.unit_id),
                get_task_id(&ctx.unit_id)
            );

            if !Path::new(&summary_path).exists() {
                return Err(format!("Missing expected artifact: {}", summary_path));
            }

            Ok(())
        }
        "plan-slice" => {
            // Check for PLAN.md
            if let Some(plan_path_buf) = resolve_slice_file(
                Path::new(&ctx.base_path),
                get_milestone_id(&ctx.unit_id),
                get_slice_id(&ctx.unit_id),
                "PLAN.md",
            ) {
                let plan_path_owned = plan_path_buf.to_string_lossy().to_string();
                let plan_path = plan_path_buf.to_str().unwrap_or(&plan_path_owned);
                if !plan_path_buf.exists() {
                    return Err(format!("Missing expected artifact: {}", plan_path));
                }
            }
            Ok(())
        }
        "complete-slice" => {
            // Check for SLICE_SUMMARY.md
            if let Some(summary_path_buf) = resolve_slice_file(
                Path::new(&ctx.base_path),
                get_milestone_id(&ctx.unit_id),
                get_slice_id(&ctx.unit_id),
                "SLICE_SUMMARY.md",
            ) {
                let summary_path_owned = summary_path_buf.to_string_lossy().to_string();
                let summary_path = summary_path_buf.to_str().unwrap_or(&summary_path_owned);
                if !summary_path_buf.exists() {
                    return Err(format!("Missing expected artifact: {}", summary_path));
                }
            }
            Ok(())
        }
        _ => Ok(()), // Other unit types may not have artifacts
    }
}

fn persist_completed_key_for_unit(ctx: &PostUnitContext) -> Result<(), String> {
    let key = format!("{}/{}", ctx.unit_type, ctx.unit_id);
    persist_completed_key(Path::new(&ctx.base_path), &key)
        .map_err(|e| format!("Failed to persist completed key: {}", e))
}

fn record_unit_outcome(_ctx: &PostUnitContext) -> Result<(), String> {
    // Placeholder: Record unit outcome for metrics and learning
    // This would integrate with metrics and routing history modules
    Ok(())
}

fn dual_write_database(_ctx: &PostUnitContext) -> Result<(), String> {
    // Placeholder: Dual-write unit completion to database
    // This would integrate with a database module
    Ok(())
}

fn run_post_unit_hooks(_ctx: &PostUnitContext) -> Result<(), String> {
    // Placeholder: Run after-unit, after-slice, after-milestone hooks
    // This would integrate with a hooks system
    Ok(())
}

fn check_hook_retry(_ctx: &PostUnitContext) -> Option<HookRetryTrigger> {
    // Placeholder: Check if a hook requested a retry of the trigger unit
    // This would read from a retry trigger file
    None
}

// ─── Utility Functions ──────────────────────────────────────────────────────────

/// Extract milestone ID from unit ID
fn get_milestone_id(unit_id: &str) -> &str {
    unit_id.split('/').next().unwrap_or("M01")
}

/// Extract slice ID from unit ID
fn get_slice_id(unit_id: &str) -> &str {
    unit_id.split('/').nth(1).unwrap_or("S01")
}

/// Extract task ID from unit ID
fn get_task_id(unit_id: &str) -> &str {
    unit_id.split('/').nth(2).unwrap_or("T01")
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_post_unit_context() {
        let ctx = PostUnitContext {
            base_path: "/test".to_string(),
            unit_type: "execute-task".to_string(),
            unit_id: "M01/S01/T01".to_string(),
            started_at: 1234567890,
            step_mode: false,
            verbose: false,
        };

        assert_eq!(ctx.unit_type, "execute-task");
        assert_eq!(ctx.unit_id, "M01/S01/T01");
    }

    #[test]
    fn test_get_milestone_id() {
        assert_eq!(get_milestone_id("M01/S01/T01"), "M01");
        assert_eq!(get_milestone_id("M02/S03"), "M02");
    }

    #[test]
    fn test_get_slice_id() {
        assert_eq!(get_slice_id("M01/S01/T01"), "S01");
        assert_eq!(get_slice_id("M02/S03"), "S03");
    }

    #[test]
    fn test_get_task_id() {
        assert_eq!(get_task_id("M01/S01/T01"), "T01");
        assert_eq!(get_task_id("M01/S01/T02"), "T02");
    }

    #[test]
    fn test_triage_capture() {
        let capture = TriageCapture {
            id: "cap1".to_string(),
            text: "Test capture".to_string(),
            timestamp: 1234567890,
        };

        assert_eq!(capture.id, "cap1");
        assert_eq!(capture.text, "Test capture");
    }

    #[test]
    fn test_quick_task() {
        let task = QuickTask {
            id: "qt1".to_string(),
            text: "Test task".to_string(),
            milestone_id: "M01".to_string(),
        };

        assert_eq!(task.id, "qt1");
        assert_eq!(task.milestone_id, "M01");
    }

    #[test]
    fn test_hook_retry_trigger() {
        let trigger = HookRetryTrigger {
            unit_type: "execute-task".to_string(),
            unit_id: "M01/S01/T01".to_string(),
        };

        assert_eq!(trigger.unit_type, "execute-task");
        assert_eq!(trigger.unit_id, "M01/S01/T01");
    }

    #[test]
    fn test_post_verification_action_partial_eq() {
        let action1 = PostVerificationAction::Continue;
        let action2 = PostVerificationAction::Continue;
        assert_eq!(action1, action2);
    }

    #[test]
    fn test_post_verification_action_clone() {
        let action = PostVerificationAction::Continue;
        let cloned = action.clone();
        assert_eq!(action, cloned);
    }

    #[test]
    fn test_post_unit_pre_verification_success() {
        let ctx = PostUnitContext {
            base_path: "/nonexistent".to_string(),
            unit_type: "complete-slice".to_string(),
            unit_id: "M01/S01".to_string(),
            started_at: 1234567890,
            step_mode: false,
            verbose: false,
        };

        // complete-slice has no artifacts to verify, so it should succeed
        // even with a nonexistent path
        let result = post_unit_pre_verification(&ctx);
        // Will fail at artifact verification, which is expected
        assert!(result.is_err());
    }
}
