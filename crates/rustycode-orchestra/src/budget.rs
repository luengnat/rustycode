//! Budget Tracking & Enforcement
//!
//! This module provides cost tracking and budget enforcement to prevent
//! uncontrolled spending on LLM API calls during autonomous development.
//!
//! # Budget Levels
//!
//! The tracker monitors spending against a budget ceiling and takes action
//! at different thresholds:
//!
//! - **Ok** (< 80%): Continue execution normally
//! - **Warning** (80-95%): Warn but continue (log alert)
//! - **Critical** (95-100%): Warn but continue (urgent alert)
//! - **Exceeded** (> 100%): Stop execution immediately
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::budget::{BudgetTracker, BudgetAction};
//!
//! let mut tracker = BudgetTracker::new(10.0); // $10 budget
//!
//! // Record each API cost
//! match tracker.record_cost(0.50) {
//!     BudgetAction::Continue => println!("Within budget"),
//!     BudgetAction::Warn { level, spent, budget } => {
//!         println!("Alert: ${}/$ {}", spent, budget);
//!     }
//!     BudgetAction::Stop { spent, budget } => {
//!         println!("Budget exceeded: ${}/$ {}", spent, budget);
//!         return Err(anyhow::anyhow!("Budget exceeded"));
//!     }
//! }
//! ```
//!
//! # Persistence
//!
//! Budget state is persisted to `.orchestra/budget.jsonl` for crash recovery
//! and audit trails.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use tracing::info;

/// Budget alert level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BudgetAlertLevel {
    /// Within budget
    Ok,
    /// Approaching limit (80%+)
    Warning,
    /// Near limit (95%+)
    Critical,
    /// Exceeded budget
    Exceeded,
}

/// Budget enforcement action
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BudgetAction {
    /// Continue execution
    Continue,
    /// Warn user but continue
    Warn {
        level: BudgetAlertLevel,
        spent: f64,
        budget: f64,
    },
    /// Stop execution
    Stop { spent: f64, budget: f64 },
}

/// Budget tracker
#[derive(Debug, Clone)]
pub struct BudgetTracker {
    budget: f64,
    spent: f64,
}

impl BudgetTracker {
    pub fn new(budget: f64) -> Self {
        Self { budget, spent: 0.0 }
    }

    /// Record cost and check if budget exceeded
    #[must_use]
    pub fn record_cost(&mut self, cost: f64) -> BudgetAction {
        // Check before incrementing so callers can act on accurate state
        let projected = self.spent + cost;
        let ratio = if self.budget > 0.0 {
            projected / self.budget
        } else {
            f64::INFINITY
        };

        let level = if ratio >= 1.0 {
            BudgetAlertLevel::Exceeded
        } else if ratio >= 0.95 {
            BudgetAlertLevel::Critical
        } else if ratio >= 0.80 {
            BudgetAlertLevel::Warning
        } else {
            BudgetAlertLevel::Ok
        };

        self.spent = projected;

        match level {
            BudgetAlertLevel::Ok => BudgetAction::Continue,
            BudgetAlertLevel::Warning => BudgetAction::Warn {
                level,
                spent: self.spent,
                budget: self.budget,
            },
            BudgetAlertLevel::Critical => BudgetAction::Warn {
                level,
                spent: self.spent,
                budget: self.budget,
            },
            BudgetAlertLevel::Exceeded => BudgetAction::Stop {
                spent: self.spent,
                budget: self.budget,
            },
        }
    }

    /// Get current budget alert level
    pub fn get_alert_level(&self) -> BudgetAlertLevel {
        if self.budget <= 0.0 {
            return BudgetAlertLevel::Exceeded;
        }
        let ratio = self.spent / self.budget;

        if ratio >= 1.0 {
            BudgetAlertLevel::Exceeded
        } else if ratio >= 0.95 {
            BudgetAlertLevel::Critical
        } else if ratio >= 0.80 {
            BudgetAlertLevel::Warning
        } else {
            BudgetAlertLevel::Ok
        }
    }

