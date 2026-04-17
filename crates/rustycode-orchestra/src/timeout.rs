//! Timeout Supervision
//!
//! Detects and recovers from hung units using three timeout levels:
//! - **Soft timeout**: Warn but continue (5 min default)
//! - **Idle timeout**: No progress? Kill and retry (10 min default)
//! - **Hard timeout**: Absolute limit, kill unit (30 min default)
//!
//! # Problem
//!
//! In autonomous development, LLMs can get stuck in loops or wait forever
//! for tools to complete. Without timeout supervision, units run forever.
//!
//! # Solution
//!
//! The timeout supervisor tracks three time dimensions:
//!
//! 1. **Elapsed Time**: Total time since unit started
//! 2. **Idle Time**: Time since last progress (tool call, LLM response)
//! 3. **Tool Wait Time**: How long oldest in-flight tool has been running
//!
//! Using these three dimensions, we can distinguish:
//! - "Making progress, just taking a while" → Continue
//! - "Waiting for tool to finish" → Continue (don't kill while tool active)
//! - "Truly stuck/idle" → Kill and retry
//!
//! # Timeout Levels
//!
//! ## Soft Timeout (Warning)
//! Alerts user that unit is taking longer than expected but allows
//! continuation. Useful for long-running operations (e.g., large builds).
//!
//! ## Idle Timeout (Retry)
//! If no progress is made for the configured duration, kill the unit
//! and retry. **Exception**: If tools are still running, wait for them.
//!
//! ## Hard Timeout (Kill)
//! Absolute upper limit. Unit is killed immediately regardless of
//! tool state. Prevents runaway units from consuming infinite resources.
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::timeout::{TimeoutSupervisor, TimeoutConfig};
//!
//! let config = TimeoutConfig {
//!     soft_timeout_secs: 300,   // 5 minutes
//!     idle_timeout_secs: 600,   // 10 minutes
//!     hard_timeout_secs: 1800,  // 30 minutes
//! };
//!
//! let supervisor = TimeoutSupervisor::new(project_root, config);
//! let mut state = supervisor.start_unit("T01");
//!
//! // Call periodically in the execution loop
//! match supervisor.check_timeouts(&mut state) {
//!     TimeoutAction::Continue => println!("Still running"),
//!     TimeoutAction::Warn { reason } => println!("Warning: {}", reason),
//!     TimeoutAction::Retry { reason } => {
//!         println!("Retrying: {}", reason);
//!         // Retry the unit
//!     }
//!     TimeoutAction::Kill { reason } => {
//!         println!("Killing unit: {}", reason);
//!         // Terminate execution
//!     }
//! }
//! ```
//!
//! # Progress Tracking
//!
//! Call `state.record_progress()` to update `last_progress_at`:
//! - After each LLM response
//! - After each tool call completes
//! - After each verification check
//!
//! This resets the idle timeout timer.

use crate::tool_tracking::{get_in_flight_tool_count, get_oldest_in_flight_tool_age_ms};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

/// Timeout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Soft timeout: Warn but continue (seconds)
    pub soft_timeout_secs: u64,
    /// Idle timeout: No progress? Kill and retry (seconds)
    pub idle_timeout_secs: u64,
    /// Hard timeout: Absolute limit (seconds)
    pub hard_timeout_secs: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            soft_timeout_secs: 300,  // 5 minutes
            idle_timeout_secs: 600,  // 10 minutes
            hard_timeout_secs: 1800, // 30 minutes
        }
    }
}

/// Unit execution state for timeout tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitTimeoutState {
    pub unit_id: String,
    pub started_at: DateTime<Utc>,
    pub last_progress_at: DateTime<Utc>,
    pub progress_count: usize,
    pub soft_warning_sent: bool,
}

/// Timeout supervisor
pub struct TimeoutSupervisor {
    #[allow(dead_code)] // Kept for future use
    project_root: PathBuf, // Reserved for future timeout state persistence
    config: TimeoutConfig,
}

impl TimeoutSupervisor {
    pub fn new(project_root: PathBuf, config: TimeoutConfig) -> Self {
        Self {
            project_root,
            config,
        }
    }

