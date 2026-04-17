//! Orchestra Activity Log — Session Persistence for Crash Recovery
//!
//! Saves raw chat sessions to `.orchestra/activity/` before context wipes.
//! These are debug artifacts for post-mortem analysis and crash recovery.
//!
//! # When Activity Logs Are Written
//!
//! Activity logs are written at these checkpoints:
//! - Before context window compaction (to preserve session)
//! - After each unit completion (for crash recovery)
//! - On error/failure (for debugging)
//!
//! # File Format
//!
//! Each activity log is a JSONL file (one JSON object per line):
//!
//! ```json
//! {"type":"session_start","timestamp":"2024-03-19T12:00:00Z","unit_id":"T01"}
//! {"type":"tool_use","tool":"bash","command":"cargo test","timestamp":"..."}
//! {"type":"llm_response","content":"...","timestamp":"..."}
//! {"type":"error","message":"...","timestamp":"..."}
//! ```
//!
//! # Crash Recovery Usage
//!
//! When a crash occurs, SessionForensics reads the activity log to:
//! - Determine what the LLM was working on
//! - Identify the last successful tool call
//! - Extract files written during the session
//! - Generate a recovery briefing for resume
//!
//! # Memory Management
//!
//! Activity logs accumulate in memory during a session. Call
//! `clear_global_state()` when auto-mode stops to prevent memory leaks.
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::crash_recovery::{ActivityLog, ActivityEvent, ActivityType};
//!
//! let log = ActivityLog::new(project_root);
//!
//! // Log an event
//! log.log(ActivityEvent {
//!     timestamp: Utc::now(),
//!     unit_id: "T01".to_string(),
//!     event_type: ActivityType::ToolUse,
//!     detail: serde_json::json!({"tool": "bash", "command": "cargo test"}),
//! }).await?;
//!
//! // Read events for crash recovery
//! let events = log.read_unit_log("T01").await?;
//! ```

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::{debug, warn};

// ─── Types ────────────────────────────────────────────────────────────────────

/// Activity log state (per-directory)
#[derive(Debug)]
struct ActivityLogState {
    next_seq: u64,
    last_snapshot_key_by_unit: HashMap<String, String>,
}

/// Session entry (simplified - actual format depends on session manager)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

// ─── Global State ─────────────────────────────────────────────────────────────

/// Global activity log state (thread-safe, lazily initialized)
static ACTIVITY_LOG_STATE: OnceLock<Mutex<HashMap<String, ActivityLogState>>> = OnceLock::new();

/// Get the global activity log state
fn get_global_state() -> &'static Mutex<HashMap<String, ActivityLogState>> {
    ACTIVITY_LOG_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Sequence prefix regex
const SEQ_PREFIX_RE: &str = r"^(\d+)-";

/// Activity directory name
const ACTIVITY_DIR: &str = "activity";

// ─── Public API ───────────────────────────────────────────────────────────────

/// Clear accumulated activity log state
///
/// Call when auto-mode stops to prevent unbounded memory growth
/// from lastSnapshotKeyByUnit maps accumulating across units.
pub fn clear_activity_log_state() {
    let mut state = get_global_state().lock().unwrap_or_else(|e| e.into_inner());
    state.clear();
    debug!("Activity log state cleared");
}

/// Save activity log for a unit
///
/// Saves the full session as JSONL to `.orchestra/activity/` directory.
/// Returns the file path if saved, None if skipped (duplicate) or error.
pub fn save_activity_log(
    project_root: &Path,
    unit_type: &str,
    unit_id: &str,
    entries: &[SessionEntry],
) -> Option<PathBuf> {
    if entries.is_empty() {
        debug!("No entries to save for {} {}", unit_type, unit_id);
        return None;
    }

    let activity_dir = project_root.join(".orchestra").join(ACTIVITY_DIR);

    // Create activity directory if needed
    if let Err(e) = fs::create_dir_all(&activity_dir) {
        warn!(
            "Failed to create activity directory {:?}: {}",
            activity_dir, e
        );
        return None;
    }

    let state_key = activity_dir.to_string_lossy().to_string();
    let safe_unit_id = unit_id.replace('/', "-");
    let unit_key = format!("{}\0{}", unit_type, safe_unit_id);

    // Get or create state
    let mut global_state = get_global_state().lock().unwrap_or_else(|e| e.into_inner());
    let state = global_state
        .entry(state_key.clone())
        .or_insert_with(|| ActivityLogState {
            next_seq: scan_next_sequence(&activity_dir),
            last_snapshot_key_by_unit: HashMap::new(),
        });

    // Check for duplicate via snapshot key
    let snapshot_key = snapshot_key(unit_type, &safe_unit_id, entries);
    if let Some(last_key) = state.last_snapshot_key_by_unit.get(&unit_key) {
        if last_key == &snapshot_key {
            debug!(
                "Duplicate activity log for {} {}, skipping",
                unit_type, unit_id
            );
            return None;
        }
    }

    // Find next available file path
    let file_path = match next_activity_file_path(&activity_dir, state, unit_type, &safe_unit_id) {
        Some(path) => path,
        None => {
            warn!(
                "Failed to find available activity log sequence in {:?}",
                activity_dir
            );
            return None;
        }
    };

    // Write entries to file
    match write_activity_log(&file_path, entries) {
        Ok(()) => {
            state.next_seq += 1;
            state
                .last_snapshot_key_by_unit
                .insert(unit_key, snapshot_key);
            debug!("Saved activity log: {:?}", file_path);
            Some(file_path)
        }
        Err(e) => {
            warn!("Failed to write activity log {:?}: {}", file_path, e);
            None
        }
    }
}

