//! Orchestra Metrics — Token & Cost Tracking (orchestra-2 pattern)
//!
//! Accumulates per-unit usage data across auto-mode sessions for
//! cost analysis, performance optimization, and audit trails.
//!
//! # Metrics Tracked
//!
//! For each executed unit, we track:
//! - **Tokens**: Input, output, cache reads, cache writes
//! - **Cost**: Per-unit and cumulative spend
//! - **Timing**: Unit completion timestamp
//! - **Unit Metadata**: ID, type, success/failure
//!
//! # Persistence
//!
//! Metrics are persisted to `.orchestra/metrics.json` after each unit
//! completion. This enables:
//!
//! - **Crash Recovery**: Metrics survive process restarts
//! - **Cost Tracking**: Real-time budget monitoring
//! - **Post-Mortem**: Analyze what went wrong
//! - **Optimization**: Identify expensive operations
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::metrics::{MetricsManager, MetricsLedger, MetricUnit};
//!
//! let manager = MetricsManager::new(project_root);
//!
//! // Record unit completion
//! manager.record_unit(MetricUnit {
//!     unit_id: "T01".to_string(),
//!     timestamp: Utc::now(),
//!     tokens: TokenCounts { input: 1000, output: 500, ..Default::default() },
//!     cost: 0.015,
//!     success: true,
//! });
//!
//! // Get total spend
//! let ledger = manager.load_ledger().await?;
//! let total_cost: f64 = ledger.units.iter().map(|u| u.cost).sum();
//! println!("Total spend: ${:.2}", total_cost);
//! ```

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::fs;
use tracing::{debug, info};

/// Metrics file path
const METRICS_FILE: &str = ".orchestra/metrics.json";

/// LEDGER version
const LEDGER_VERSION: i32 = 1;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Token counts for a unit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCounts {
    /// Input tokens (prompt)
    pub input: u64,
    /// Output tokens (completion)
    pub output: u64,
    /// Cache read tokens
    pub cache_read: u64,
    /// Cache write tokens
    pub cache_write: u64,
    /// Total tokens
    pub total: u64,
}

impl TokenCounts {
    pub fn new() -> Self {
        Self {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            total: 0,
        }
    }

    /// Add token counts
    pub fn add(&mut self, other: &TokenCounts) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.total += other.total;
    }
}

impl Default for TokenCounts {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics for a single unit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitMetrics {
    /// Unit type (e.g., "research-milestone", "execute-task")
    #[serde(rename = "type")]
    pub unit_type: String,
    /// Unit ID (e.g., "M001/S01/T01")
    pub id: String,
    /// Model ID used
    pub model: String,
    /// When unit started (ms timestamp)
    pub started_at: i64,
    /// When unit finished (ms timestamp)
    pub finished_at: i64,
    /// Token counts
    pub tokens: TokenCounts,
    /// Total cost in USD
    pub cost: f64,
    /// Number of tool calls
    pub tool_calls: u64,
    /// Number of assistant messages
    pub assistant_messages: u64,
    /// Number of user messages
    pub user_messages: u64,
    /// Context window tokens (optional)
    pub context_window_tokens: Option<u64>,
    /// Number of sections truncated (optional)
    pub truncation_sections: Option<u64>,
    /// Whether continue-here fired (optional)
    pub continue_here_fired: Option<bool>,
    /// Prompt character count (optional)
    pub prompt_char_count: Option<u64>,
    /// Baseline character count (optional)
    pub baseline_char_count: Option<u64>,
    /// Complexity tier (optional)
    pub tier: Option<String>,
    /// Whether model was downgraded (optional)
    pub model_downgraded: Option<bool>,
    /// Skills used (optional)
    pub skills: Option<Vec<String>>,
    /// Cache hit rate percentage (optional)
    pub cache_hit_rate: Option<f64>,
    /// Compression savings percentage (optional)
    pub compression_savings: Option<f64>,
}

/// Metrics ledger for the entire project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsLedger {
    /// Ledger version
    pub version: i32,
    /// When project started (ms timestamp)
    pub project_started_at: i64,
    /// All unit metrics
    pub units: Vec<UnitMetrics>,
}

impl MetricsLedger {
    /// Create a new metrics ledger
    pub fn new() -> Self {
        Self {
            version: LEDGER_VERSION,
            project_started_at: Utc::now().timestamp_millis(),
            units: Vec::new(),
        }
    }

    /// Add unit metrics
    pub fn add_unit(&mut self, metrics: UnitMetrics) {
        self.units.push(metrics);
    }

    /// Get total tokens across all units
    #[must_use]
    pub fn total_tokens(&self) -> u64 {
        self.units.iter().map(|u| u.tokens.total).sum()
    }

    /// Get total cost across all units
    #[must_use]
    pub fn total_cost(&self) -> f64 {
        self.units.iter().map(|u| u.cost).sum()
    }

    /// Get total duration across all units
    #[must_use]
    pub fn total_duration_ms(&self) -> i64 {
        self.units
            .iter()
            .map(|u| u.finished_at - u.started_at)
            .sum()
    }

