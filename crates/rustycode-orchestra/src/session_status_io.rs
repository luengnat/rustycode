// rustycode-orchestra/src/session_status_io.rs
//! Orchestra Session Status I/O — File-based IPC for parallel milestone orchestration.
//!
//! Each worker writes its status to a file; the coordinator reads all status
//! files to monitor progress. Atomic writes prevent partial reads. Signal files
//! let the coordinator send pause/resume/stop/rebase to workers. Stale
//! detection combines PID liveness checks with heartbeat timeouts.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::json_persistence::{load_json_file_or_null, write_json_file_atomic};
use crate::paths::orchestra_root;

/// Parallel directory name.
const PARALLEL_DIR: &str = "parallel";

/// Status file suffix.
const STATUS_SUFFIX: &str = ".status.json";

/// Signal file suffix.
const SIGNAL_SUFFIX: &str = ".signal.json";

/// Default stale timeout in milliseconds.
const DEFAULT_STALE_TIMEOUT_MS: u64 = 30_000;

/// Session state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum SessionState {
    Running,
    Paused,
    Stopped,
    Error,
}

/// Session signal from coordinator to worker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum SessionSignal {
    Pause,
    Resume,
    Stop,
    Rebase,
}

/// Current unit being executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUnit {
    pub unit_type: String,
    pub unit_id: String,
    pub started_at: u64,
}

/// Session status written by workers and read by coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatus {
    pub milestone_id: String,
    pub pid: u32,
    pub state: SessionState,
    pub current_unit: Option<CurrentUnit>,
    pub completed_units: u32,
    pub cost: f64,
    pub last_heartbeat: u64,
    pub started_at: u64,
    pub worktree_path: String,
}

/// Signal message from coordinator to worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalMessage {
    pub signal: SessionSignal,
    pub sent_at: u64,
    #[serde(default)]
    pub from: String,
}

impl SignalMessage {
    /// Create a new signal message from coordinator.
    pub fn new(signal: SessionSignal) -> Self {
        SignalMessage {
            signal,
            sent_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            from: "coordinator".to_string(),
        }
    }
}

// ─── Path Helpers ─────────────────────────────────────────────────────────────

/// Get the parallel directory path.
fn parallel_dir(base_path: &Path) -> PathBuf {
    orchestra_root(base_path).join(PARALLEL_DIR)
}

/// Get the status file path for a milestone.
fn status_path(base_path: &Path, milestone_id: &str) -> PathBuf {
    parallel_dir(base_path).join(format!("{}{}", milestone_id, STATUS_SUFFIX))
}

/// Get the signal file path for a milestone.
fn signal_path(base_path: &Path, milestone_id: &str) -> PathBuf {
    parallel_dir(base_path).join(format!("{}{}", milestone_id, SIGNAL_SUFFIX))
}

/// Ensure the parallel directory exists.
fn ensure_parallel_dir(base_path: &Path) -> io::Result<()> {
    let dir = parallel_dir(base_path);
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(())
}

// ─── Status I/O ───────────────────────────────────────────────────────────────

/// Write session status atomically.
///
/// Uses atomic write pattern (write to .tmp, then rename) to prevent partial reads.
///
/// # Arguments
/// * `base_path` - Project root path
/// * `status` - Session status to write
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::session_status_io::{write_session_status, SessionStatus, SessionState};
/// use std::path::Path;
///
/// let status = SessionStatus {
///     milestone_id: "M001".to_string(),
///     pid: 12345,
///     state: SessionState::Running,
///     current_unit: None,
///     completed_units: 0,
///     cost: 0.0,
///     last_heartbeat: 1234567890,
///     started_at: 1234567890,
///     worktree_path: "/path/to/worktree".to_string(),
/// };
/// write_session_status(Path::new("/project"), &status).unwrap();
/// ```
pub fn write_session_status(base_path: &Path, status: &SessionStatus) -> io::Result<()> {
    ensure_parallel_dir(base_path)?;
    write_json_file_atomic(&status_path(base_path, &status.milestone_id), status)
}

/// Read a specific milestone's session status.
///
/// # Arguments
/// * `base_path` - Project root path
/// * `milestone_id` - Milestone identifier
///
/// # Returns
/// Session status if file exists and is valid, None otherwise
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::session_status_io::read_session_status;
/// use std::path::Path;
///
/// let status = read_session_status(Path::new("/project"), "M001");
/// ```
pub fn read_session_status(base_path: &Path, milestone_id: &str) -> Option<SessionStatus> {
    load_json_file_or_null(
        &status_path(base_path, milestone_id),
        |_status: &SessionStatus| true,
    )
}

