//! Orchestra Auto Stuck Detection — Loop detection and recovery for auto-mode
//!
//! Tracks dispatch counts per unit, enforces lifetime caps, and attempts
//! stub/artifact recovery before stopping.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum dispatches per unit before loop detection
pub const MAX_UNIT_DISPATCHES: usize = 10;

/// Threshold for stub recovery attempts
pub const STUB_RECOVERY_THRESHOLD: usize = 5;

/// Maximum lifetime dispatches (hard cap)
pub const MAX_LIFETIME_DISPATCHES: usize = 20;

/// Stuck detection context
#[derive(Debug, Clone)]
pub struct StuckContext<'a> {
    pub session: &'a StuckDetectionSession,
    pub unit_type: String,
    pub unit_id: String,
    pub base_path: String,
}

/// Auto session tracking dispatch counts for stuck detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StuckDetectionSession {
    pub unit_dispatch_count: HashMap<String, usize>,
    pub unit_consecutive_skips: HashMap<String, usize>,
    pub unit_lifetime_dispatches: HashMap<String, usize>,
    pub completed_key_set: Vec<String>,
    pub current_unit: Option<StuckDetectionCurrentUnit>,
}

/// Current unit being executed for stuck detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StuckDetectionCurrentUnit {
    pub unit_type: String,
    pub unit_id: String,
    pub started_at: i64,
}

/// Stuck detection result
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StuckResult {
    /// Proceed with normal dispatch
    Proceed,
    /// Unit recovered, dispatch again
    Recovered { dispatch_again: bool },
    /// Stop with reason
    Stop {
        reason: String,
        notify_message: Option<String>,
    },
}