    /// Get metrics for a specific phase
    pub fn phase_metrics(&self, phase: &str) -> Vec<&UnitMetrics> {
        self.units
            .iter()
            .filter(|u| u.unit_type.contains(phase))
            .collect()
    }

    /// Get metrics for a specific unit ID
    pub fn unit_metrics(&self, unit_id: &str) -> Option<&UnitMetrics> {
        self.units.iter().find(|u| u.id == unit_id)
    }
}

impl Default for MetricsLedger {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Phase classification ─────────────────────────────────────────────────────

/// Metrics phase classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MetricsPhase {
    Research,
    Planning,
    Execution,
    Completion,
    Reassessment,
}

/// Classify unit type into metrics phase
///
/// Matches orchestra-2's `classifyUnitPhase()` function.
pub fn classify_unit_phase(unit_type: &str) -> MetricsPhase {
    match unit_type {
        "research-milestone" | "research-slice" => MetricsPhase::Research,
        "plan-milestone" | "plan-slice" => MetricsPhase::Planning,
        "execute-task" => MetricsPhase::Execution,
        "complete-slice" => MetricsPhase::Completion,
        "reassess-roadmap" => MetricsPhase::Reassessment,
        _ => MetricsPhase::Execution,
    }
}

// ─── Metrics Manager ─────────────────────────────────────────────────────────

/// Metrics manager (thread-safe)
#[derive(Clone)]
pub struct MetricsManager {
    project_root: PathBuf,
    ledger: Arc<Mutex<MetricsLedger>>,
}

impl MetricsManager {
    /// Create a new metrics manager
    pub fn new(project_root: PathBuf) -> Self {
        let ledger = Arc::new(Mutex::new(MetricsLedger::new()));

        Self {
            project_root,
            ledger,
        }
    }

    /// Initialize metrics from disk
    ///
    /// Loads existing ledger if present, otherwise creates new.
    pub async fn init(&self) -> Result<()> {
        let metrics_path = self.project_root.join(METRICS_FILE);

        if metrics_path.exists() {
            debug!("Loading metrics from: {:?}", metrics_path);
            let content = fs::read_to_string(&metrics_path).await?;
            let loaded: MetricsLedger = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse metrics from {:?}", metrics_path))?;

            let mut ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
            *ledger = loaded;

            info!("✅ Loaded metrics: {} units", ledger.units.len());
        } else {
            debug!("No existing metrics, starting fresh");
        }

        Ok(())
    }

    /// Reset metrics (clear in-memory state)
    pub fn reset(&self) {
        let mut ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        *ledger = MetricsLedger::new();
        debug!("Metrics reset");
    }

    /// Record unit metrics
    pub fn record_unit(&self, metrics: UnitMetrics) {
        let id = metrics.id.clone();
        let unit_type = metrics.unit_type.clone();
        let mut ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        ledger.add_unit(metrics);
        debug!("Recorded unit metrics: {} ({})", id, unit_type);
    }

    /// Flush metrics to disk
    pub async fn flush(&self) -> Result<()> {
        let metrics_path = self.project_root.join(METRICS_FILE);
        let parent_dir = metrics_path.parent().unwrap();

        // Create parent directory if needed
        fs::create_dir_all(parent_dir).await?;

        // Serialize and write
        let content = {
            let ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
            serde_json::to_string_pretty(&*ledger)
                .with_context(|| "Failed to serialize metrics".to_string())?
        };

        fs::write(&metrics_path, content).await?;

        debug!("Metrics flushed to disk: {:?}", metrics_path);
        Ok(())
    }

    /// Get project totals
    pub fn get_totals(&self) -> MetricsTotals {
        let ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        MetricsTotals {
            total_tokens: ledger.total_tokens(),
            total_cost: ledger.total_cost(),
            total_duration_ms: ledger.total_duration_ms(),
            total_units: ledger.units.len() as u64,
        }
    }

    /// Get phase breakdown
    pub fn get_phase_breakdown(&self) -> PhaseBreakdown {
        let ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());

        let mut research_tokens = 0;
        let mut research_cost = 0.0;
        let mut research_units = 0;

        let mut planning_tokens = 0;
        let mut planning_cost = 0.0;
        let mut planning_units = 0;

        let mut execution_tokens = 0;
        let mut execution_cost = 0.0;
        let mut execution_units = 0;

        let mut completion_tokens = 0;
        let mut completion_cost = 0.0;
        let mut completion_units = 0;

        for unit in &ledger.units {
            let phase = classify_unit_phase(&unit.unit_type);

            match phase {
                MetricsPhase::Research => {
                    research_tokens += unit.tokens.total;
                    research_cost += unit.cost;
                    research_units += 1;
                }
                MetricsPhase::Planning => {
                    planning_tokens += unit.tokens.total;
                    planning_cost += unit.cost;
                    planning_units += 1;
                }
                MetricsPhase::Execution => {
                    execution_tokens += unit.tokens.total;
                    execution_cost += unit.cost;
                    execution_units += 1;
                }
                MetricsPhase::Completion => {
                    completion_tokens += unit.tokens.total;
                    completion_cost += unit.cost;
                    completion_units += 1;
                }
                MetricsPhase::Reassessment => {
                    // Count reassessment in research
                    research_tokens += unit.tokens.total;
                    research_cost += unit.cost;
                    research_units += 1;
                }
            }
        }

