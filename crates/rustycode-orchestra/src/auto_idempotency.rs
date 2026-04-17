//! Orchestra Auto Idempotency — Idempotency Checks for Auto-Mode Unit Dispatch
//!
//! Handles completed-key membership, artifact cross-validation,
//! consecutive skip counting, phantom skip loop detection, key eviction,
//! and fallback persistence.
//!
//! Critical for preventing redundant work and infinite loops in autonomous development.

use std::collections::{HashMap, HashSet};
use std::path::Path;

// ─── Constants ───────────────────────────────────────────────────────────────────

/// Maximum consecutive skips before detecting a loop
pub const MAX_CONSECUTIVE_SKIPS: usize = 5;

/// Maximum lifetime dispatches before hard-stop
pub const MAX_LIFETIME_DISPATCHES: usize = 20;

// ─── Types ──────────────────────────────────────────────────────────────────────

/// Idempotency check context
#[derive(Debug)]
pub struct IdempotencyContext<'a> {
    /// Idempotency state (mutable)
    pub state: &'a mut IdempotencyState,
    /// Unit type (e.g., "plan-slice", "execute-task")
    pub unit_type: String,
    /// Unit ID (e.g., "M01/S01/T01")
    pub unit_id: String,
    /// Base path for the project
    pub base_path: String,
}

/// Idempotency state for tracking completed units and skip loops
#[derive(Debug, Clone)]
pub struct IdempotencyState {
    /// Set of completed idempotency keys
    pub completed_key_set: HashSet<String>,

    /// Consecutive skip counter per unit
    pub unit_consecutive_skips: HashMap<String, usize>,

    /// Lifetime dispatch counter per unit
    pub unit_lifetime_dispatches: HashMap<String, usize>,

    /// Recently evicted keys (to prevent fallback re-adding)
    pub recently_evicted_keys: HashSet<String>,
}

impl Default for IdempotencyState {
    fn default() -> Self {
        Self::new()
    }
}

impl IdempotencyState {
    pub fn new() -> Self {
        Self {
            completed_key_set: HashSet::new(),
            unit_consecutive_skips: HashMap::new(),
            unit_lifetime_dispatches: HashMap::new(),
            recently_evicted_keys: HashSet::new(),
        }
    }
}

/// Idempotency check result
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdempotencyResult {
    /// Skip the unit - already completed
    Skip { reason: SkipReason },

    /// Re-run the unit - completion record was stale
    Rerun { reason: String },

    /// Proceed with normal dispatch
    Proceed,

    /// Stop auto-mode - hard loop detected
    Stop { reason: String },
}

/// Reason for skipping a unit
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipReason {
    /// Unit completed in prior session
    Completed,

    /// Completion record evicted due to skip loop
    Evicted,

    /// Fallback - artifact existed but key was missing
    FallbackPersisted,

    /// Phantom skip loop cleared (milestone completed)
    PhantomLoopCleared,
}

// ─── Public API ────────────────────────────────────────────────────────────────

