//! Debug Logger — Structured JSONL debug logging for diagnosing stuck/slow Orchestra sessions
//!
//! Zero overhead when disabled — all public functions are no-ops when debug mode is off.
//! Can be activated via `--debug` flag or `Orchestra_DEBUG=1` env var.

use crate::error::Result;
use crate::paths::orchestra_root;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

// ─── State ───────────────────────────────────────────────────────────────────

/// Global debug enabled flag
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Current log file path
static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Session start time
static START_TIME: Mutex<Option<SystemTime>> = Mutex::new(None);

/// Max debug log files to keep. Older ones are pruned on enable.
const MAX_DEBUG_LOGS: usize = 5;

/// Debug counter keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DebugCounter {
    DeriveStateCalls,
    DeriveStateTotalMs,
    TtsrChecks,
    TtsrTotalMs,
    TtsrPeakBuffer,
    ParseRoadmapCalls,
    ParseRoadmapTotalMs,
    ParsePlanCalls,
    ParsePlanTotalMs,
    Dispatches,
    Renders,
}

/// Rolling counters for the debug summary written on stop
/// Rolling counters for the debug summary written on stop.
static COUNTERS: OnceLock<Mutex<HashMap<DebugCounter, u64>>> = OnceLock::new();

fn get_counters() -> &'static Mutex<HashMap<DebugCounter, u64>> {
    COUNTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Enable debug logging. Creates the log file and prunes old logs.
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Example
/// ```
/// use rustycode_orchestra::debug_logger::*;
///
/// enable_debug(project_path)?;
/// assert!(is_enabled());
/// ```
pub fn enable_debug(base_path: &Path) -> Result<()> {
    let debug_dir = orchestra_root(base_path).join("debug");
    fs::create_dir_all(&debug_dir)?;

    // Prune old debug logs
    prune_old_logs(&debug_dir)?;

    // Create new log file with timestamp
    let timestamp = format_timestamp();
    let log_path = debug_dir.join(format!("debug-{}.log", timestamp));

    // Reset counters
    let mut counters = get_counters().lock().unwrap_or_else(|e| e.into_inner());
    counters.clear();

    // Set state
    *LOG_PATH.lock().unwrap_or_else(|e| e.into_inner()) = Some(log_path.clone());
    *START_TIME.lock().unwrap_or_else(|e| e.into_inner()) = Some(SystemTime::now());
    DEBUG_ENABLED.store(true, Ordering::SeqCst);

    Ok(())
}

/// Disable debug logging and return the log file path (if any).
///
/// # Returns
/// Log file path if logging was active, None otherwise
///
/// # Example
/// ```
/// use rustycode_orchestra::debug_logger::*;
///
/// enable_debug(project_path)?;
/// let path = disable_debug();
/// assert!(path.is_some());
/// ```
pub fn disable_debug() -> Option<PathBuf> {
    let path = LOG_PATH.lock().unwrap_or_else(|e| e.into_inner()).take();
    DEBUG_ENABLED.store(false, Ordering::SeqCst);
    *START_TIME.lock().unwrap_or_else(|e| e.into_inner()) = None;
    path
}

/// Check if debug mode is active.
///
/// # Returns
/// true if debug logging is enabled
///
/// # Example
/// ```
/// use rustycode_orchestra::debug_logger::*;
///
/// assert!(!is_enabled());
/// enable_debug(project_path)?;
/// assert!(is_enabled());
/// ```
pub fn is_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::SeqCst)
}

/// Return the current log file path (or null).
///
/// # Returns
/// Log file path if logging is active, None otherwise
pub fn get_debug_log_path() -> Option<PathBuf> {
    LOG_PATH
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .cloned()
}

/// Log a structured debug event. No-op when debug is disabled.
///
/// Each event is one JSON line: `{ ts, event, ...data }`
///
/// # Arguments
/// * `event` - Event name/type
/// * `data` - Optional event data (must be serializable)
///
/// # Example
/// ```
/// use rustycode_orchestra::debug_logger::*;
///
/// enable_debug(project_path)?;
/// debug_log("test-event", &HashMap::from([
///     ("key", "value"),
///     ("number", 42),
/// ]));
/// ```
pub fn debug_log(event: &str, data: &HashMap<String, serde_json::Value>) {
    if !is_enabled() {
        return;
    }

    let log_path = match get_debug_log_path() {
        Some(path) => path,
        None => return,
    };

    // Create entry
    let mut entry = serde_json::Map::new();
    entry.insert(
        "ts".to_string(),
        serde_json::Value::String(format_iso_timestamp()),
    );
    entry.insert(
        "event".to_string(),
        serde_json::Value::String(event.to_string()),
    );

    // Add data
    for (k, v) in data.iter() {
        entry.insert(k.clone(), v.clone());
    }

    // Append to log file
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = writeln!(
            file,
            "{}",
            serde_json::to_string(&entry).unwrap_or_default()
        );
        let _ = file.flush();
    }
    // Silently ignore write failures — debug logging must never break Orchestra
}