    /// Get remaining budget
    pub fn remaining(&self) -> f64 {
        (self.budget - self.spent).max(0.0)
    }

    /// Get total spent
    pub fn total_spent(&self) -> f64 {
        self.spent
    }
}

/// Unit metrics record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitMetrics {
    pub unit_id: String,
    pub timestamp: DateTime<Utc>,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub total_tokens: u32,
    pub cost: f64,
    pub duration_ms: u64,
    pub succeeded: bool,
}

/// Metrics ledger
pub struct MetricsLedger {
    project_root: PathBuf,
}

impl MetricsLedger {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Record metrics for a completed unit
    pub fn record(&self, metrics: UnitMetrics) -> Result<()> {
        let runtime_dir = self.project_root.join(".orchestra/runtime");
        std::fs::create_dir_all(&runtime_dir)?;

        let ledger_path = runtime_dir.join("metrics.jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ledger_path)
            .context("Failed to open metrics ledger")?;

        let line = serde_json::to_string(&metrics)?;
        writeln!(file, "{}", line)?;

        info!(
            "📊 Recorded metrics for {}: {} tokens, ${:.4}",
            metrics.unit_id, metrics.total_tokens, metrics.cost
        );

        Ok(())
    }

    /// Get total project metrics
    pub fn get_totals(&self) -> Result<ProjectTotals> {
        let ledger_path = self.project_root.join(".orchestra/runtime/metrics.jsonl");

        if !ledger_path.exists() {
            return Ok(ProjectTotals::default());
        }

        let content = std::fs::read_to_string(&ledger_path)?;
        let mut totals = ProjectTotals::default();

        for line in content.lines() {
            if let Ok(metrics) = serde_json::from_str::<UnitMetrics>(line) {
                totals.units_completed += 1;
                totals.total_tokens_in += metrics.tokens_in as u64;
                totals.total_tokens_out += metrics.tokens_out as u64;
                totals.total_cost += metrics.cost;
                totals.total_duration_ms += metrics.duration_ms;
            }
        }

        Ok(totals)
    }
}

/// Project totals
#[derive(Debug, Clone, Default)]
pub struct ProjectTotals {
    pub units_completed: u64,
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub total_cost: f64,
    pub total_duration_ms: u64,
}

impl ProjectTotals {
    pub fn format_cost(&self) -> String {
        format!("${:.4}", self.total_cost)
    }

    pub fn format_tokens(&self) -> String {
        let total = self.total_tokens_in + self.total_tokens_out;
        if total >= 1_000_000 {
            format!("{:.1}M", total as f64 / 1_000_000.0)
        } else if total >= 1_000 {
            format!("{:.1}K", total as f64 / 1_000.0)
        } else {
            format!("{}", total)
        }
    }