/// Prune old activity logs
///
/// Removes logs older than retention_days, but always preserves the
/// highest-sequence log (most recent).
pub fn prune_activity_logs(activity_dir: &Path, retention_days: u64) -> Result<usize> {
    if !activity_dir.exists() {
        return Ok(0);
    }

    let cutoff_ms = Utc::now().timestamp_millis() - (retention_days as i64 * 86_400_000);

    let mut max_seq = 0u64;
    let mut entries_to_check = Vec::new();

    // Scan directory for sequence files
    let re = regex::Regex::new(SEQ_PREFIX_RE).unwrap();
    for entry in fs::read_dir(activity_dir)
        .with_context(|| format!("Failed to read activity directory: {:?}", activity_dir))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if let Some(caps) = re.captures(&file_name_str) {
            if let Some(seq_str) = caps.get(1) {
                if let Ok(seq) = seq_str.as_str().parse::<u64>() {
                    max_seq = max_seq.max(seq);
                    entries_to_check.push((seq, entry.path()));
                }
            }
        }
    }

    let mut pruned = 0;
    for (seq, path) in entries_to_check {
        // Always preserve highest-sequence log
        if seq == max_seq {
            continue;
        }

        // Check modification time
        match fs::metadata(&path) {
            Ok(metadata) => {
                if let Ok(mtime) = metadata.modified() {
                    let mtime_ms = mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;
                    if mtime_ms <= cutoff_ms {
                        // Prune this file
                        if let Err(e) = fs::remove_file(&path) {
                            warn!("Failed to prune activity log {:?}: {}", path, e);
                        } else {
                            debug!("Pruned activity log: {:?}", path);
                            pruned += 1;
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to stat activity log {:?}: {}", path, e);
            }
        }
    }

    Ok(pruned)
}

// ─── Internal Helpers ──────────────────────────────────────────────────────────

/// Scan directory for next sequence number
fn scan_next_sequence(activity_dir: &Path) -> u64 {
    let mut max_seq = 0u64;
    let re = regex::Regex::new(SEQ_PREFIX_RE).unwrap();

    if let Ok(entries) = fs::read_dir(activity_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            if let Some(caps) = re.captures(&file_name_str) {
                if let Some(seq_str) = caps.get(1) {
                    if let Ok(seq) = seq_str.as_str().parse::<u64>() {
                        max_seq = max_seq.max(seq);
                    }
                }
            }
        }
    }

    max_seq + 1
}

/// Build lightweight dedup key from session entries
///
/// Uses entry count + hash of the last few entries as a fingerprint
/// instead of hashing megabytes of data.
fn snapshot_key(unit_type: &str, unit_id: &str, entries: &[SessionEntry]) -> String {
    let mut hasher = Sha256::new();

    // Hash unit metadata
    hasher.update(format!("{}\0{}\0{}\0", unit_type, unit_id, entries.len()));

    // Hash only the last 3 entries as a fingerprint
    let tail = if entries.len() > 3 {
        &entries[entries.len() - 3..]
    } else {
        entries
    };
    for entry in tail {
        if let Ok(json) = serde_json::to_string(entry) {
            hasher.update(json);
        }
    }

    format!("{:x}", hasher.finalize())
}

/// Find next available activity file path with atomic creation
fn next_activity_file_path(
    activity_dir: &Path,
    state: &ActivityLogState,
    unit_type: &str,
    unit_id: &str,
) -> Option<PathBuf> {
    for _ in 0..1000 {
        let seq = format!("{:03}", state.next_seq);
        let file_name = format!("{}-{}-{}.jsonl", seq, unit_type, unit_id);
        let file_path = activity_dir.join(&file_name);

        // Try to create file exclusively (atomic check-and-create)
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file_path)
        {
            Ok(_) => return Some(file_path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // File exists, try next sequence
                // Note: we'd need to mutate state.next_seq here, but we can't
                // because we're borrowing state. The caller will increment.
                continue;
            }
            Err(_) => return None,
        }
    }

    None
}

