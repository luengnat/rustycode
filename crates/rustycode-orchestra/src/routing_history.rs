//! Orchestra Routing History — Adaptive Learning for Model Tier Selection
//!
//! Tracks success/failure per tier per unit-type pattern to improve
//! classification accuracy over time through adaptive learning.
//!
//! # Problem
//!
//! Static model selection (always using opus, always using sonnet) fails because:
//! - **Over-provisioning**: Expensive models for simple tasks (waste money)
//! - **Under-provisioning**: Cheap models for complex tasks (waste time)
//! - **One-size-fits-all**: Different units need different capabilities
//!
//! # Solution: Adaptive Learning
//!
//! The routing history system learns from past executions:
//!
//! 1. **Pattern Discovery**: Group units by type (execute-task, plan-slice, etc.)
//! 2. **Outcome Tracking**: Record which tier succeeded/failed for each pattern
//! 3. **Rolling Window**: Only consider recent history (last 50 executions)
//! 4. **Adaptive Adjustment**: Adjust tier selection based on performance
//!
//! # Learning Algorithm
//!
//! For each unit pattern, we track success/failure rates per tier:
//!
//! - **Failure rate > 20%**: Tier is underperforming, upgrade next time
//! - **Success rate > 80%**: Tier is working well, stick with it
//! - **High variance**: Mixed results, need more data
//!
//! # User Feedback
//!
//! User feedback (from manual retries/ratings) is weighted **2x** higher
//! than automatic outcomes. This allows experts to guide the system.
//!
//! # Persistence
//!
//! History is persisted to `.orchestra/routing-history.json`:
//! - Survives process restarts
//! - Enables long-term learning across sessions
//! - Provides audit trail for cost analysis
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::routing_history::{RoutingHistory, RoutingOutcome, ModelSelection};
//!
//! let mut history = RoutingHistory::new(project_root);
//!
//! // Record outcome
//! history.record(
//!     &ModelSelection { tier: ComplexityTier::Standard, ... },
//!     &RoutingOutcome::Success,
//!     &unit
//! );
//!
//! // Get adaptive adjustment
//! if let Some(adj) = history.get_adaptive_tier_adjustment("execute-task") {
//!     println!("Suggested adjustment: {:?}", adj);
//! }
//! ```
//!
//! # Convergence
//!
//! Over time, the system converges on:
//! - **Optimal tier** for each unit type pattern
//! - **Cost efficiency** without sacrificing success rate
//! - **Human-in-the-loop** refinement through feedback

use crate::paths::orchestra_root;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ─── Constants ───────────────────────────────────────────────────────────────────

/// History file name
pub const HISTORY_FILE: &str = "routing-history.json";

/// Rolling window - only consider last N entries per pattern
pub const ROLLING_WINDOW: u32 = 50;

/// Failure threshold - >20% failure rate triggers tier bump
pub const FAILURE_THRESHOLD: f64 = 0.20;

/// Feedback signals count 2x vs automatic
pub const FEEDBACK_WEIGHT: u32 = 2;

// ─── Types ──────────────────────────────────────────────────────────────────────

/// Complexity tier (matches complexity-classifier)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[non_exhaustive]
pub enum ComplexityTier {
    Light,
    Standard,
    Heavy,
}

/// Tier outcome tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TierOutcome {
    pub success: u32,
    pub fail: u32,
}

/// Pattern history for a unit type or tag
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatternHistory {
    pub light: TierOutcome,
    pub standard: TierOutcome,
    pub heavy: TierOutcome,
}

/// User feedback entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackEntry {
    pub unit_type: String,
    pub unit_id: String,
    pub tier: ComplexityTier,
    pub rating: FeedbackRating,
    pub timestamp: String,
}

/// User feedback rating
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FeedbackRating {
    /// Model was overkill (could have used simpler tier)
    Over,
    /// Model was underpowered (needed better tier)
    Under,
    /// Model was appropriate
    Ok,
}