/// Check whether a unit should be skipped (already completed), rerun
/// (stale completion record), or dispatched normally.
///
/// # Arguments
/// * `ictx` - Idempotency context
///
/// # Returns
/// Idempotency result indicating action to take
///
/// # Side Effects
/// Mutates state:
/// - completed_key_set
/// - unit_consecutive_skips
/// - unit_lifetime_dispatches
/// - recently_evicted_keys
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_idempotency::*;
///
/// let mut state = IdempotencyState::new();
/// let ctx = IdempotencyContext {
///     state: &mut state,
///     unit_type: "execute-task".to_string(),
///     unit_id: "M01/S01/T01".to_string(),
///     base_path: "/project".to_string(),
/// };
///
/// let result = check_idempotency(ctx);
/// ```
pub fn check_idempotency(ictx: IdempotencyContext) -> IdempotencyResult {
    let IdempotencyContext {
        state,
        unit_type,
        unit_id,
        base_path,
    } = ictx;

    let idempotency_key = format!("{}/{}", unit_type, unit_id);

    // ── Primary path: key exists in completed set ──
    if state.completed_key_set.contains(&idempotency_key) {
        // Check if expected artifact still exists
        let artifact_exists = verify_expected_artifact(&unit_type, &unit_id, &base_path);

        if artifact_exists {
            // Guard against infinite skip loops
            let skip_count = state
                .unit_consecutive_skips
                .get(&idempotency_key)
                .map(|v| v + 1)
                .unwrap_or(1);
            state
                .unit_consecutive_skips
                .insert(idempotency_key.clone(), skip_count);

            if skip_count > MAX_CONSECUTIVE_SKIPS {
                // Cross-check: verify the unit's milestone is still active
                let skipped_mid = unit_id.split('/').next().unwrap_or("");
                let skipped_milestone_complete = if !skipped_mid.is_empty() {
                    milestone_has_summary(&base_path, skipped_mid)
                } else {
                    false
                };

                if skipped_milestone_complete {
                    state.unit_consecutive_skips.remove(&idempotency_key);
                    return IdempotencyResult::Skip {
                        reason: SkipReason::PhantomLoopCleared,
                    };
                }

                // Evict completion record
                state.unit_consecutive_skips.remove(&idempotency_key);
                state.completed_key_set.remove(&idempotency_key);
                state.recently_evicted_keys.insert(idempotency_key.clone());

                return IdempotencyResult::Skip {
                    reason: SkipReason::Evicted,
                };
            }

            // Count toward lifetime cap
            let life_skip = state
                .unit_lifetime_dispatches
                .get(&idempotency_key)
                .map(|v| v + 1)
                .unwrap_or(1);
            state
                .unit_lifetime_dispatches
                .insert(idempotency_key.clone(), life_skip);

            if life_skip > MAX_LIFETIME_DISPATCHES {
                return IdempotencyResult::Stop {
                    reason: format!("Hard loop: {} {} (skip cycle)", unit_type, unit_id),
                };
            }

            return IdempotencyResult::Skip {
                reason: SkipReason::Completed,
            };
        } else {
            // Stale completion record — artifact missing. Remove and re-run.
            state.completed_key_set.remove(&idempotency_key);
            return IdempotencyResult::Rerun {
                reason: "marked complete but expected artifact missing".to_string(),
            };
        }
    }

    // ── Fallback: key missing but artifact exists ──
    if verify_expected_artifact(&unit_type, &unit_id, &base_path)
        && !state.recently_evicted_keys.contains(&idempotency_key)
    {
        // Persist the completed key
        state.completed_key_set.insert(idempotency_key.clone());

        // Same consecutive-skip guard as the primary path
        let skip_count = state
            .unit_consecutive_skips
            .get(&idempotency_key)
            .map(|v| v + 1)
            .unwrap_or(1);
        state
            .unit_consecutive_skips
            .insert(idempotency_key.clone(), skip_count);

        if skip_count > MAX_CONSECUTIVE_SKIPS {
            let skipped_mid = unit_id.split('/').next().unwrap_or("");
            let skipped_milestone_complete = if !skipped_mid.is_empty() {
                milestone_has_summary(&base_path, skipped_mid)
            } else {
                false
            };

            if skipped_milestone_complete {
                state.unit_consecutive_skips.remove(&idempotency_key);
                return IdempotencyResult::Skip {
                    reason: SkipReason::PhantomLoopCleared,
                };
            }

            state.unit_consecutive_skips.remove(&idempotency_key);
            state.completed_key_set.remove(&idempotency_key);
            state.recently_evicted_keys.insert(idempotency_key.clone());

            return IdempotencyResult::Skip {
                reason: SkipReason::Evicted,
            };
        }

        // Count toward lifetime cap
        let life_skip = state
            .unit_lifetime_dispatches
            .get(&idempotency_key)
            .map(|v| v + 1)
            .unwrap_or(1);
        state
            .unit_lifetime_dispatches
            .insert(idempotency_key.clone(), life_skip);

        if life_skip > MAX_LIFETIME_DISPATCHES {
            return IdempotencyResult::Stop {
                reason: format!("Hard loop: {} {} (skip cycle)", unit_type, unit_id),
            };
        }

        return IdempotencyResult::Skip {
            reason: SkipReason::FallbackPersisted,
        };
    }

    IdempotencyResult::Proceed
}

/// Build idempotency key from unit type and unit ID
///
/// # Arguments
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
///
/// # Returns
/// Formatted idempotency key
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_idempotency::*;
///
/// let key = build_idempotency_key("execute-task", "M01/S01/T01");
/// assert_eq!(key, "execute-task/M01/S01/T01");
/// ```
pub fn build_idempotency_key(unit_type: &str, unit_id: &str) -> String {
    format!("{}/{}", unit_type, unit_id)
}

/// Check if a skip reason indicates the unit was already completed
///
/// # Arguments
/// * `reason` - Skip reason
///
/// # Returns
/// True if the unit was completed in a prior session
pub fn is_completed_skip(reason: &SkipReason) -> bool {
    matches!(
        reason,
        SkipReason::Completed | SkipReason::FallbackPersisted
    )
}

/// Check if a skip reason indicates a loop was detected
///
/// # Arguments
/// * `reason` - Skip reason
///
/// # Returns
/// True if a skip loop was detected and handled
pub fn is_loop_skip(reason: &SkipReason) -> bool {
    matches!(reason, SkipReason::Evicted | SkipReason::PhantomLoopCleared)
}