/// Read all session status files from .orchestra/parallel/.
///
/// # Arguments
/// * `base_path` - Project root path
///
/// # Returns
/// Vector of all valid session statuses found
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::session_status_io::read_all_session_statuses;
/// use std::path::Path;
///
/// let statuses = read_all_session_statuses(Path::new("/project"));
/// ```
pub fn read_all_session_statuses(base_path: &Path) -> Vec<SessionStatus> {
    let dir = parallel_dir(base_path);
    if !dir.exists() {
        return Vec::new();
    }

    let mut results = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if !name.ends_with(STATUS_SUFFIX) {
                    continue;
                }
            }

            if let Some(status) = load_json_file_or_null(&path, |_status: &SessionStatus| true) {
                results.push(status);
            }
        }
    }

    results
}

/// Remove a milestone's session status file.
///
/// # Arguments
/// * `base_path` - Project root path
/// * `milestone_id` - Milestone identifier
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::session_status_io::remove_session_status;
/// use std::path::Path;
///
/// remove_session_status(Path::new("/project"), "M001").unwrap();
/// ```
pub fn remove_session_status(base_path: &Path, milestone_id: &str) -> io::Result<()> {
    let p = status_path(base_path, milestone_id);
    if p.exists() {
        fs::remove_file(p)?;
    }
    Ok(())
}

// ─── Signal I/O ───────────────────────────────────────────────────────────────

/// Write a signal file for a worker to consume.
///
/// # Arguments
/// * `base_path` - Project root path
/// * `milestone_id` - Milestone identifier
/// * `signal` - Signal to send
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::session_status_io::{send_signal, SessionSignal};
/// use std::path::Path;
///
/// send_signal(Path::new("/project"), "M001", SessionSignal::Pause).unwrap();
/// ```
pub fn send_signal(base_path: &Path, milestone_id: &str, signal: SessionSignal) -> io::Result<()> {
    ensure_parallel_dir(base_path)?;
    let msg = SignalMessage::new(signal);
    write_json_file_atomic(&signal_path(base_path, milestone_id), &msg)
}

/// Read and delete a signal file (atomic consume).
///
/// # Arguments
/// * `base_path` - Project root path
/// * `milestone_id` - Milestone identifier
///
/// # Returns
/// Signal message if one was pending, None otherwise
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::session_status_io::consume_signal;
/// use std::path::Path;
///
/// let signal = consume_signal(Path::new("/project"), "M001");
/// ```
pub fn consume_signal(base_path: &Path, milestone_id: &str) -> Option<SignalMessage> {
    let p = signal_path(base_path, milestone_id);
    if !p.exists() {
        return None;
    }

    // Atomically consume: rename to temp, then read from temp
    // This prevents two concurrent consumers from reading the same signal
    let tmp = p.with_extension("consuming");
    match fs::rename(&p, &tmp) {
        Ok(()) => {
            let msg = load_json_file_or_null(&tmp, |_msg: &SignalMessage| true);
            let _ = fs::remove_file(&tmp); // Clean up temp file
            msg
        }
        Err(_) => {
            // Another consumer already renamed the file
            None
        }
    }
}

// ─── Stale Detection ───────────────────────────────────────────────────────────

/// Check whether a PID is alive.
///
/// On Unix, sends signal 0 to check. On Windows, assumes PID is alive
/// (platform-specific implementation would be needed for accuracy).
///
/// # Arguments
/// * `pid` - Process ID to check
///
/// # Returns
/// true if process appears to be alive, false otherwise
#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    use std::process::Command;
    // On Unix, use kill -0 to check if process exists
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Windows stub - assumes PID is alive (platform-specific check needed).
#[cfg(windows)]
fn is_pid_alive(_pid: u32) -> bool {
    // Windows would need different implementation (e.g., OpenProcess)
    // For now, assume alive to avoid false positives
    true
}