/// Check dispatch counts, enforce lifetime cap and MAX_UNIT_DISPATCHES,
/// attempt stub/artifact recovery. Returns an action for the caller.
///
/// # Arguments
/// * `sctx` - Stuck detection context
///
/// # Returns
/// Stuck result indicating action to take
///
/// # Example
/// ```
/// use rustycode_orchestra::auto_stuck_detection::*;
///
/// let session = AutoSession {
///     unit_dispatch_count: HashMap::new(),
///     unit_consecutive_skips: HashMap::new(),
///     unit_lifetime_dispatches: HashMap::new(),
///     completed_key_set: Vec::new(),
///     current_unit: None,
/// };
///
/// let sctx = StuckContext {
///     session: &session,
///     unit_type: "execute-task".to_string(),
///     unit_id: "M01/S01/T01".to_string(),
///     base_path: "/project".to_string(),
/// };
///
/// let result = check_stuck_and_recover(sctx);
/// ```
pub fn check_stuck_and_recover(sctx: StuckContext) -> StuckResult {
    let StuckContext {
        session,
        unit_type,
        unit_id,
        base_path,
    } = sctx;
    let dispatch_key = format!("{}/{}", unit_type, unit_id);
    let prev_count = *session.unit_dispatch_count.get(&dispatch_key).unwrap_or(&0);

    // Real dispatch reached — clear the consecutive-skip counter for this unit.
    // Note: In Rust, we can't mutate the session directly here since it's passed by reference
    // The caller is responsible for updating the session state

    // Hard lifetime cap — survives counter resets from loop-recovery/self-repair.
    let lifetime_count = *session
        .unit_lifetime_dispatches
        .get(&dispatch_key)
        .unwrap_or(&0)
        + 1;
    if lifetime_count > MAX_LIFETIME_DISPATCHES {
        let expected = diagnose_expected_artifact(&unit_type, &unit_id, &base_path);
        return StuckResult::Stop {
            reason: format!("Hard loop: {} {}", unit_type, unit_id),
            notify_message: Some(format!(
                "Hard loop detected: {} {} dispatched {} times total (across reconciliation cycles).{}{}\n   This may indicate deriveState() keeps returning the same unit despite artifacts existing.\n   Check .orchestra/completed-units.json and the slice plan checkbox state.",
                unit_type,
                unit_id,
                lifetime_count,
                if let Some(exp) = &expected {
                    format!("\n   Expected artifact: {}", exp)
                } else {
                    String::new()
                },
                if expected.is_some() { "" } else { "\n   No expected artifact path found." }
            )),
        };
    }

    if prev_count >= MAX_UNIT_DISPATCHES {
        // Final reconciliation pass for execute-task
        if unit_type == "execute-task" {
            let parts: Vec<&str> = unit_id.split('/').collect();
            if parts.len() == 3 {
                let mid = parts[0];
                let sid = parts[1];
                let tid = parts[2];

                if let Some(status) = inspect_execute_task_durability(&base_path, &unit_id) {
                    if let Ok(true) = skip_execute_task(
                        &base_path,
                        mid,
                        sid,
                        tid,
                        &status,
                        "loop-recovery",
                        prev_count,
                    ) {
                        if verify_expected_artifact(&unit_type, &unit_id, &base_path) {
                            return StuckResult::Recovered {
                                dispatch_again: true,
                            };
                        }
                    }
                }
            }
        }

        // General reconciliation: artifact appeared on last attempt
        if verify_expected_artifact(&unit_type, &unit_id, &base_path) {
            return StuckResult::Recovered {
                dispatch_again: true,
            };
        }

        // Last resort for complete-milestone: generate stub summary
        if unit_type == "complete-milestone" {
            if let Some(m_path) = resolve_milestone_path(&base_path, &unit_id) {
                let stub_path = m_path.join(format!("{}-SUMMARY.md", unit_id));
                if !stub_path.exists()
                    && std::fs::write(
                        &stub_path,
                        format!(
                            "# {} Summary\n\nAuto-generated stub — milestone tasks completed but summary generation failed after {} attempts.\nReview and replace this stub with a proper summary.\n",
                            unit_id,
                            prev_count + 1
                        )
                    ).is_ok() {
                        return StuckResult::Recovered {
                            dispatch_again: true,
                        };
                    }
            }
        }

        let expected = diagnose_expected_artifact(&unit_type, &unit_id, &base_path);
        let remediation = build_loop_remediation_steps(&unit_type, &unit_id, &base_path);

        return StuckResult::Stop {
            reason: format!("Loop: {} {}", unit_type, unit_id),
            notify_message: Some(format!(
                "Loop detected: {} {} dispatched {} times total. Expected artifact not found.{}{}{}",
                unit_type,
                unit_id,
                prev_count + 1,
                if let Some(exp) = &expected {
                    format!("\n   Expected: {}", exp)
                } else {
                    String::new()
                },
                if let Some(rem) = &remediation {
                    format!("\n\n   Remediation steps:\n{}", rem)
                } else {
                    String::new()
                },
                if expected.is_some() || remediation.is_some() {
                    ""
                } else {
                    "\n   Check branch state and .orchestra/ artifacts."
                }
            )),
        };
    }

    // Adaptive self-repair: each retry attempts a different remediation step.
    if prev_count > 0 && unit_type == "execute-task" {
        let parts: Vec<&str> = unit_id.split('/').collect();
        if parts.len() == 3 {
            let mid = parts[0];
            let sid = parts[1];
            let tid = parts[2];

            if let Some(status) = inspect_execute_task_durability(&base_path, &unit_id) {
                // Self-repair: summary exists but checkbox unmarked
                if status.summary_exists && !status.task_checked {
                    if let Ok(true) =
                        skip_execute_task(&base_path, mid, sid, tid, &status, "self-repair", 0)
                    {
                        if verify_expected_artifact(&unit_type, &unit_id, &base_path) {
                            return StuckResult::Recovered {
                                dispatch_again: true,
                            };
                        }
                    }
                }
                // Stub recovery: generate placeholder summary
                else if prev_count >= STUB_RECOVERY_THRESHOLD && !status.summary_exists {
                    if let Some(tasks_dir) = resolve_tasks_dir(&base_path, mid, sid) {
                        let summary_path = tasks_dir.join(format!("{}-SUMMARY.md", tid));
                        if !summary_path.exists() {
                            let stub_content = format!(
                                "# PARTIAL RECOVERY — attempt {} of {}\n\n\
                                Task `{}` in slice `{}` (milestone `{}`) has not yet produced a real summary.\n\
                                This placeholder was written by auto-mode after {} dispatch attempts.\n\n\
                                The next agent session will retry this task. Replace this file with real work when done.\n",
                                prev_count + 1,
                                MAX_UNIT_DISPATCHES,
                                tid,
                                sid,
                                mid,
                                prev_count
                            );

                            if std::fs::write(&summary_path, stub_content).is_ok() {
                                // Stub written, continue to proceed
                            }
                        }
                    }
                }
            }
        }
    }

    StuckResult::Proceed
}

/// Execute task durability status
#[derive(Debug, Clone)]
pub struct ExecuteTaskStatus {
    pub summary_exists: bool,
    pub task_checked: bool,
    pub summary_path: String,
}