/// Add a completed key to the state
///
/// # Arguments
/// * `state` - Idempotency state
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
pub fn add_completed_key(state: &mut IdempotencyState, unit_type: &str, unit_id: &str) {
    let key = build_idempotency_key(unit_type, unit_id);
    state.completed_key_set.insert(key);
}

/// Remove a completed key from the state
///
/// # Arguments
/// * `state` - Idempotency state
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
pub fn remove_completed_key(state: &mut IdempotencyState, unit_type: &str, unit_id: &str) {
    let key = build_idempotency_key(unit_type, unit_id);
    state.completed_key_set.remove(&key);
    state.unit_consecutive_skips.remove(&key);
    state.unit_lifetime_dispatches.remove(&key);
}

/// Mark a key as recently evicted
///
/// # Arguments
/// * `state` - Idempotency state
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
pub fn mark_recently_evicted(state: &mut IdempotencyState, unit_type: &str, unit_id: &str) {
    let key = build_idempotency_key(unit_type, unit_id);
    state.recently_evicted_keys.insert(key);
}

/// Clear recently evicted keys (e.g., after session restart)
///
/// # Arguments
/// * `state` - Idempotency state
pub fn clear_recently_evicted(state: &mut IdempotencyState) {
    state.recently_evicted_keys.clear();
}

/// Get skip count for a unit
///
/// # Arguments
/// * `state` - Idempotency state
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
///
/// # Returns
/// Current consecutive skip count
pub fn get_skip_count(state: &IdempotencyState, unit_type: &str, unit_id: &str) -> usize {
    let key = build_idempotency_key(unit_type, unit_id);
    state.unit_consecutive_skips.get(&key).copied().unwrap_or(0)
}

/// Get lifetime dispatch count for a unit
///
/// # Arguments
/// * `state` - Idempotency state
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
///
/// # Returns
/// Current lifetime dispatch count
pub fn get_lifetime_dispatch_count(
    state: &IdempotencyState,
    unit_type: &str,
    unit_id: &str,
) -> usize {
    let key = build_idempotency_key(unit_type, unit_id);
    state
        .unit_lifetime_dispatches
        .get(&key)
        .copied()
        .unwrap_or(0)
}

// ─── Helper Functions ─────────────────────────────────────────────────────────

/// Verify that the expected artifact exists for a completed unit
///
/// # Arguments
/// * `unit_type` - Unit type
/// * `unit_id` - Unit ID
/// * `base_path` - Base project path
///
/// # Returns
/// True if the expected artifact exists
fn verify_expected_artifact(unit_type: &str, unit_id: &str, base_path: &str) -> bool {
    let parts: Vec<&str> = unit_id.split('/').collect();
    if parts.len() < 2 {
        return false;
    }

    let milestone_id = parts[0];
    let slice_id = parts.get(1).unwrap_or(&"");

    match unit_type {
        "plan-slice" => {
            // Check for PLAN.md
            let plan_path = Path::new(base_path)
                .join(".orchestra")
                .join("milestones")
                .join(milestone_id)
                .join(format!("{}-PLAN.md", slice_id));
            plan_path.exists()
        }
        "execute-task" => {
            // Check for SUMMARY.md (task completion)
            if parts.len() >= 3 {
                let task_id = parts[2];
                let summary_path = Path::new(base_path)
                    .join(".orchestra")
                    .join("milestones")
                    .join(milestone_id)
                    .join(slice_id)
                    .join(format!("{}-SUMMARY.md", task_id));
                summary_path.exists()
            } else {
                false
            }
        }
        "complete-slice" => {
            // Check for SLICE-SUMMARY.md
            let summary_path = Path::new(base_path)
                .join(".orchestra")
                .join("milestones")
                .join(milestone_id)
                .join(format!("{}-SUMMARY.md", slice_id));
            summary_path.exists()
        }
        _ => false,
    }
}

