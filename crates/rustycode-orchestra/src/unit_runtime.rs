//! Orchestra Unit Runtime — Unit Lifecycle Tracking
//!
//! Tracks the runtime state of each autonomous unit:
//! * Dispatch, execution, and completion phases
//! * Progress tracking with timestamps and counts
//! * Timeout and recovery monitoring
//! * Durability inspection for task artifacts
//!
//! Critical for production autonomous systems to enable crash recovery,
//! supervision, and state reconciliation.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Runtime phase of a unit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum UnitRuntimePhase {
    /// Unit has been dispatched but not yet started
    Dispatched,
    /// Wrapup warning has been sent (soft timeout approaching)
    WrapupWarningSent,
    /// Hard timeout fired
    Timeout,
    /// Unit recovered from timeout/idle
    Recovered,
    /// Unit finalized and completed
    Finalized,
    /// Unit paused by user
    Paused,
    /// Unit skipped (blocker written)
    Skipped,
}

/// Recovery status for execute-task units
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteTaskRecoveryStatus {
    /// Relative path to slice plan
    pub plan_path: String,

    /// Relative path to task summary
    pub summary_path: String,

    /// Whether summary file exists
    pub summary_exists: bool,

    /// Whether task checkbox is marked [x] in plan
    pub task_checked: bool,

    /// Whether state next action has advanced past this task
    pub next_action_advanced: bool,

    /// Number of must-haves in task plan
    pub must_have_count: usize,

    /// Number of must-haves mentioned in summary
    pub must_haves_mentioned_in_summary: usize,
}

/// Runtime record for an autonomous unit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitRuntimeRecord {
    /// Record version
    pub version: u32,

    /// Unit type (e.g., "execute-task", "plan-slice")
    pub unit_type: String,

    /// Unit ID (e.g., "M01/S01/T01")
    pub unit_id: String,

    /// When the unit started (Unix timestamp ms)
    pub started_at: u64,

    /// When the record was last updated (Unix timestamp ms)
    pub updated_at: u64,

    /// Current phase
    pub phase: UnitRuntimePhase,

    /// Whether wrapup warning has been sent
    pub wrapup_warning_sent: bool,

    /// Whether continue-here (context pressure) fired
    pub continue_here_fired: bool,

    /// Hard timeout timestamp (null if not set)
    pub timeout_at: Option<u64>,

    /// Last time progress was made (Unix timestamp ms)
    pub last_progress_at: u64,

    /// Number of progress events
    pub progress_count: u32,

    /// Last progress kind (e.g., "dispatch", "tool_complete", "manual")
    pub last_progress_kind: String,

    /// Recovery status for execute-task units
    pub recovery: Option<ExecuteTaskRecoveryStatus>,

    /// Number of recovery attempts
    pub recovery_attempts: u32,

    /// Last recovery reason
    pub last_recovery_reason: Option<RecoveryReason>,
}