/// Start a timer for a named operation. Returns a stop function that logs
/// the elapsed time and optional result data.
///
/// # Arguments
/// * `event` - Event name for timing
///
/// # Returns
/// Stop function that logs elapsed time when called
///
/// # Example
/// ```
/// use rustycode_orchestra::debug_logger::*;
///
/// let stop = debug_time("derive-state");
/// // ... do work ...
/// stop(&HashMap::from([("phase", "executing")]));
/// ```
type DebugTimeCallback = Box<dyn FnOnce(&HashMap<String, Value>)>;

pub fn debug_time(event: &str) -> DebugTimeCallback {
    if !is_enabled() {
        return Box::new(|_| {});
    }

    let start = SystemTime::now();
    let event = event.to_string();

    Box::new(move |data: &HashMap<String, Value>| {
        let elapsed = start.elapsed().unwrap_or_default().as_millis();
        let elapsed_ms = elapsed as u64;

        let mut entry = HashMap::new();
        for (k, v) in data.iter() {
            entry.insert(k.clone(), v.clone());
        }
        entry.insert(
            "elapsed_ms".to_string(),
            Value::Number(serde_json::Number::from(elapsed_ms)),
        );

        debug_log(&event, &entry);
    })
}

// ─── Counter Helpers ─────────────────────────────────────────────────────────

/// Increment a debug counter (used by instrumentation points).
///
/// # Arguments
/// * `counter` - Counter to increment
/// * `value` - Amount to add (default 1)
///
/// # Example
/// ```
/// use rustycode_orchestra::debug_logger::*;
///
/// enable_debug(project_path)?;
/// debug_count(DebugCounter::DeriveStateCalls, 1);
/// ```
pub fn debug_count(counter: DebugCounter, value: u64) {
    if !is_enabled() {
        return;
    }

    let mut counters = get_counters().lock().unwrap_or_else(|e| e.into_inner());
    *counters.entry(counter).or_insert(0) += value;
}

/// Record a peak value (only updates if new value is higher).
///
/// # Arguments
/// * `counter` - Counter to update
/// * `value` - Value to check
///
/// # Example
/// ```
/// use rustycode_orchestra::debug_logger::*;
///
/// enable_debug(project_path)?;
/// debug_peak(DebugCounter::TtsrPeakBuffer, 1024);
/// debug_peak(DebugCounter::TtsrPeakBuffer, 512); // Won't update
/// ```
pub fn debug_peak(counter: DebugCounter, value: u64) {
    if !is_enabled() {
        return;
    }

    let mut counters = get_counters().lock().unwrap_or_else(|e| e.into_inner());
    let entry = counters.entry(counter).or_insert(0);

    if value > *entry {
        *entry = value;
    }
}