        PhaseBreakdown {
            research_tokens,
            research_cost,
            research_units,
            planning_tokens,
            planning_cost,
            planning_units,
            execution_tokens,
            execution_cost,
            execution_units,
            completion_tokens,
            completion_cost,
            completion_units,
        }
    }

    /// Get recent units (last N)
    pub fn get_recent_units(&self, n: usize) -> Vec<UnitMetrics> {
        let ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        let len = ledger.units.len();
        let start = len.saturating_sub(n);
        ledger.units[start..].to_vec()
    }
}

/// Project totals
#[derive(Debug, Clone)]
pub struct MetricsTotals {
    pub total_tokens: u64,
    pub total_cost: f64,
    pub total_duration_ms: i64,
    pub total_units: u64,
}

/// Phase breakdown
#[derive(Debug, Clone)]
pub struct PhaseBreakdown {
    pub research_tokens: u64,
    pub research_cost: f64,
    pub research_units: u64,
    pub planning_tokens: u64,
    pub planning_cost: f64,
    pub planning_units: u64,
    pub execution_tokens: u64,
    pub execution_cost: f64,
    pub execution_units: u64,
    pub completion_tokens: u64,
    pub completion_cost: f64,
    pub completion_units: u64,
}

/// Format token count (orchestra-2 pattern)
///
/// Formats large token counts in human-readable format.
pub fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

/// Format cost in USD (orchestra-2 pattern)
pub fn format_cost(cost: f64) -> String {
    if cost >= 1.0 {
        format!("${:.2}", cost)
    } else if cost >= 0.01 {
        format!("${:.3}", cost)
    } else if cost > 0.0 {
        format!("${:.4}", cost)
    } else {
        "$0.00".to_string()
    }
}

/// Format duration in human-readable format
pub fn format_duration(ms: i64) -> String {
    let seconds = ms / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    if hours > 0 {
        format!("{}h {}m", hours, minutes % 60)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds % 60)
    } else {
        format!("{}s", seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_token_count() {
        assert_eq!(format_token_count(500), "500");
        assert_eq!(format_token_count(1_500), "1.5K");
        assert_eq!(format_token_count(2_500_000), "2.5M");
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.001), "$0.0010");
        assert_eq!(format_cost(0.01), "$0.010");
        assert_eq!(format_cost(0.1), "$0.100");
        assert_eq!(format_cost(1.0), "$1.00");
        assert_eq!(format_cost(10.0), "$10.00");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30_000), "30s");
        assert_eq!(format_duration(90_000), "1m 30s");
        assert_eq!(format_duration(3_600_000), "1h 0m");
    }

    #[test]
    fn test_classify_unit_phase() {
        assert_eq!(
            classify_unit_phase("research-milestone"),
            MetricsPhase::Research
        );
        assert_eq!(classify_unit_phase("plan-slice"), MetricsPhase::Planning);
        assert_eq!(classify_unit_phase("execute-task"), MetricsPhase::Execution);
        assert_eq!(
            classify_unit_phase("complete-slice"),
            MetricsPhase::Completion
        );
        assert_eq!(
            classify_unit_phase("reassess-roadmap"),
            MetricsPhase::Reassessment
        );
    }

    #[test]
    fn test_metrics_ledger() {
        let mut ledger = MetricsLedger::new();

        let unit1 = UnitMetrics {
            unit_type: "execute-task".to_string(),
            id: "M01/S01/T01".to_string(),
            model: "claude-sonnet-4".to_string(),
            started_at: 1000,
            finished_at: 2000,
            tokens: TokenCounts {
                input: 1000,
                output: 500,
                cache_read: 0,
                cache_write: 0,
                total: 1500,
            },
            cost: 0.01,
            tool_calls: 5,
            assistant_messages: 3,
            user_messages: 2,
            context_window_tokens: None,
            truncation_sections: None,
            continue_here_fired: None,
            prompt_char_count: None,
            baseline_char_count: None,
            tier: None,
            model_downgraded: None,
            skills: None,
            cache_hit_rate: None,
            compression_savings: None,
        };

        ledger.add_unit(unit1);

        assert_eq!(ledger.total_tokens(), 1500);
        assert_eq!(ledger.total_cost(), 0.01);
        assert_eq!(ledger.total_duration_ms(), 1000);
    }

    #[test]
    fn test_token_counts_add() {
        let mut counts = TokenCounts::new();
        counts.input = 100;
        counts.output = 50;

        let mut other = TokenCounts::new();
        other.input = 200;
        other.output = 100;

        counts.add(&other);

        assert_eq!(counts.input, 300);
        assert_eq!(counts.output, 150);
    }
}