/// Reason for recovery attempt
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum RecoveryReason {
    /// Idle timeout (no progress)
    Idle,
    /// Hard timeout
    Hard,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Write or update a unit runtime record
///
/// # Arguments
/// * `base` - Project base path
/// * `unit_type` - Type of unit
/// * `unit_id` - ID of unit
/// * `started_at` - Start timestamp (Unix ms)
/// * `updates` - Optional fields to update
///
/// # Returns
/// The updated runtime record
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::unit_runtime::*;
///
/// let record = write_unit_runtime_record(
///     &Path::new("/project"),
///     "execute-task",
///     "M01/S01/T01",
///     1700000000000,
///     &[("phase", UnitRuntimePhase::Dispatched)]
/// );
/// ```
pub fn write_unit_runtime_record(
    base: &Path,
    unit_type: &str,
    unit_id: &str,
    started_at: u64,
    updates: &[(&str, &str)],
) -> Result<UnitRuntimeRecord> {
    use std::fs;

    let runtime_dir = runtime_dir(base);
    fs::create_dir_all(&runtime_dir)?;

    let runtime_path = runtime_path(base, unit_type, unit_id);

    // Read previous record if exists
    let prev = read_unit_runtime_record(base, unit_type, unit_id);

    // Parse updates
    let phase = updates
        .iter()
        .find(|(k, _)| *k == "phase")
        .map(|(_, v)| parse_phase(v))
        .unwrap_or_else(|| {
            prev.as_ref()
                .map(|p| p.phase.clone())
                .unwrap_or(UnitRuntimePhase::Dispatched)
        });

    let wrapup_warning_sent = updates
        .iter()
        .find(|(k, _)| *k == "wrapup_warning_sent")
        .map(|(_, v)| *v == "true")
        .unwrap_or_else(|| {
            prev.as_ref()
                .map(|p| p.wrapup_warning_sent)
                .unwrap_or(false)
        });

    let continue_here_fired = updates
        .iter()
        .find(|(k, _)| *k == "continue_here_fired")
        .map(|(_, v)| *v == "true")
        .unwrap_or_else(|| {
            prev.as_ref()
                .map(|p| p.continue_here_fired)
                .unwrap_or(false)
        });

    let timeout_at = updates
        .iter()
        .find(|(k, _)| *k == "timeout_at")
        .and_then(|(_, v)| v.parse::<u64>().ok())
        .or_else(|| prev.as_ref().and_then(|p| p.timeout_at));

    let progress_count = prev.as_ref().map(|p| p.progress_count).unwrap_or(0)
        + updates
            .iter()
            .filter(|(k, _)| *k == "progress_increment")
            .count() as u32;

    let last_progress_kind = updates
        .iter()
        .find(|(k, _)| *k == "progress_kind")
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| {
            prev.as_ref()
                .map(|p| p.last_progress_kind.clone())
                .unwrap_or_else(|| "dispatch".to_string())
        });

    let recovery_attempts = updates
        .iter()
        .find(|(k, _)| *k == "recovery_increment")
        .map(|_| prev.as_ref().map(|p| p.recovery_attempts + 1).unwrap_or(1))
        .unwrap_or_else(|| prev.as_ref().map(|p| p.recovery_attempts).unwrap_or(0));

    let last_recovery_reason = updates
        .iter()
        .find(|(k, _)| *k == "recovery_reason")
        .and_then(|(_, v)| parse_recovery_reason(v))
        .or_else(|| prev.as_ref().and_then(|p| p.last_recovery_reason.clone()));

    let now = chrono::Utc::now().timestamp_millis() as u64;

    let record = UnitRuntimeRecord {
        version: 1,
        unit_type: unit_type.to_string(),
        unit_id: unit_id.to_string(),
        started_at,
        updated_at: now,
        phase,
        wrapup_warning_sent,
        continue_here_fired,
        timeout_at,
        last_progress_at: now,
        progress_count,
        last_progress_kind,
        recovery: prev.and_then(|p| p.recovery),
        recovery_attempts,
        last_recovery_reason,
    };

    // Write record
    let json = serde_json::to_string_pretty(&record)?;
    fs::write(&runtime_path, json)?;

    debug!(
        "Wrote runtime record for {}:{} at {:?}",
        unit_type, unit_id, runtime_path
    );

    Ok(record)
}