/// Inspect execute-task durability
///
/// # Arguments
/// * `base_path` - Project base path
/// * `unit_id` - Unit ID (e.g., "M01/S01/T01")
///
/// # Returns
/// Status or None if not applicable
fn inspect_execute_task_durability(base_path: &str, unit_id: &str) -> Option<ExecuteTaskStatus> {
    let parts: Vec<&str> = unit_id.split('/').collect();
    if parts.len() != 3 {
        return None;
    }

    let mid = parts[0];
    let sid = parts[1];
    let tid = parts[2];

    let tasks_dir = resolve_tasks_dir(base_path, mid, sid)?;
    let summary_path = tasks_dir.join(format!("{}-SUMMARY.md", tid));

    let summary_exists = summary_path.exists();

    // Check if task is marked done in PLAN.md
    let plan_path = tasks_dir.join(format!("{}-PLAN.md", tid));
    let task_checked = if plan_path.exists() {
        let content = std::fs::read_to_string(&plan_path).ok()?;
        content.contains(&format!("[x] {}", tid))
    } else {
        false
    };

    Some(ExecuteTaskStatus {
        summary_exists,
        task_checked,
        summary_path: summary_path.to_string_lossy().to_string(),
    })
}

/// Skip execute-task by writing placeholder
///
/// # Arguments
/// * `base_path` - Project base path
/// * `mid` - Milestone ID
/// * `sid` - Slice ID
/// * `tid` - Task ID
/// * `status` - Task status
/// * `reason` - Reason for skip
/// * `attempt` - Attempt number
///
/// # Returns
/// Ok(true) if successful, Ok(false) if skipped, Err on failure
fn skip_execute_task(
    base_path: &str,
    mid: &str,
    sid: &str,
    tid: &str,
    _status: &ExecuteTaskStatus,
    _reason: &str,
    _attempt: usize,
) -> Result<bool, String> {
    // Mark task as done in PLAN.md
    let tasks_dir =
        resolve_tasks_dir(base_path, mid, sid).ok_or("Failed to resolve tasks dir".to_string())?;

    let plan_path = tasks_dir.join(format!("{}-PLAN.md", tid));
    if !plan_path.exists() {
        return Ok(false);
    }

    let mut content =
        std::fs::read_to_string(&plan_path).map_err(|e| format!("Failed to read plan: {}", e))?;

    // Replace [ ] with [x] for the task
    let checkbox_pattern = format!("[ ] {}", tid);
    let checked_pattern = format!("[x] {}", tid);

    if content.contains(&checkbox_pattern) {
        content = content.replace(&checkbox_pattern, &checked_pattern);

        std::fs::write(&plan_path, content).map_err(|e| format!("Failed to write plan: {}", e))?;

        Ok(true)
    } else {
        Ok(false)
    }
}

/// Verify expected artifact exists
///
/// # Arguments
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
/// * `base_path` - Project base path
///
/// # Returns
/// true if artifact exists
fn verify_expected_artifact(unit_type: &str, unit_id: &str, base_path: &str) -> bool {
    match unit_type {
        "execute-task" => {
            let parts: Vec<&str> = unit_id.split('/').collect();
            if parts.len() == 3 {
                let tasks_dir = resolve_tasks_dir(base_path, parts[0], parts[1]);
                if let Some(dir) = tasks_dir {
                    let summary_path = dir.join(format!("{}-SUMMARY.md", parts[2]));
                    return summary_path.exists();
                }
            }
            false
        }
        "complete-slice" => {
            let parts: Vec<&str> = unit_id.split('/').collect();
            if parts.len() == 2 {
                if let Some(slice_path) = resolve_slice_path(base_path, parts[0], parts[1]) {
                    let summary_path = slice_path.join(format!("{}-SUMMARY.md", parts[1]));
                    return summary_path.exists();
                }
            }
            false
        }
        "complete-milestone" => {
            if let Some(m_path) = resolve_milestone_path(base_path, unit_id) {
                let summary_path = m_path.join(format!("{}-SUMMARY.md", unit_id));
                return summary_path.exists();
            }
            false
        }
        _ => false,
    }
}

/// Diagnose expected artifact path
///
/// # Arguments
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
/// * `base_path` - Project base path
///
/// # Returns
/// Artifact path or None
fn diagnose_expected_artifact(unit_type: &str, unit_id: &str, base_path: &str) -> Option<String> {
    match unit_type {
        "execute-task" => {
            let parts: Vec<&str> = unit_id.split('/').collect();
            if parts.len() == 3 {
                let tasks_dir = resolve_tasks_dir(base_path, parts[0], parts[1])?;
                let summary_path = tasks_dir.join(format!("{}-SUMMARY.md", parts[2]));
                Some(summary_path.to_string_lossy().to_string())
            } else {
                None
            }
        }
        "complete-slice" => {
            let parts: Vec<&str> = unit_id.split('/').collect();
            if parts.len() == 2 {
                let slice_path = resolve_slice_path(base_path, parts[0], parts[1])?;
                let summary_path = slice_path.join(format!("{}-SUMMARY.md", parts[1]));
                Some(summary_path.to_string_lossy().to_string())
            } else {
                None
            }
        }
        "complete-milestone" => {
            let m_path = resolve_milestone_path(base_path, unit_id)?;
            let summary_path = m_path.join(format!("{}-SUMMARY.md", unit_id));
            Some(summary_path.to_string_lossy().to_string())
        }
        _ => None,
    }
}