    /// Start tracking a unit
    pub fn start_unit(&self, unit_id: &str) -> UnitTimeoutState {
        let now = Utc::now();
        UnitTimeoutState {
            unit_id: unit_id.to_string(),
            started_at: now,
            last_progress_at: now,
            progress_count: 0,
            soft_warning_sent: false,
        }
    }

    /// Check if unit has exceeded any timeout
    pub fn check_timeouts(&self, state: &mut UnitTimeoutState) -> TimeoutAction {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(state.started_at).num_seconds() as u64;
        let idle = now
            .signed_duration_since(state.last_progress_at)
            .num_seconds() as u64;
        let in_flight_tool_count = get_in_flight_tool_count();
        let oldest_tool_age_secs = get_oldest_in_flight_tool_age_ms() / 1000;

        // Hard timeout check
        if elapsed > self.config.hard_timeout_secs {
            warn!(
                "⏱️  HARD TIMEOUT: {} exceeded {} seconds",
                state.unit_id, self.config.hard_timeout_secs
            );
            return TimeoutAction::Kill(format!(
                "Unit exceeded hard timeout of {} seconds",
                self.config.hard_timeout_secs
            ));
        }

        // Idle timeout check
        if idle > self.config.idle_timeout_secs {
            if in_flight_tool_count > 0 && oldest_tool_age_secs <= self.config.idle_timeout_secs {
                info!(
                    "⏱️  TOOL WAIT: {} has been idle for {}s but {} tool(s) are still running (oldest {}s)",
                    state.unit_id,
                    idle,
                    in_flight_tool_count,
                    oldest_tool_age_secs
                );
                return TimeoutAction::Continue;
            }

            warn!(
                "⏱️  IDLE TIMEOUT: {} no progress for {} seconds",
                state.unit_id, self.config.idle_timeout_secs
            );
            if in_flight_tool_count > 0 {
                return TimeoutAction::Retry(format!(
                    "Unit appears stuck waiting on {} in-flight tool(s) for {} seconds",
                    in_flight_tool_count, oldest_tool_age_secs
                ));
            }
            return TimeoutAction::Retry(format!(
                "Unit has made no progress for {} seconds",
                self.config.idle_timeout_secs
            ));
        }

        // Soft timeout check
        if elapsed > self.config.soft_timeout_secs && !state.soft_warning_sent {
            info!(
                "⏱️  SOFT TIMEOUT: {} exceeded {} seconds (continuing)",
                state.unit_id, self.config.soft_timeout_secs
            );
            state.soft_warning_sent = true;
            return TimeoutAction::Warn(format!("Unit has been running for {} seconds", elapsed));
        }

        TimeoutAction::Continue
    }

    /// Record progress (resets idle timer)
    pub fn record_progress(&self, state: &mut UnitTimeoutState) {
        state.last_progress_at = Utc::now();
        state.progress_count += 1;
    }
}