/// Check whether a session is stale (PID dead or heartbeat timed out).
///
/// # Arguments
/// * `status` - Session status to check
/// * `timeout_ms` - Heartbeat timeout in milliseconds (default: 30000)
///
/// # Returns
/// true if session is stale, false otherwise
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::session_status_io::{is_session_stale, SessionStatus, SessionState};
///
/// let status = SessionStatus {
///     milestone_id: "M001".to_string(),
///     pid: 12345,
///     state: SessionState::Running,
///     current_unit: None,
///     completed_units: 0,
///     cost: 0.0,
///     last_heartbeat: 1234567890,
///     started_at: 1234567890,
///     worktree_path: "/path".to_string(),
/// };
/// let stale = is_session_stale(&status, 30000);
/// ```
pub fn is_session_stale(status: &SessionStatus, timeout_ms: Option<u64>) -> bool {
    if !is_pid_alive(status.pid) {
        return true;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let elapsed = now.saturating_sub(status.last_heartbeat);
    let timeout = timeout_ms.unwrap_or(DEFAULT_STALE_TIMEOUT_MS);

    elapsed > timeout
}

/// Find and remove stale sessions.
///
/// # Arguments
/// * `base_path` - Project root path
/// * `timeout_ms` - Heartbeat timeout in milliseconds (default: 30000)
///
/// # Returns
/// List of milestone IDs that were cleaned up
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::session_status_io::cleanup_stale_sessions;
/// use std::path::Path;
///
/// let removed = cleanup_stale_sessions(Path::new("/project"), Some(60000));
/// ```
pub fn cleanup_stale_sessions(base_path: &Path, timeout_ms: Option<u64>) -> Vec<String> {
    let mut removed = Vec::new();
    let statuses = read_all_session_statuses(base_path);

    for status in statuses {
        if is_session_stale(&status, timeout_ms) {
            let _ = remove_session_status(base_path, &status.milestone_id);

            // Also clean up any lingering signal file
            let sig_path = signal_path(base_path, &status.milestone_id);
            if sig_path.exists() {
                let _ = fs::remove_file(sig_path);
            }

            removed.push(status.milestone_id);
        }
    }

    removed
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_and_read_session_status() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let status = SessionStatus {
            milestone_id: "M001".to_string(),
            pid: 12345,
            state: SessionState::Running,
            current_unit: Some(CurrentUnit {
                unit_type: "plan".to_string(),
                unit_id: "P01".to_string(),
                started_at: 1234567890,
            }),
            completed_units: 5,
            cost: 1.23,
            last_heartbeat: 1234567890,
            started_at: 1234567000,
            worktree_path: "/path/to/worktree".to_string(),
        };

        write_session_status(base_path, &status).unwrap();
        let read = read_session_status(base_path, "M001");

        assert!(read.is_some());
        let read = read.unwrap();
        assert_eq!(read.milestone_id, "M001");
        assert_eq!(read.pid, 12345);
        assert_eq!(read.state, SessionState::Running);
        assert_eq!(read.completed_units, 5);
    }

    #[test]
    fn test_read_nonexistent_status() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let status = read_session_status(base_path, "M999");
        assert!(status.is_none());
    }

    #[test]
    fn test_read_all_session_statuses() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let status1 = SessionStatus {
            milestone_id: "M001".to_string(),
            pid: 12345,
            state: SessionState::Running,
            current_unit: None,
            completed_units: 0,
            cost: 0.0,
            last_heartbeat: 1234567890,
            started_at: 1234567000,
            worktree_path: "/path".to_string(),
        };

        let status2 = SessionStatus {
            milestone_id: "M002".to_string(),
            pid: 12346,
            state: SessionState::Paused,
            current_unit: None,
            completed_units: 3,
            cost: 0.5,
            last_heartbeat: 1234567890,
            started_at: 1234567000,
            worktree_path: "/path2".to_string(),
        };

        write_session_status(base_path, &status1).unwrap();
        write_session_status(base_path, &status2).unwrap();

        let all = read_all_session_statuses(base_path);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_remove_session_status() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let status = SessionStatus {
            milestone_id: "M001".to_string(),
            pid: 12345,
            state: SessionState::Running,
            current_unit: None,
            completed_units: 0,
            cost: 0.0,
            last_heartbeat: 1234567890,
            started_at: 1234567000,
            worktree_path: "/path".to_string(),
        };

        write_session_status(base_path, &status).unwrap();
        assert!(read_session_status(base_path, "M001").is_some());

        remove_session_status(base_path, "M001").unwrap();
        assert!(read_session_status(base_path, "M001").is_none());
    }

    #[test]
    fn test_send_and_consume_signal() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        send_signal(base_path, "M001", SessionSignal::Pause).unwrap();

        let signal = consume_signal(base_path, "M001");
        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.signal, SessionSignal::Pause);
        assert_eq!(signal.from, "coordinator");

        // Second consume should return None (file was deleted)
        let signal2 = consume_signal(base_path, "M001");
        assert!(signal2.is_none());
    }

    #[test]
    fn test_consume_nonexistent_signal() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let signal = consume_signal(base_path, "M999");
        assert!(signal.is_none());
    }

    #[test]
    fn test_signal_message_new() {
        let msg = SignalMessage::new(SessionSignal::Stop);
        assert_eq!(msg.signal, SessionSignal::Stop);
        assert_eq!(msg.from, "coordinator");
        assert!(msg.sent_at > 0);
    }

    #[test]
    fn test_session_state_equality() {
        assert_eq!(SessionState::Running, SessionState::Running);
        assert_ne!(SessionState::Running, SessionState::Paused);
    }

    #[test]
    fn test_session_signal_equality() {
        assert_eq!(SessionSignal::Pause, SessionSignal::Pause);
        assert_ne!(SessionSignal::Pause, SessionSignal::Resume);
    }

    #[test]
    fn test_parallel_dir_path() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let dir = parallel_dir(base_path);

        assert!(dir.ends_with("parallel"));
        assert!(dir.to_string_lossy().contains(".orchestra"));
    }

    #[test]
    fn test_status_path_format() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let path = status_path(base_path, "M001");

        let path_str = path.to_string_lossy();
        assert!(path_str.contains("M001.status.json"));
    }

    #[test]
    fn test_signal_path_format() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let path = signal_path(base_path, "M002");

        let path_str = path.to_string_lossy();
        assert!(path_str.contains("M002.signal.json"));
    }

    #[test]
    fn test_ensure_parallel_dir_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let parallel_path = parallel_dir(base_path);

        assert!(!parallel_path.exists());
        ensure_parallel_dir(base_path).unwrap();
        assert!(parallel_path.exists());
    }

    #[test]
    fn test_is_session_stale_dead_process() {
        let status = SessionStatus {
            milestone_id: "M001".to_string(),
            pid: 99999, // Non-existent PID
            state: SessionState::Running,
            current_unit: None,
            completed_units: 0,
            cost: 0.0,
            last_heartbeat: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            started_at: 1234567000,
            worktree_path: "/path".to_string(),
        };

        // Should be stale because PID doesn't exist
        #[cfg(unix)]
        assert!(is_session_stale(&status, Some(60000)));
    }

    #[test]
    fn test_cleanup_stale_sessions() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let status = SessionStatus {
            milestone_id: "M001".to_string(),
            pid: 99999, // Non-existent PID
            state: SessionState::Running,
            current_unit: None,
            completed_units: 0,
            cost: 0.0,
            last_heartbeat: 1234567000,
            started_at: 1234567000,
            worktree_path: "/path".to_string(),
        };

        write_session_status(base_path, &status).unwrap();
        assert!(read_session_status(base_path, "M001").is_some());

        let removed = cleanup_stale_sessions(base_path, Some(60000));

        #[cfg(unix)]
        assert!(!removed.is_empty());

        #[cfg(unix)]
        assert!(read_session_status(base_path, "M001").is_none());
    }

    #[test]
    fn test_cleanup_stale_sessions_removes_signal_file() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let status = SessionStatus {
            milestone_id: "M001".to_string(),
            pid: 99999,
            state: SessionState::Running,
            current_unit: None,
            completed_units: 0,
            cost: 0.0,
            last_heartbeat: 1234567000,
            started_at: 1234567000,
            worktree_path: "/path".to_string(),
        };

        write_session_status(base_path, &status).unwrap();
        send_signal(base_path, "M001", SessionSignal::Stop).unwrap();

        let signal_path = signal_path(base_path, "M001");
        assert!(signal_path.exists());

        let _removed = cleanup_stale_sessions(base_path, Some(60000));

        #[cfg(unix)]
        assert!(!signal_path.exists());
    }

    #[test]
    fn test_default_constants() {
        assert_eq!(PARALLEL_DIR, "parallel");
        assert_eq!(STATUS_SUFFIX, ".status.json");
        assert_eq!(SIGNAL_SUFFIX, ".signal.json");
        assert_eq!(DEFAULT_STALE_TIMEOUT_MS, 30_000);
    }

    #[test]
    fn test_session_status_with_none_current_unit() {
        let status = SessionStatus {
            milestone_id: "M001".to_string(),
            pid: 12345,
            state: SessionState::Stopped,
            current_unit: None,
            completed_units: 10,
            cost: 2.5,
            last_heartbeat: 1234567890,
            started_at: 1234567000,
            worktree_path: "/path".to_string(),
        };

        assert!(status.current_unit.is_none());
        assert_eq!(status.completed_units, 10);
    }
}