/// Build loop remediation steps
///
/// # Arguments
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
/// * `base_path` - Project base path
///
/// # Returns
/// Remediation steps or None
fn build_loop_remediation_steps(unit_type: &str, unit_id: &str, base_path: &str) -> Option<String> {
    let artifact_path = diagnose_expected_artifact(unit_type, unit_id, base_path)?;

    Some(format!(
        "1. Check if the expected artifact exists: {}\n\
         2. If it exists, verify .orchestra/completed-units.json includes the key\n\
         3. If not, manually add the key: \"{}/{}\"\n\
         4. Verify the slice plan checkbox state is correct",
        artifact_path, unit_type, unit_id
    ))
}

/// Resolve tasks directory path
fn resolve_tasks_dir(base_path: &str, mid: &str, sid: &str) -> Option<std::path::PathBuf> {
    use crate::paths::resolve_tasks_dir as resolve_tasks_dir_impl;
    resolve_tasks_dir_impl(std::path::Path::new(base_path), mid, sid)
}

/// Resolve slice path
fn resolve_slice_path(base_path: &str, mid: &str, sid: &str) -> Option<std::path::PathBuf> {
    use crate::paths::resolve_slice_path;
    resolve_slice_path(std::path::Path::new(base_path), mid, sid)
}

/// Resolve milestone path
fn resolve_milestone_path(base_path: &str, milestone_id: &str) -> Option<std::path::PathBuf> {
    use crate::paths::resolve_milestone_path;
    resolve_milestone_path(std::path::Path::new(base_path), milestone_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_session(unit_dispatch_count: usize) -> StuckDetectionSession {
        let mut dispatch_counts = HashMap::new();
        if unit_dispatch_count > 0 {
            dispatch_counts.insert("execute-task/M01/S01/T01".to_string(), unit_dispatch_count);
        }

        StuckDetectionSession {
            unit_dispatch_count: dispatch_counts,
            unit_consecutive_skips: HashMap::new(),
            unit_lifetime_dispatches: HashMap::new(),
            completed_key_set: Vec::new(),
            current_unit: None,
        }
    }

    #[test]
    fn test_stuck_result_proceed() {
        let session = create_test_session(0);
        let sctx = StuckContext {
            session: &session,
            unit_type: "execute-task".to_string(),
            unit_id: "M01/S01/T01".to_string(),
            base_path: "/tmp".to_string(),
        };

        let result = check_stuck_and_recover(sctx);
        assert_eq!(result, StuckResult::Proceed);
    }

    #[test]
    fn test_max_unit_dispatches() {
        let session = create_test_session(MAX_UNIT_DISPATCHES);
        let sctx = StuckContext {
            session: &session,
            unit_type: "execute-task".to_string(),
            unit_id: "M01/S01/T01".to_string(),
            base_path: "/tmp".to_string(),
        };

        let result = check_stuck_and_recover(sctx);
        // Should stop due to max dispatches (no artifact exists)
        assert!(matches!(result, StuckResult::Stop { .. }));
    }

    #[test]
    fn test_lifetime_cap() {
        let mut lifetime_counts = HashMap::new();
        lifetime_counts.insert(
            "execute-task/M01/S01/T01".to_string(),
            MAX_LIFETIME_DISPATCHES + 1,
        );

        let session = StuckDetectionSession {
            unit_dispatch_count: HashMap::new(),
            unit_consecutive_skips: HashMap::new(),
            unit_lifetime_dispatches: lifetime_counts,
            completed_key_set: Vec::new(),
            current_unit: None,
        };

        let sctx = StuckContext {
            session: &session,
            unit_type: "execute-task".to_string(),
            unit_id: "M01/S01/T01".to_string(),
            base_path: "/tmp".to_string(),
        };

        let result = check_stuck_and_recover(sctx);
        assert!(matches!(result, StuckResult::Stop { .. }));
        if let StuckResult::Stop { reason, .. } = result {
            assert!(reason.contains("Hard loop"));
        }
    }

    #[test]
    fn test_non_slice_unit_type() {
        let session = create_test_session(0);
        let sctx = StuckContext {
            session: &session,
            unit_type: "research-milestone".to_string(),
            unit_id: "M01".to_string(),
            base_path: "/tmp".to_string(),
        };

        let result = check_stuck_and_recover(sctx);
        assert_eq!(result, StuckResult::Proceed);
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_UNIT_DISPATCHES, 10);
        assert_eq!(STUB_RECOVERY_THRESHOLD, 5);
        assert_eq!(MAX_LIFETIME_DISPATCHES, 20);
    }
}