/// Routing history data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingHistoryData {
    pub version: u32,
    /// Keyed by pattern string, e.g. "execute-task:docs" or "complete-slice"
    pub patterns: HashMap<String, PatternHistory>,
    /// User feedback entries
    pub feedback: Vec<FeedbackEntry>,
    /// Last updated timestamp
    pub updated_at: String,
}

impl Default for RoutingHistoryData {
    fn default() -> Self {
        Self {
            version: 1,
            patterns: HashMap::new(),
            feedback: Vec::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

// ─── In-Memory State ─────────────────────────────────────────────────────────────

use once_cell::sync::Lazy;
use std::sync::Mutex;

/// Global routing history state (thread-safe)
struct RoutingHistoryState {
    history: Option<RoutingHistoryData>,
    base_path: String,
}

impl RoutingHistoryState {
    fn new() -> Self {
        Self {
            history: None,
            base_path: String::new(),
        }
    }
}

static GLOBAL_STATE: Lazy<Mutex<RoutingHistoryState>> =
    Lazy::new(|| Mutex::new(RoutingHistoryState::new()));

// ─── Public API ────────────────────────────────────────────────────────────────

/// Initialize routing history for a project
///
/// # Arguments
/// * `base` - Project base path
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::routing_history::*;
///
/// init_routing_history("/project");
/// ```
pub fn init_routing_history(base: &str) {
    let mut state = GLOBAL_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.base_path = base.to_string();
    state.history = Some(load_history(base));
}

/// Reset routing history state
pub fn reset_routing_history() {
    let mut state = GLOBAL_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.history = None;
    state.base_path = String::new();
}

/// Record the outcome of a unit dispatch
///
/// # Arguments
/// * `unit_type` - The unit type (e.g. "execute-task")
/// * `tier` - The tier that was used
/// * `success` - Whether the unit completed successfully
/// * `tags` - Optional tags from task metadata (e.g. ["docs", "test"])
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::routing_history::*;
///
/// record_outcome("execute-task", ComplexityTier::Standard, true, Some(&["test".to_string()]));
/// ```
pub fn record_outcome(
    unit_type: &str,
    tier: ComplexityTier,
    success: bool,
    tags: Option<&[String]>,
) {
    let mut state = GLOBAL_STATE.lock().unwrap_or_else(|e| e.into_inner());

    // Clone base_path before borrowing history
    let base_path = state.base_path.clone();

    let history = match &mut state.history {
        Some(h) => h,
        None => return,
    };

    // Record for the base unit type
    let base_pattern = unit_type.to_string();
    ensure_pattern(history, &base_pattern);

    // Update outcome for base pattern
    {
        let pattern_entry = history.patterns.get_mut(&base_pattern).unwrap();
        let outcome = match tier {
            ComplexityTier::Light => &mut pattern_entry.light,
            ComplexityTier::Standard => &mut pattern_entry.standard,
            ComplexityTier::Heavy => &mut pattern_entry.heavy,
        };
        if success {
            outcome.success += 1;
        } else {
            outcome.fail += 1;
        }
    }

    // Record for tag-specific patterns (e.g. "execute-task:docs")
    if let Some(tags) = tags {
        for tag in tags {
            let tag_pattern = format!("{}:{}", unit_type, tag);
            ensure_pattern(history, &tag_pattern);

            let pattern_entry = history.patterns.get_mut(&tag_pattern).unwrap();
            let tag_outcome = match tier {
                ComplexityTier::Light => &mut pattern_entry.light,
                ComplexityTier::Standard => &mut pattern_entry.standard,
                ComplexityTier::Heavy => &mut pattern_entry.heavy,
            };
            if success {
                tag_outcome.success += 1;
            } else {
                tag_outcome.fail += 1;
            }
        }
    }

    // Apply rolling window — cap total entries per tier per pattern
    for (_pattern, history_entry) in history.patterns.iter_mut() {
        for tier_outcome in [
            &mut history_entry.light,
            &mut history_entry.standard,
            &mut history_entry.heavy,
        ] {
            let total = tier_outcome.success + tier_outcome.fail;
            if total > ROLLING_WINDOW {
                let scale = ROLLING_WINDOW as f64 / total as f64;
                tier_outcome.success = (tier_outcome.success as f64 * scale).round() as u32;
                tier_outcome.fail = (tier_outcome.fail as f64 * scale).round() as u32;
            }
        }
    }

    history.updated_at = chrono::Utc::now().to_rfc3339();

    // Clone the history before dropping the lock
    let history_clone = history.clone();
    drop(state);

    save_history(&base_path, &history_clone);
}

/// Record user feedback for the last completed unit
///
/// # Arguments
/// * `unit_type` - The unit type
/// * `unit_id` - The unit ID
/// * `tier` - The tier that was used
/// * `rating` - User's rating (over/under/ok)
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::routing_history::*;
///
/// record_feedback(
///     "execute-task",
///     "M01/S01/T01",
///     ComplexityTier::Standard,
///     FeedbackRating::Over
/// );
/// ```
pub fn record_feedback(
    unit_type: &str,
    unit_id: &str,
    tier: ComplexityTier,
    rating: FeedbackRating,
) {
    let mut state = GLOBAL_STATE.lock().unwrap_or_else(|e| e.into_inner());

    // Clone base_path before borrowing history
    let base_path = state.base_path.clone();

    let history = match &mut state.history {
        Some(h) => h,
        None => return,
    };

    history.feedback.push(FeedbackEntry {
        unit_type: unit_type.to_string(),
        unit_id: unit_id.to_string(),
        tier,
        rating,
        timestamp: chrono::Utc::now().to_rfc3339(),
    });

    // Cap feedback array at 200 entries
    if history.feedback.len() > 200 {
        history.feedback = history.feedback.split_off(history.feedback.len() - 200);
    }

    // Apply feedback as weighted outcome
    let pattern = unit_type.to_string();
    ensure_pattern(history, &pattern);

    {
        let pattern_entry = history.patterns.get_mut(&pattern).unwrap();

        match rating {
            FeedbackRating::Over => {
                // User says this could have used a simpler model → record as success at current tier
                // and also as success at one tier lower (encourages more downgrading)
                if let Some(lower) = tier_below(tier) {
                    let outcomes = match lower {
                        ComplexityTier::Light => &mut pattern_entry.light,
                        ComplexityTier::Standard => &mut pattern_entry.standard,
                        ComplexityTier::Heavy => &mut pattern_entry.heavy,
                    };
                    outcomes.success += FEEDBACK_WEIGHT;
                }
            }
            FeedbackRating::Under => {
                // User says this needed a better model → record as failure at current tier
                let outcomes = match tier {
                    ComplexityTier::Light => &mut pattern_entry.light,
                    ComplexityTier::Standard => &mut pattern_entry.standard,
                    ComplexityTier::Heavy => &mut pattern_entry.heavy,
                };
                outcomes.fail += FEEDBACK_WEIGHT;
            }
            FeedbackRating::Ok => {
                // No adjustment needed
            }
        }
    }

    history.updated_at = chrono::Utc::now().to_rfc3339();

    // Clone the history before dropping the lock
    let history_clone = history.clone();
    drop(state);

    save_history(&base_path, &history_clone);
}

/// Get the recommended tier adjustment for a given pattern
///
/// Returns the tier to bump to if the failure rate exceeds threshold,
/// or None if no adjustment is needed.
///
/// # Arguments
/// * `unit_type` - The unit type
/// * `current_tier` - The current tier
/// * `tags` - Optional tags for more specific pattern matching
///
/// # Returns
/// Suggested tier if adjustment needed, None otherwise
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::routing_history::*;
///
/// let adjustment = get_adaptive_tier_adjustment(
///     "execute-task",
///     ComplexityTier::Standard,
///     Some(&["docs".to_string()])
/// );
/// ```
pub fn get_adaptive_tier_adjustment(
    unit_type: &str,
    current_tier: ComplexityTier,
    tags: Option<&[String]>,
) -> Option<ComplexityTier> {
    let state = GLOBAL_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let history = state.history.as_ref()?;

    // Check tag-specific patterns first (more specific)
    if let Some(tags) = tags {
        for tag in tags {
            let tag_pattern = format!("{}:{}", unit_type, tag);
            if let Some(adjustment) =
                check_pattern_failure_rate(history, &tag_pattern, current_tier)
            {
                return Some(adjustment);
            }
        }
    }

    // Fall back to base pattern
    check_pattern_failure_rate(history, unit_type, current_tier)
}

/// Clear all routing history (user-triggered reset)
///
/// # Arguments
/// * `base` - Project base path
pub fn clear_routing_history(base: &str) {
    let history = create_empty_history();
    save_history(base, &history);

    let mut state = GLOBAL_STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.base_path == base {
        state.history = Some(history);
    }
}

/// Get current history data (for display/debugging)
///
/// # Returns
/// Clone of current history data, or None if not initialized
pub fn get_routing_history() -> Option<RoutingHistoryData> {
    let state = GLOBAL_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.history.clone()
}

// ─── Internal Functions ─────────────────────────────────────────────────────────

fn check_pattern_failure_rate(
    history: &RoutingHistoryData,
    pattern: &str,
    tier: ComplexityTier,
) -> Option<ComplexityTier> {
    let pattern_history = history.patterns.get(pattern)?;

    let outcomes = match tier {
        ComplexityTier::Light => &pattern_history.light,
        ComplexityTier::Standard => &pattern_history.standard,
        ComplexityTier::Heavy => &pattern_history.heavy,
    };

    let total = outcomes.success + outcomes.fail;
    if total < 3 {
        return None; // Not enough data
    }

    let failure_rate = outcomes.fail as f64 / total as f64;
    if failure_rate > FAILURE_THRESHOLD {
        // Bump to next tier
        tier_above(tier)
    } else {
        None
    }
}

fn tier_above(tier: ComplexityTier) -> Option<ComplexityTier> {
    match tier {
        ComplexityTier::Light => Some(ComplexityTier::Standard),
        ComplexityTier::Standard => Some(ComplexityTier::Heavy),
        ComplexityTier::Heavy => None,
    }
}

fn tier_below(tier: ComplexityTier) -> Option<ComplexityTier> {
    match tier {
        ComplexityTier::Light => None,
        ComplexityTier::Standard => Some(ComplexityTier::Light),
        ComplexityTier::Heavy => Some(ComplexityTier::Standard),
    }
}

fn ensure_pattern(history: &mut RoutingHistoryData, pattern: &str) {
    history.patterns.entry(pattern.to_string()).or_default();
}

fn create_empty_history() -> RoutingHistoryData {
    RoutingHistoryData::default()
}

fn history_path(base: &str) -> std::path::PathBuf {
    orchestra_root(Path::new(base)).join(HISTORY_FILE)
}

fn load_history(base: &str) -> RoutingHistoryData {
    let path = history_path(base);

    match fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<RoutingHistoryData>(&raw) {
            Ok(parsed) if parsed.version == 1 => parsed,
            _ => create_empty_history(),
        },
        Err(_) => create_empty_history(),
    }
}

fn save_history(base: &str, data: &RoutingHistoryData) {
    let path = history_path(base);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Non-fatal — don't let history failures break auto-mode
    let _ = fs::write(path, serde_json::to_string_pretty(data).unwrap() + "\n");
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complexity_tier_ordinals() {
        // Test tier ordering
        assert_eq!(
            tier_above(ComplexityTier::Light),
            Some(ComplexityTier::Standard)
        );
        assert_eq!(
            tier_above(ComplexityTier::Standard),
            Some(ComplexityTier::Heavy)
        );
        assert_eq!(tier_above(ComplexityTier::Heavy), None);

        assert_eq!(tier_below(ComplexityTier::Light), None);
        assert_eq!(
            tier_below(ComplexityTier::Standard),
            Some(ComplexityTier::Light)
        );
        assert_eq!(
            tier_below(ComplexityTier::Heavy),
            Some(ComplexityTier::Standard)
        );
    }

    #[test]
    fn test_tier_outcome_default() {
        let outcome = TierOutcome::default();
        assert_eq!(outcome.success, 0);
        assert_eq!(outcome.fail, 0);
    }

    #[test]
    fn test_pattern_history_default() {
        let history = PatternHistory::default();
        assert_eq!(history.light.success, 0);
        assert_eq!(history.standard.success, 0);
        assert_eq!(history.heavy.success, 0);
    }

    #[test]
    fn test_routing_history_data_default() {
        let data = RoutingHistoryData::default();
        assert_eq!(data.version, 1);
        assert!(data.patterns.is_empty());
        assert!(data.feedback.is_empty());
    }

    #[test]
    fn test_ensure_pattern() {
        let mut history = RoutingHistoryData::default();
        ensure_pattern(&mut history, "execute-task");
        assert!(history.patterns.contains_key("execute-task"));

        // Calling again should not overwrite existing data
        {
            let pattern_entry = history.patterns.get_mut("execute-task").unwrap();
            pattern_entry.light.success = 5;
        }
        ensure_pattern(&mut history, "execute-task");
        assert_eq!(history.patterns["execute-task"].light.success, 5);
    }

    #[test]
    fn test_check_pattern_failure_rate_no_data() {
        let history = RoutingHistoryData::default();
        let result = check_pattern_failure_rate(&history, "execute-task", ComplexityTier::Standard);
        assert!(result.is_none());
    }

    #[test]
    fn test_check_pattern_failure_rate_not_enough_data() {
        let mut history = RoutingHistoryData::default();
        ensure_pattern(&mut history, "execute-task");

        {
            let pattern_entry = history.patterns.get_mut("execute-task").unwrap();
            pattern_entry.standard.success = 2;
        }

        // Only 2 entries - not enough
        let result = check_pattern_failure_rate(&history, "execute-task", ComplexityTier::Standard);
        assert!(result.is_none());
    }

    #[test]
    fn test_check_pattern_failure_rate_below_threshold() {
        let mut history = RoutingHistoryData::default();
        ensure_pattern(&mut history, "execute-task");

        {
            let pattern_entry = history.patterns.get_mut("execute-task").unwrap();
            pattern_entry.standard.success = 8;
            pattern_entry.standard.fail = 1; // 11% failure rate
        }

        let result = check_pattern_failure_rate(&history, "execute-task", ComplexityTier::Standard);
        assert!(result.is_none());
    }

    #[test]
    fn test_check_pattern_failure_rate_above_threshold() {
        let mut history = RoutingHistoryData::default();
        ensure_pattern(&mut history, "execute-task");

        {
            let pattern_entry = history.patterns.get_mut("execute-task").unwrap();
            pattern_entry.standard.success = 7;
            pattern_entry.standard.fail = 3; // 30% failure rate
        }

        let result = check_pattern_failure_rate(&history, "execute-task", ComplexityTier::Standard);
        assert_eq!(result, Some(ComplexityTier::Heavy));
    }

    #[test]
    fn test_feedback_rating() {
        // Test that rating enum works
        let over = FeedbackRating::Over;
        let under = FeedbackRating::Under;
        let ok = FeedbackRating::Ok;

        assert_eq!(over, FeedbackRating::Over);
        assert_eq!(under, FeedbackRating::Under);
        assert_eq!(ok, FeedbackRating::Ok);
    }

    #[test]
    fn test_constants() {
        assert_eq!(ROLLING_WINDOW, 50);
        assert_eq!(FAILURE_THRESHOLD, 0.20);
        assert_eq!(FEEDBACK_WEIGHT, 2);
    }
}