/// Write the debug summary and disable logging. Call this when auto-mode stops.
///
/// # Returns
/// Log file path for user notification (None if debug was not enabled)
///
/// # Example
/// ```
/// use rustycode_orchestra::debug_logger::*;
///
/// enable_debug(project_path)?;
/// // ... do work ...
/// let path = write_debug_summary();
/// ```
pub fn write_debug_summary() -> Option<PathBuf> {
    if !is_enabled() {
        return None;
    }

    let _log_path = get_debug_log_path()?;
    let start_time = {
        let start_time_opt = START_TIME.lock().unwrap_or_else(|e| e.into_inner());
        match *start_time_opt {
            Some(t) => t,
            None => {
                drop(start_time_opt); // Release lock before calling disable_debug
                return disable_debug(); // No start time, just disable
            }
        }
    };
    let counters = get_counters().lock().unwrap_or_else(|e| e.into_inner());

    // Calculate totals
    let total_elapsed_ms = start_time.elapsed().unwrap_or_default().as_millis() as u64;

    let get_counter = |key: DebugCounter| -> u64 { *counters.get(&key).unwrap_or(&0) };

    let derive_state_calls = get_counter(DebugCounter::DeriveStateCalls);
    let avg_derive_state_ms = get_counter(DebugCounter::DeriveStateTotalMs)
        .checked_div(derive_state_calls)
        .unwrap_or(0);

    let ttsr_checks = get_counter(DebugCounter::TtsrChecks);
    let avg_ttsr_check_ms = get_counter(DebugCounter::TtsrTotalMs)
        .checked_div(ttsr_checks)
        .unwrap_or(0);

    let parse_roadmap_calls = get_counter(DebugCounter::ParseRoadmapCalls);
    let avg_parse_roadmap_ms = get_counter(DebugCounter::ParseRoadmapTotalMs)
        .checked_div(parse_roadmap_calls)
        .unwrap_or(0);

    // Build summary entry
    let mut summary = HashMap::new();
    summary.insert(
        "totalElapsed_ms".to_string(),
        serde_json::Value::Number(serde_json::Number::from(total_elapsed_ms)),
    );
    summary.insert(
        "dispatches".to_string(),
        serde_json::Value::Number(serde_json::Number::from(get_counter(
            DebugCounter::Dispatches,
        ))),
    );
    summary.insert(
        "deriveStateCalls".to_string(),
        serde_json::Value::Number(serde_json::Number::from(derive_state_calls)),
    );
    summary.insert(
        "avgDeriveState_ms".to_string(),
        serde_json::Value::Number(serde_json::Number::from(avg_derive_state_ms)),
    );
    summary.insert(
        "parseRoadmapCalls".to_string(),
        serde_json::Value::Number(serde_json::Number::from(parse_roadmap_calls)),
    );
    summary.insert(
        "avgParseRoadmap_ms".to_string(),
        serde_json::Value::Number(serde_json::Number::from(avg_parse_roadmap_ms)),
    );
    summary.insert(
        "parsePlanCalls".to_string(),
        serde_json::Value::Number(serde_json::Number::from(get_counter(
            DebugCounter::ParsePlanCalls,
        ))),
    );
    summary.insert(
        "ttsrChecks".to_string(),
        serde_json::Value::Number(serde_json::Number::from(ttsr_checks)),
    );
    summary.insert(
        "avgTtsrCheck_ms".to_string(),
        serde_json::Value::Number(serde_json::Number::from(avg_ttsr_check_ms)),
    );
    summary.insert(
        "ttsrPeakBuffer".to_string(),
        serde_json::Value::Number(serde_json::Number::from(get_counter(
            DebugCounter::TtsrPeakBuffer,
        ))),
    );
    summary.insert(
        "renders".to_string(),
        serde_json::Value::Number(serde_json::Number::from(get_counter(DebugCounter::Renders))),
    );

    debug_log("debug-summary", &summary);

    disable_debug()
}

// ─── Internal Helpers ───────────────────────────────────────────────────────

/// Prune old debug logs, keeping only MAX_DEBUG_LOGS most recent
fn prune_old_logs(debug_dir: &Path) -> Result<()> {
    let entries = fs::read_dir(debug_dir)?;

    let mut log_files: Vec<(String, SystemTime)> = Vec::new();

    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with("debug-") && name_str.ends_with(".log") {
            let metadata = entry.metadata()?;
            let modified = metadata.modified()?;
            log_files.push((name_str.to_string(), modified));
        }
    }

    // Sort by modification time (oldest first)
    log_files.sort_by_key(|(_, modified)| *modified);

    // Remove oldest files if we have too many
    while log_files.len() >= MAX_DEBUG_LOGS {
        if let Some((oldest, _)) = log_files.first() {
            let oldest_path = debug_dir.join(oldest);
            let _ = fs::remove_file(&oldest_path);
            log_files.remove(0);
        }
    }

    Ok(())
}

/// Format timestamp for log file name
fn format_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(now as i64, 0)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH);
    datetime.format("%Y-%m-%d-%H-%M-%S").to_string()
}