/// Action to take on timeout
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TimeoutAction {
    /// Continue executing
    Continue,
    /// Warn user but continue
    Warn(String),
    /// Kill and retry unit
    Retry(String),
    /// Kill and fail unit
    Kill(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    #[test]
    fn test_soft_timeout() {
        let supervisor = TimeoutSupervisor::new(
            PathBuf::from("/tmp"),
            TimeoutConfig {
                soft_timeout_secs: 60,
                idle_timeout_secs: 600,
                hard_timeout_secs: 1800,
            },
        );

        let mut state = supervisor.start_unit("T01");
        state.started_at = Utc::now() - ChronoDuration::seconds(70);
        state.last_progress_at = Utc::now() - ChronoDuration::seconds(10);

        let action = supervisor.check_timeouts(&mut state);
        assert!(matches!(action, TimeoutAction::Warn(_)));
        assert!(state.soft_warning_sent);
    }

    #[test]
    fn test_idle_timeout() {
        let supervisor = TimeoutSupervisor::new(PathBuf::from("/tmp"), TimeoutConfig::default());

        let mut state = supervisor.start_unit("T01");
        state.started_at = Utc::now() - ChronoDuration::seconds(700);
        state.last_progress_at = Utc::now() - ChronoDuration::seconds(650);

        let action = supervisor.check_timeouts(&mut state);
        assert!(matches!(action, TimeoutAction::Retry(_)));
    }

    #[test]
    fn test_hard_timeout() {
        let supervisor = TimeoutSupervisor::new(PathBuf::from("/tmp"), TimeoutConfig::default());

        let mut state = supervisor.start_unit("T01");
        state.started_at = Utc::now() - ChronoDuration::seconds(2000);
        state.last_progress_at = Utc::now() - ChronoDuration::seconds(100);

        let action = supervisor.check_timeouts(&mut state);
        assert!(matches!(action, TimeoutAction::Kill(_)));
    }

    #[test]
    fn test_record_progress() {
        let supervisor = TimeoutSupervisor::new(PathBuf::from("/tmp"), TimeoutConfig::default());
        let mut state = supervisor.start_unit("T01");

        let initial_progress = state.progress_count;
        supervisor.record_progress(&mut state);

        assert_eq!(state.progress_count, initial_progress + 1);
    }

    // --- TimeoutConfig ---

    #[test]
    fn config_default_values() {
        let cfg = TimeoutConfig::default();
        assert_eq!(cfg.soft_timeout_secs, 300);
        assert_eq!(cfg.idle_timeout_secs, 600);
        assert_eq!(cfg.hard_timeout_secs, 1800);
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = TimeoutConfig {
            soft_timeout_secs: 100,
            idle_timeout_secs: 200,
            hard_timeout_secs: 300,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let decoded: TimeoutConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.soft_timeout_secs, 100);
        assert_eq!(decoded.idle_timeout_secs, 200);
        assert_eq!(decoded.hard_timeout_secs, 300);
    }

    // --- UnitTimeoutState ---

    #[test]
    fn state_serde_roundtrip() {
        let state = UnitTimeoutState {
            unit_id: "T42".into(),
            started_at: Utc::now(),
            last_progress_at: Utc::now(),
            progress_count: 5,
            soft_warning_sent: true,
        };
        let json = serde_json::to_string(&state).unwrap();
        let decoded: UnitTimeoutState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.unit_id, "T42");
        assert_eq!(decoded.progress_count, 5);
        assert!(decoded.soft_warning_sent);
    }

    #[test]
    fn start_unit_initial_state() {
        let supervisor = TimeoutSupervisor::new(PathBuf::from("/tmp"), TimeoutConfig::default());
        let state = supervisor.start_unit("T99");
        assert_eq!(state.unit_id, "T99");
        assert!(!state.soft_warning_sent);
        assert_eq!(state.progress_count, 0);
    }

    // --- TimeoutAction ---

    #[test]
    fn action_continue_no_warning_resend() {
        let supervisor = TimeoutSupervisor::new(
            PathBuf::from("/tmp"),
            TimeoutConfig {
                soft_timeout_secs: 60,
                idle_timeout_secs: 600,
                hard_timeout_secs: 1800,
            },
        );
        let mut state = supervisor.start_unit("T01");
        state.started_at = Utc::now() - ChronoDuration::seconds(70);
        state.last_progress_at = Utc::now() - ChronoDuration::seconds(10);
        state.soft_warning_sent = true;

        // Already warned, should return Continue
        let action = supervisor.check_timeouts(&mut state);
        assert!(matches!(action, TimeoutAction::Continue));
    }

    #[test]
    fn no_timeout_when_within_limits() {
        let supervisor = TimeoutSupervisor::new(
            PathBuf::from("/tmp"),
            TimeoutConfig {
                soft_timeout_secs: 300,
                idle_timeout_secs: 600,
                hard_timeout_secs: 1800,
            },
        );
        let mut state = supervisor.start_unit("T01");
        // Freshly created, well within limits
        let action = supervisor.check_timeouts(&mut state);
        assert!(matches!(action, TimeoutAction::Continue));
    }
}