/// Check if a milestone has a SUMMARY file (indicating completion)
///
/// # Arguments
/// * `base_path` - Base project path
/// * `milestone_id` - Milestone ID
///
/// # Returns
/// True if milestone SUMMARY.md exists
fn milestone_has_summary(base_path: &str, milestone_id: &str) -> bool {
    let summary_path = Path::new(base_path)
        .join(".orchestra")
        .join("milestones")
        .join(milestone_id)
        .join("SUMMARY.md");
    summary_path.exists()
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_idempotency_key() {
        let key = build_idempotency_key("execute-task", "M01/S01/T01");
        assert_eq!(key, "execute-task/M01/S01/T01");
    }

    #[test]
    fn test_is_completed_skip() {
        assert!(is_completed_skip(&SkipReason::Completed));
        assert!(is_completed_skip(&SkipReason::FallbackPersisted));
        assert!(!is_completed_skip(&SkipReason::Evicted));
        assert!(!is_completed_skip(&SkipReason::PhantomLoopCleared));
    }

    #[test]
    fn test_is_loop_skip() {
        assert!(is_loop_skip(&SkipReason::Evicted));
        assert!(is_loop_skip(&SkipReason::PhantomLoopCleared));
        assert!(!is_loop_skip(&SkipReason::Completed));
        assert!(!is_loop_skip(&SkipReason::FallbackPersisted));
    }

    #[test]
    fn test_idempotency_state_default() {
        let state = IdempotencyState::default();
        assert!(state.completed_key_set.is_empty());
        assert!(state.unit_consecutive_skips.is_empty());
        assert!(state.unit_lifetime_dispatches.is_empty());
        assert!(state.recently_evicted_keys.is_empty());
    }

    #[test]
    fn test_idempotency_state_new() {
        let state = IdempotencyState::new();
        assert!(state.completed_key_set.is_empty());
        assert!(state.unit_consecutive_skips.is_empty());
    }

    #[test]
    fn test_add_completed_key() {
        let mut state = IdempotencyState::new();
        add_completed_key(&mut state, "execute-task", "M01/S01/T01");
        assert!(state.completed_key_set.contains("execute-task/M01/S01/T01"));
    }

    #[test]
    fn test_remove_completed_key() {
        let mut state = IdempotencyState::new();
        add_completed_key(&mut state, "execute-task", "M01/S01/T01");
        remove_completed_key(&mut state, "execute-task", "M01/S01/T01");
        assert!(!state.completed_key_set.contains("execute-task/M01/S01/T01"));
        assert!(!state
            .unit_consecutive_skips
            .contains_key("execute-task/M01/S01/T01"));
        assert!(!state
            .unit_lifetime_dispatches
            .contains_key("execute-task/M01/S01/T01"));
    }

    #[test]
    fn test_mark_recently_evicted() {
        let mut state = IdempotencyState::new();
        mark_recently_evicted(&mut state, "execute-task", "M01/S01/T01");
        assert!(state
            .recently_evicted_keys
            .contains("execute-task/M01/S01/T01"));
    }

    #[test]
    fn test_clear_recently_evicted() {
        let mut state = IdempotencyState::new();
        mark_recently_evicted(&mut state, "execute-task", "M01/S01/T01");
        clear_recently_evicted(&mut state);
        assert!(state.recently_evicted_keys.is_empty());
    }

    #[test]
    fn test_get_skip_count() {
        let mut state = IdempotencyState::new();
        assert_eq!(get_skip_count(&state, "execute-task", "M01/S01/T01"), 0);

        state
            .unit_consecutive_skips
            .insert("execute-task/M01/S01/T01".to_string(), 3);
        assert_eq!(get_skip_count(&state, "execute-task", "M01/S01/T01"), 3);
    }

    #[test]
    fn test_get_lifetime_dispatch_count() {
        let mut state = IdempotencyState::new();
        assert_eq!(
            get_lifetime_dispatch_count(&state, "execute-task", "M01/S01/T01"),
            0
        );

        state
            .unit_lifetime_dispatches
            .insert("execute-task/M01/S01/T01".to_string(), 5);
        assert_eq!(
            get_lifetime_dispatch_count(&state, "execute-task", "M01/S01/T01"),
            5
        );
    }

    #[test]
    fn test_check_idempotency_proceed() {
        let mut state = IdempotencyState::new();
        let ctx = IdempotencyContext {
            state: &mut state,
            unit_type: "execute-task".to_string(),
            unit_id: "M01/S01/T01".to_string(),
            base_path: "/tmp/nonexistent".to_string(),
        };

        let result = check_idempotency(ctx);
        assert_eq!(result, IdempotencyResult::Proceed);
    }

    #[test]
    fn test_check_idempotency_skip_completed() {
        let mut state = IdempotencyState::new();
        // Simulate completed unit with artifact
        add_completed_key(&mut state, "execute-task", "M01/S01/T01");

        let ctx = IdempotencyContext {
            state: &mut state,
            unit_type: "execute-task".to_string(),
            unit_id: "M01/S01/T01".to_string(),
            base_path: "/tmp/nonexistent".to_string(),
        };

        let result = check_idempotency(ctx);
        // Will proceed because artifact doesn't exist (rerun)
        assert!(matches!(result, IdempotencyResult::Rerun { .. }));
    }

    #[test]
    fn test_max_consecutive_skips_constant() {
        assert_eq!(MAX_CONSECUTIVE_SKIPS, 5);
    }

    #[test]
    fn test_max_lifetime_dispatches_constant() {
        assert_eq!(MAX_LIFETIME_DISPATCHES, 20);
    }
}