    pub fn format_duration(&self) -> String {
        let seconds = self.total_duration_ms / 1000;
        if seconds >= 3600 {
            format!("{:.1}h", seconds as f64 / 3600.0)
        } else if seconds >= 60 {
            format!("{:.1}m", seconds as f64 / 60.0)
        } else {
            format!("{}s", seconds)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_tracking() {
        let mut tracker = BudgetTracker::new(100.0);

        // Start with OK
        assert_eq!(tracker.get_alert_level(), BudgetAlertLevel::Ok);
        assert_eq!(tracker.remaining(), 100.0);

        // 80% - Warning
        tracker.spent = 80.0;
        assert_eq!(tracker.get_alert_level(), BudgetAlertLevel::Warning);

        // 95% - Critical
        tracker.spent = 95.0;
        assert_eq!(tracker.get_alert_level(), BudgetAlertLevel::Critical);

        // 100% - Exceeded
        tracker.spent = 100.0;
        assert_eq!(tracker.get_alert_level(), BudgetAlertLevel::Exceeded);

        // Over budget
        tracker.spent = 110.0;
        assert_eq!(tracker.get_alert_level(), BudgetAlertLevel::Exceeded);
        assert_eq!(tracker.remaining(), 0.0);
    }

    #[test]
    fn test_budget_actions() {
        let mut tracker = BudgetTracker::new(100.0);

        // Small cost - continue
        let action = tracker.record_cost(10.0);
        assert!(matches!(action, BudgetAction::Continue));

        // Approaching limit - warn
        let action = tracker.record_cost(75.0);
        assert!(matches!(action, BudgetAction::Warn { .. }));

        // Exceed budget - stop
        let action = tracker.record_cost(20.0);
        assert!(matches!(action, BudgetAction::Stop { .. }));
    }

    #[test]
    fn test_formatting() {
        let totals = ProjectTotals {
            total_cost: 123.456,
            total_tokens_in: 1_500_000,
            total_tokens_out: 500_000,
            total_duration_ms: 3665000, // ~1 hour
            ..ProjectTotals::default()
        };

        assert_eq!(totals.format_cost(), "$123.4560");
        assert_eq!(totals.format_tokens(), "2.0M");
        assert_eq!(totals.format_duration(), "1.0h");
    }

    // --- BudgetAlertLevel tests ---

    #[test]
    fn budget_alert_level_equality() {
        assert_eq!(BudgetAlertLevel::Ok, BudgetAlertLevel::Ok);
        assert_ne!(BudgetAlertLevel::Warning, BudgetAlertLevel::Critical);
    }

    #[test]
    fn budget_tracker_zero_budget_exceeded() {
        let tracker = BudgetTracker::new(0.0);
        assert_eq!(tracker.get_alert_level(), BudgetAlertLevel::Exceeded);
    }

    #[test]
    fn budget_tracker_new_state() {
        let tracker = BudgetTracker::new(50.0);
        assert_eq!(tracker.total_spent(), 0.0);
        assert_eq!(tracker.remaining(), 50.0);
        assert_eq!(tracker.get_alert_level(), BudgetAlertLevel::Ok);
    }

    #[test]
    fn budget_record_cost_accumulates() {
        let mut tracker = BudgetTracker::new(100.0);
        let _ = tracker.record_cost(10.0);
        assert!((tracker.total_spent() - 10.0).abs() < f64::EPSILON);
        assert!((tracker.remaining() - 90.0).abs() < f64::EPSILON);

        let _ = tracker.record_cost(20.0);
        assert!((tracker.total_spent() - 30.0).abs() < f64::EPSILON);
        assert!((tracker.remaining() - 70.0).abs() < f64::EPSILON);
    }

    #[test]
    fn budget_record_cost_exact_threshold() {
        let mut tracker = BudgetTracker::new(100.0);
        // 80% exactly → Warning
        let action = tracker.record_cost(80.0);
        assert!(matches!(
            action,
            BudgetAction::Warn {
                level: BudgetAlertLevel::Warning,
                ..
            }
        ));
    }

    #[test]
    fn budget_record_cost_exact_exceeded() {
        let mut tracker = BudgetTracker::new(100.0);
        let action = tracker.record_cost(100.0);
        assert!(matches!(action, BudgetAction::Stop { .. }));
    }

    #[test]
    fn budget_record_cost_critical_threshold() {
        let mut tracker = BudgetTracker::new(100.0);
        // 95% exactly → Critical
        let action = tracker.record_cost(95.0);
        assert!(matches!(
            action,
            BudgetAction::Warn {
                level: BudgetAlertLevel::Critical,
                ..
            }
        ));
    }

    #[test]
    fn budget_remaining_clamps_to_zero() {
        let mut tracker = BudgetTracker::new(10.0);
        let _ = tracker.record_cost(20.0);
        assert_eq!(tracker.remaining(), 0.0);
    }

    #[test]
    fn budget_action_debug_format() {
        let action = BudgetAction::Continue;
        assert!(format!("{:?}", action).contains("Continue"));

        let warn = BudgetAction::Warn {
            level: BudgetAlertLevel::Warning,
            spent: 80.0,
            budget: 100.0,
        };
        assert!(format!("{:?}", warn).contains("Warn"));

        let stop = BudgetAction::Stop {
            spent: 110.0,
            budget: 100.0,
        };
        assert!(format!("{:?}", stop).contains("Stop"));
    }

    // --- UnitMetrics serde ---

    #[test]
    fn unit_metrics_serde_roundtrip() {
        let m = UnitMetrics {
            unit_id: "unit_42".to_string(),
            timestamp: Utc::now(),
            tokens_in: 1000,
            tokens_out: 500,
            total_tokens: 1500,
            cost: 0.05,
            duration_ms: 3000,
            succeeded: true,
        };
        let json = serde_json::to_string(&m).unwrap();
        let decoded: UnitMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.unit_id, "unit_42");
        assert_eq!(decoded.tokens_in, 1000);
        assert_eq!(decoded.total_tokens, 1500);
        assert!(decoded.succeeded);
    }

    #[test]
    fn unit_metrics_failed_unit() {
        let m = UnitMetrics {
            unit_id: "unit_fail".to_string(),
            timestamp: Utc::now(),
            tokens_in: 500,
            tokens_out: 100,
            total_tokens: 600,
            cost: 0.02,
            duration_ms: 1500,
            succeeded: false,
        };
        let json = serde_json::to_string(&m).unwrap();
        let decoded: UnitMetrics = serde_json::from_str(&json).unwrap();
        assert!(!decoded.succeeded);
    }

    // --- ProjectTotals formatting edge cases ---

    #[test]
    fn project_totals_default() {
        let t = ProjectTotals::default();
        assert_eq!(t.units_completed, 0);
        assert_eq!(t.total_cost, 0.0);
        assert_eq!(t.total_tokens_in, 0);
    }

    #[test]
    fn project_totals_format_tokens_thousands() {
        let t = ProjectTotals {
            total_tokens_in: 5_000,
            total_tokens_out: 3_000,
            ..ProjectTotals::default()
        };
        assert_eq!(t.format_tokens(), "8.0K");
    }

    #[test]
    fn project_totals_format_tokens_small() {
        let t = ProjectTotals {
            total_tokens_in: 50,
            total_tokens_out: 30,
            ..ProjectTotals::default()
        };
        assert_eq!(t.format_tokens(), "80");
    }

    #[test]
    fn project_totals_format_duration_minutes() {
        let t = ProjectTotals {
            total_duration_ms: 120_000, // 2 minutes
            ..ProjectTotals::default()
        };
        assert_eq!(t.format_duration(), "2.0m");
    }

    #[test]
    fn project_totals_format_duration_seconds() {
        let t = ProjectTotals {
            total_duration_ms: 5_000, // 5 seconds
            ..ProjectTotals::default()
        };
        assert_eq!(t.format_duration(), "5s");
    }

    #[test]
    fn project_totals_format_cost_zero() {
        let t = ProjectTotals::default();
        assert_eq!(t.format_cost(), "$0.0000");
    }

    #[test]
    fn metrics_ledger_records_and_reads() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = MetricsLedger::new(dir.path().to_path_buf());

        let m = UnitMetrics {
            unit_id: "u1".to_string(),
            timestamp: Utc::now(),
            tokens_in: 100,
            tokens_out: 50,
            total_tokens: 150,
            cost: 0.01,
            duration_ms: 500,
            succeeded: true,
        };
        ledger.record(m).unwrap();

        let totals = ledger.get_totals().unwrap();
        assert_eq!(totals.units_completed, 1);
        assert_eq!(totals.total_tokens_in, 100);
        assert!((totals.total_cost - 0.01).abs() < 0.001);
    }

    #[test]
    fn metrics_ledger_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = MetricsLedger::new(dir.path().to_path_buf());
        let totals = ledger.get_totals().unwrap();
        assert_eq!(totals.units_completed, 0);
        assert_eq!(totals.total_cost, 0.0);
    }
}