/// Format timestamp for JSON log entry
fn format_iso_timestamp() -> String {
    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let nsecs = duration.subsec_nanos();

    let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, nsecs)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH);
    datetime.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_is_enabled_initially_disabled() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        // Ensure clean state
        disable_debug();
        assert!(!is_enabled());
    }

    #[test]
    fn test_enable_debug() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        let result = enable_debug(temp_dir.path());
        assert!(result.is_ok());
        assert!(is_enabled());

        // Cleanup
        disable_debug();
    }

    #[test]
    fn test_disable_debug() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        enable_debug(temp_dir.path()).unwrap();
        let path = disable_debug();
        assert!(path.is_some());
        assert!(!is_enabled());

        // Already cleaned up by disable_debug
    }

    #[test]
    fn test_disable_debug_when_not_enabled() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        // Ensure clean state first
        disable_debug();
        let path = disable_debug();
        assert!(path.is_none());
        assert!(!is_enabled());
    }

    #[test]
    fn test_get_debug_log_path() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        assert!(get_debug_log_path().is_none());

        enable_debug(temp_dir.path()).unwrap();
        let path = get_debug_log_path();
        assert!(path.is_some());
        assert!(path.unwrap().starts_with(temp_dir.path()));

        // Cleanup
        disable_debug();
    }

    #[test]
    fn test_debug_log_when_disabled() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        // Should not panic when debug is disabled
        debug_log("test", &HashMap::new());
    }

    #[test]
    fn test_debug_time_when_disabled() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let stop = debug_time("test");
        // Should not panic when debug is disabled
        stop(&HashMap::new());
    }

    #[test]
    fn test_debug_count_when_disabled() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        // Should not panic when debug is disabled
        debug_count(DebugCounter::DeriveStateCalls, 1);
    }

    #[test]
    fn test_debug_peak_when_disabled() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        // Should not panic when debug is disabled
        debug_peak(DebugCounter::TtsrPeakBuffer, 100);
    }

    #[test]
    fn test_debug_count() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        enable_debug(temp_dir.path()).unwrap();

        debug_count(DebugCounter::DeriveStateCalls, 1);
        debug_count(DebugCounter::DeriveStateCalls, 5);

        let counters = get_counters().lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            *counters.get(&DebugCounter::DeriveStateCalls).unwrap_or(&0),
            6
        );

        // Cleanup
        disable_debug();
    }

    #[test]
    fn test_debug_peak() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        enable_debug(temp_dir.path()).unwrap();

        debug_peak(DebugCounter::TtsrPeakBuffer, 100);
        debug_peak(DebugCounter::TtsrPeakBuffer, 50);
        debug_peak(DebugCounter::TtsrPeakBuffer, 200);

        let counters = get_counters().lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            *counters.get(&DebugCounter::TtsrPeakBuffer).unwrap_or(&0),
            200
        );

        // Cleanup
        disable_debug();
    }

    #[test]
    fn test_debug_time() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        enable_debug(temp_dir.path()).unwrap();

        let stop = debug_time("test-operation");
        std::thread::sleep(std::time::Duration::from_millis(10));
        stop(&HashMap::new());

        // Verify log was written
        let log_path = get_debug_log_path().unwrap();
        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("test-operation"));
        assert!(content.contains("elapsed_ms"));

        // Cleanup
        disable_debug();
    }

    #[test]
    fn test_debug_log() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        enable_debug(temp_dir.path()).unwrap();

        let mut data = HashMap::new();
        data.insert(
            "key".to_string(),
            serde_json::Value::String("value".to_string()),
        );
        data.insert(
            "number".to_string(),
            serde_json::Value::Number(serde_json::Number::from(42)),
        );

        debug_log("test-event", &data);

        // Verify log was written
        let log_path = get_debug_log_path().unwrap();
        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("test-event"));
        assert!(content.contains("key"));
        assert!(content.contains("value"));

        // Cleanup
        disable_debug();
    }

    #[test]
    fn test_write_debug_summary() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = TempDir::new().unwrap();
        enable_debug(temp_dir.path()).unwrap();

        debug_count(DebugCounter::DeriveStateCalls, 10);
        debug_count(DebugCounter::DeriveStateTotalMs, 100);

        let path = write_debug_summary();
        assert!(path.is_some());
        assert!(!is_enabled());

        // Verify summary was written
        let log_path = path.unwrap();
        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("debug-summary"));
        assert!(content.contains("deriveStateCalls"));

        // Cleanup already done by write_debug_summary
    }

    #[test]
    fn test_prune_old_logs() {
        let temp_dir = TempDir::new().unwrap();
        let debug_dir = temp_dir.path().join(".orchestra").join("debug");
        fs::create_dir_all(&debug_dir).unwrap();

        // Create 6 old log files (more than MAX_DEBUG_LOGS)
        for i in 0..6 {
            let log_path = debug_dir.join(format!("debug-{:02}.log", i));
            fs::write(&log_path, "test").unwrap();
            // Set different modification times
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        prune_old_logs(&debug_dir).unwrap();

        // Should have at most MAX_DEBUG_LOGS files
        let remaining = fs::read_dir(&debug_dir).unwrap().count();
        assert!(remaining <= MAX_DEBUG_LOGS);
    }

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp();
        // Check format: YYYY-MM-DD-HH-MM-SS
        assert!(ts.len() == 19); // "2024-03-18-14-30-45"
        assert!(ts.chars().filter(|c| *c == '-').count() == 5);
    }

    #[test]
    fn test_format_iso_timestamp() {
        let ts = format_iso_timestamp();
        assert!(ts.contains("T"));
        assert!(ts.contains("Z"));
    }
}