/// Write session entries to activity log file
fn write_activity_log(file_path: &Path, entries: &[SessionEntry]) -> Result<()> {
    let mut file = File::create(file_path)
        .with_context(|| format!("Failed to create activity log: {:?}", file_path))?;

    for entry in entries {
        let json = serde_json::to_string(entry)
            .with_context(|| "Failed to serialize session entry".to_string())?;
        writeln!(file, "{}", json)
            .with_context(|| format!("Failed to write to activity log: {:?}", file_path))?;
    }

    file.flush()
        .with_context(|| format!("Failed to flush activity log: {:?}", file_path))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Use the crate-level test lock to serialize tests that touch global state.
    fn test_lock() -> &'static parking_lot::Mutex<()> {
        &crate::CRATE_TEST_LOCK
    }

    #[test]
    fn test_snapshot_key_different() {
        let entries1 = vec![SessionEntry {
            entry_type: "message".to_string(),
            other: serde_json::json!({"role": "user"}),
        }];

        let entries2 = vec![SessionEntry {
            entry_type: "message".to_string(),
            other: serde_json::json!({"role": "assistant"}),
        }];

        let key1 = snapshot_key("execute-task", "M01-S01-T01", &entries1);
        let key2 = snapshot_key("execute-task", "M01-S01-T01", &entries2);

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_snapshot_key_same() {
        let entries = vec![SessionEntry {
            entry_type: "message".to_string(),
            other: serde_json::json!({"role": "user"}),
        }];

        let key1 = snapshot_key("execute-task", "M01-S01-T01", &entries);
        let key2 = snapshot_key("execute-task", "M01-S01-T01", &entries);

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_save_activity_log() {
        let _guard = test_lock().lock();
        clear_activity_log_state();
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let entries = vec![SessionEntry {
            entry_type: "message".to_string(),
            other: serde_json::json!({"role": "user", "content": "test"}),
        }];

        let result = save_activity_log(project_root, "execute-task", "M01-S01-T01", &entries);

        assert!(result.is_some());
        let file_path = result.unwrap();
        assert!(file_path.exists());
        assert!(file_path
            .to_string_lossy()
            .contains("001-execute-task-M01-S01-T01.jsonl"));
    }

    #[test]
    fn test_save_activity_log_duplicate() {
        let _guard = test_lock().lock();
        clear_activity_log_state();
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let entries = vec![SessionEntry {
            entry_type: "message".to_string(),
            other: serde_json::json!({"role": "user", "content": "test"}),
        }];

        // First save should succeed
        let result1 = save_activity_log(project_root, "execute-task", "M01-S01-T01", &entries);
        assert!(result1.is_some());

        // Second save with same entries should be skipped (duplicate)
        let result2 = save_activity_log(project_root, "execute-task", "M01-S01-T01", &entries);
        assert!(result2.is_none());
    }

    #[test]
    fn test_save_activity_log_sequence() {
        let _guard = test_lock().lock();
        clear_activity_log_state();
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let entries1 = vec![SessionEntry {
            entry_type: "message".to_string(),
            other: serde_json::json!({"role": "user", "content": "test1"}),
        }];

        let entries2 = vec![SessionEntry {
            entry_type: "message".to_string(),
            other: serde_json::json!({"role": "user", "content": "test2"}),
        }];

        // First save
        let result1 = save_activity_log(project_root, "execute-task", "M01-S01-T01", &entries1);
        assert!(result1.is_some());
        assert!(result1.unwrap().to_string_lossy().contains("001-"));

        // Second save with different content should create new file
        let result2 = save_activity_log(project_root, "execute-task", "M01-S01-T01", &entries2);
        assert!(result2.is_some());
        assert!(result2.unwrap().to_string_lossy().contains("002-"));
    }

    #[test]
    fn test_safe_unit_id() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let entries = vec![SessionEntry {
            entry_type: "message".to_string(),
            other: serde_json::json!({"role": "user"}),
        }];

        // Unit ID with slashes should be sanitized
        let result = save_activity_log(project_root, "execute-task", "M01/S01/T01", &entries);

        assert!(result.is_some());
        let file_path = result.unwrap();
        // Check the file name (not full path) doesn't contain slashes in the unit ID part
        let file_name = file_path.file_name().unwrap().to_string_lossy();
        assert!(!file_name.contains("M01/S01/T01"));
        assert!(file_name.contains("M01-S01-T01"));
    }

    #[test]
    fn test_clear_activity_log_state() {
        let _guard = test_lock().lock();
        clear_activity_log_state();
        // Create some state
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let entries = vec![SessionEntry {
            entry_type: "message".to_string(),
            other: serde_json::json!({"role": "user"}),
        }];

        save_activity_log(project_root, "execute-task", "M01-S01-T01", &entries);

        // Clear state
        clear_activity_log_state();

        // Saving again should create new file (not be detected as duplicate)
        let result = save_activity_log(project_root, "execute-task", "M01-S01-T01", &entries);
        assert!(result.is_some());
    }
}