/// Read a unit runtime record from disk
///
/// Returns None if the record doesn't exist
pub fn read_unit_runtime_record(
    base: &Path,
    unit_type: &str,
    unit_id: &str,
) -> Option<UnitRuntimeRecord> {
    use std::fs;

    let runtime_path = runtime_path(base, unit_type, unit_id);

    if !runtime_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&runtime_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Clear (delete) a unit runtime record
pub fn clear_unit_runtime_record(base: &Path, unit_type: &str, unit_id: &str) -> Result<()> {
    use std::fs;

    let runtime_path = runtime_path(base, unit_type, unit_id);

    if runtime_path.exists() {
        fs::remove_file(&runtime_path)?;
        debug!("Cleared runtime record for {}:{}", unit_type, unit_id);
    }

    Ok(())
}

/// List all runtime records currently on disk
///
/// Returns an empty vector if the runtime directory doesn't exist
pub fn list_unit_runtime_records(base: &Path) -> Vec<UnitRuntimeRecord> {
    use std::fs;

    let runtime_dir = runtime_dir(base);

    if !runtime_dir.exists() {
        return Vec::new();
    }

    let mut results = Vec::new();

    let entries = match fs::read_dir(&runtime_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().map(|e| e == "json").unwrap_or(false) {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(record) = serde_json::from_str::<UnitRuntimeRecord>(&content) {
                results.push(record);
            }
        }
    }

    results
}

/// Format execute-task recovery status for display
pub fn format_execute_task_recovery_status(status: &ExecuteTaskRecoveryStatus) -> String {
    let mut missing = Vec::new();

    if !status.summary_exists {
        missing.push(format!("summary missing ({})", status.summary_path));
    }

    if !status.task_checked {
        missing.push(format!("task checkbox unchecked in {}", status.plan_path));
    }

    if !status.next_action_advanced {
        missing.push("state next action still points at the timed-out task".to_string());
    }

    if status.must_have_count > 0 && status.must_haves_mentioned_in_summary < status.must_have_count
    {
        missing.push(format!(
            "must-have gap: {} of {} must-haves addressed in summary",
            status.must_haves_mentioned_in_summary, status.must_have_count
        ));
    }

    if missing.is_empty() {
        "all durable task artifacts present".to_string()
    } else {
        missing.join("; ")
    }
}

// ─── Helper Functions ─────────────────────────────────────────────────────────

fn runtime_dir(base: &Path) -> PathBuf {
    base.join(".orchestra").join("runtime").join("units")
}

fn runtime_path(base: &Path, unit_type: &str, unit_id: &str) -> PathBuf {
    let sanitized_unit_type = unit_type.replace('/', "-");
    let sanitized_unit_id = unit_id.replace('/', "-");
    runtime_dir(base).join(format!(
        "{}-{}.json",
        sanitized_unit_type, sanitized_unit_id
    ))
}

fn parse_phase(s: &str) -> UnitRuntimePhase {
    match s {
        "dispatched" => UnitRuntimePhase::Dispatched,
        "wrapup-warning-sent" => UnitRuntimePhase::WrapupWarningSent,
        "timeout" => UnitRuntimePhase::Timeout,
        "recovered" => UnitRuntimePhase::Recovered,
        "finalized" => UnitRuntimePhase::Finalized,
        "paused" => UnitRuntimePhase::Paused,
        "skipped" => UnitRuntimePhase::Skipped,
        _ => UnitRuntimePhase::Dispatched,
    }
}

fn parse_recovery_reason(s: &str) -> Option<RecoveryReason> {
    match s {
        "idle" => Some(RecoveryReason::Idle),
        "hard" => Some(RecoveryReason::Hard),
        _ => None,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_runtime_dir() {
        let base = Path::new("/project");
        let dir = runtime_dir(base);

        assert!(dir.ends_with(".orchestra/runtime/units"));
    }

    #[test]
    fn test_runtime_path() {
        let base = Path::new("/project");
        let path = runtime_path(base, "execute-task", "M01/S01/T01");

        assert!(path.ends_with(".orchestra/runtime/units/execute-task-M01-S01-T01.json"));
    }

    #[test]
    fn test_write_and_read_runtime_record() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        let record =
            write_unit_runtime_record(base, "execute-task", "M01/S01/T01", 1700000000000, &[])
                .unwrap();

        assert_eq!(record.unit_type, "execute-task");
        assert_eq!(record.unit_id, "M01/S01/T01");
        assert_eq!(record.started_at, 1700000000000);
        assert_eq!(record.phase, UnitRuntimePhase::Dispatched);

        let read = read_unit_runtime_record(base, "execute-task", "M01/S01/T01");
        assert!(read.is_some());

        let read = read.unwrap();
        assert_eq!(read.unit_type, "execute-task");
        assert_eq!(record.started_at, read.started_at);
    }

    #[test]
    fn test_write_with_phase_update() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        write_unit_runtime_record(base, "execute-task", "M01/S01/T01", 1700000000000, &[]).unwrap();

        let updated = write_unit_runtime_record(
            base,
            "execute-task",
            "M01/S01/T01",
            1700000000000,
            &[("phase", "timeout")],
        )
        .unwrap();

        assert_eq!(updated.phase, UnitRuntimePhase::Timeout);
    }

    #[test]
    fn test_write_with_progress_increment() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        let record1 =
            write_unit_runtime_record(base, "execute-task", "M01/S01/T01", 1700000000000, &[])
                .unwrap();

        assert_eq!(record1.progress_count, 0);

        let record2 = write_unit_runtime_record(
            base,
            "execute-task",
            "M01/S01/T01",
            1700000000000,
            &[("progress_increment", "")],
        )
        .unwrap();

        assert_eq!(record2.progress_count, 1);
    }

    #[test]
    fn test_clear_runtime_record() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        write_unit_runtime_record(base, "execute-task", "M01/S01/T01", 1700000000000, &[]).unwrap();

        let result = read_unit_runtime_record(base, "execute-task", "M01/S01/T01");
        assert!(result.is_some());

        clear_unit_runtime_record(base, "execute-task", "M01/S01/T01").unwrap();

        let result = read_unit_runtime_record(base, "execute-task", "M01/S01/T01");
        assert!(result.is_none());
    }

    #[test]
    fn test_list_empty_runtime_records() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        let records = list_unit_runtime_records(base);
        assert_eq!(records.len(), 0);
    }

    #[test]
    fn test_list_multiple_runtime_records() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        write_unit_runtime_record(base, "execute-task", "M01/S01/T01", 1700000000000, &[]).unwrap();

        write_unit_runtime_record(base, "plan-slice", "M01/S01", 1700000000000, &[]).unwrap();

        let records = list_unit_runtime_records(base);
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn test_format_execute_task_recovery_status_all_present() {
        let status = ExecuteTaskRecoveryStatus {
            plan_path: ".orchestra/milestones/M01/slices/S01/S01-PLAN.md".to_string(),
            summary_path: ".orchestra/milestones/M01/slices/S01/tasks/T01-SUMMARY.md".to_string(),
            summary_exists: true,
            task_checked: true,
            next_action_advanced: true,
            must_have_count: 0,
            must_haves_mentioned_in_summary: 0,
        };

        let formatted = format_execute_task_recovery_status(&status);
        assert_eq!(formatted, "all durable task artifacts present");
    }

    #[test]
    fn test_format_execute_task_recovery_status_missing() {
        let status = ExecuteTaskRecoveryStatus {
            plan_path: ".orchestra/milestones/M01/slices/S01/S01-PLAN.md".to_string(),
            summary_path: ".orchestra/milestones/M01/slices/S01/tasks/T01-SUMMARY.md".to_string(),
            summary_exists: false,
            task_checked: false,
            next_action_advanced: false,
            must_have_count: 3,
            must_haves_mentioned_in_summary: 1,
        };

        let formatted = format_execute_task_recovery_status(&status);
        assert!(formatted.contains("summary missing"));
        assert!(formatted.contains("task checkbox unchecked"));
        assert!(formatted.contains("must-have gap"));
    }

    #[test]
    fn test_parse_phase() {
        assert!(matches!(
            parse_phase("dispatched"),
            UnitRuntimePhase::Dispatched
        ));
        assert!(matches!(parse_phase("timeout"), UnitRuntimePhase::Timeout));
        assert!(matches!(
            parse_phase("recovered"),
            UnitRuntimePhase::Recovered
        ));
        assert!(matches!(
            parse_phase("finalized"),
            UnitRuntimePhase::Finalized
        ));
        assert!(matches!(
            parse_phase("unknown"),
            UnitRuntimePhase::Dispatched
        ));
    }

    #[test]
    fn test_parse_recovery_reason() {
        assert_eq!(parse_recovery_reason("idle"), Some(RecoveryReason::Idle));
        assert_eq!(parse_recovery_reason("hard"), Some(RecoveryReason::Hard));
        assert_eq!(parse_recovery_reason("unknown"), None);
    }
}
