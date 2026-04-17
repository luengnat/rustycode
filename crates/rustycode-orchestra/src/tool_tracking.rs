//! Orchestra Auto Tool Tracking — In-Flight Tool Call Tracking
//!
//! Tracks which tool calls are currently executing so the idle watchdog
//! can distinguish "waiting for tool completion" from "truly idle".
//!
//! # Problem
//!
//! In autonomous development, the LLM may call tools like `bash` or
//! `write_file` that take time to complete. Without tracking, the idle
//! watchdog would incorrectly kill the unit while waiting for tools.
//!
//! # Solution
//!
//! This module tracks tool start and end times, allowing the idle watchdog
//! to see "tools are still running, don't kill yet."
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::tool_tracking::{track_tool_start, track_tool_end};
//!
//! // When tool starts
//! let tool_id = "call_123";
//! track_tool_start(tool_id, true);
//!
//! // ... tool executes ...
//!
//! // When tool completes
//! track_tool_end(tool_id);
//!
//! // Check for stuck tools
//! let oldest_ms = get_oldest_in_flight_tool_age_ms();
//! if oldest_ms > 60_000 {
//!     println!("Warning: Tool running for >60 seconds");
//! }
//! ```
//!
//! # Integration
//!
//! - **Timeout Supervisor**: Uses `get_oldest_in_flight_tool_age_ms()` to
//!   distinguish between idle and waiting-for-tool
//! - **Streaming**: Calls `track_tool_start()` on `ContentBlockStart` and
//!   `track_tool_end()` on `ContentBlockStop`
//! - **Idle Detection**: Checks `get_in_flight_tool_count()` before killing

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tracing::{debug, trace};

// ─── Types ────────────────────────────────────────────────────────────────────

/// Tool call identifier
pub type ToolCallId = String;

/// Tool tracking state
#[derive(Debug, Clone)]
pub struct ToolState {
    pub tool_call_id: ToolCallId,
    pub started_at: Instant,
}

/// Global tool tracking state
struct ToolTrackingState {
    in_flight_tools: HashMap<ToolCallId, Instant>,
}

impl ToolTrackingState {
    fn new() -> Self {
        Self {
            in_flight_tools: HashMap::new(),
        }
    }
}

// ─── Global State ─────────────────────────────────────────────────────────────

static TRACKER: OnceLock<Arc<Mutex<ToolTrackingState>>> = OnceLock::new();

fn tracker() -> Arc<Mutex<ToolTrackingState>> {
    TRACKER
        .get_or_init(|| Arc::new(Mutex::new(ToolTrackingState::new())))
        .clone()
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Mark a tool execution as in-flight
///
/// Records start time so the idle watchdog can detect tools
/// that have been running longer than the idle timeout.
///
/// # Arguments
/// * `tool_call_id` - Unique identifier for this tool call
/// * `is_active` - Whether auto-mode is active (only track if active)
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::tool_tracking::*;
///
/// mark_tool_start("tool-abc123".to_string(), true);
/// // Tool is now being tracked
/// ```
pub fn mark_tool_start(tool_call_id: ToolCallId, is_active: bool) {
    if !is_active {
        return;
    }

    let binding = tracker();
    let mut tracker = binding.lock().unwrap_or_else(|e| e.into_inner());
    tracker
        .in_flight_tools
        .insert(tool_call_id.clone(), Instant::now());

    trace!(
        "Tool started: {} (total in-flight: {})",
        tool_call_id,
        tracker.in_flight_tools.len()
    );
}

/// Mark a tool execution as completed
///
/// # Arguments
/// * `tool_call_id` - Unique identifier for this tool call
///
/// # Example
/// ```rust,no_run
/// mark_tool_end("tool-abc123".to_string());
/// // Tool is no longer being tracked
/// ```
pub fn mark_tool_end(tool_call_id: ToolCallId) {
    let binding = tracker();
    let mut tracker = binding.lock().unwrap_or_else(|e| e.into_inner());
    tracker.in_flight_tools.remove(&tool_call_id);

    trace!(
        "Tool completed: {} (total in-flight: {})",
        tool_call_id,
        tracker.in_flight_tools.len()
    );
}

/// Returns the age (ms) of the oldest currently in-flight tool, or 0 if none
///
/// # Returns
/// Age in milliseconds of the oldest in-flight tool
///
/// # Example
/// ```rust,no_run
/// let age_ms = get_oldest_in_flight_tool_age_ms();
/// if age_ms > 300_000 {
///     println!("Oldest tool has been running for {}ms", age_ms);
/// }
/// ```
pub fn get_oldest_in_flight_tool_age_ms() -> u64 {
    let binding = tracker();
    let tracker = binding.lock().unwrap_or_else(|e| e.into_inner());

    if tracker.in_flight_tools.is_empty() {
        return 0;
    }

    let oldest_start = tracker.in_flight_tools.values().min().unwrap();

    oldest_start.elapsed().as_millis() as u64
}

/// Returns the number of currently in-flight tools
///
/// # Returns
/// Count of tools currently being executed
///
/// # Example
/// ```rust,no_run
/// let count = get_in_flight_tool_count();
/// println!("{} tools currently executing", count);
/// ```
pub fn get_in_flight_tool_count() -> usize {
    let binding = tracker();
    let tracker = binding.lock().unwrap_or_else(|e| e.into_inner());
    tracker.in_flight_tools.len()
}

/// Returns the start timestamp of the oldest in-flight tool, or None if none
///
/// # Returns
/// Optional Instant representing when the oldest tool started
///
/// # Example
/// ```rust,no_run
/// if let Some(started) = get_oldest_in_flight_tool_start() {
///     let age = started.elapsed();
///     println!("Oldest tool started {:?} ago", age);
/// }
/// ```
pub fn get_oldest_in_flight_tool_start() -> Option<Instant> {
    let binding = tracker();
    let tracker = binding.lock().unwrap_or_else(|e| e.into_inner());

    if tracker.in_flight_tools.is_empty() {
        return None;
    }

    tracker.in_flight_tools.values().min().copied()
}

/// Clear all in-flight tool tracking state
///
/// Useful for cleanup or when starting a new session
///
/// # Example
/// ```rust,no_run
/// clear_in_flight_tools();
/// assert_eq!(get_in_flight_tool_count(), 0);
/// ```
pub fn clear_in_flight_tools() {
    let binding = tracker();
    let mut tracker = binding.lock().unwrap_or_else(|e| e.into_inner());
    let count = tracker.in_flight_tools.len();
    tracker.in_flight_tools.clear();

    debug!("Cleared {} in-flight tools", count);
}

/// Get all currently in-flight tool states
///
/// # Returns
/// Vector of tool states (for debugging/monitoring)
///
/// # Example
/// ```rust,no_run
/// let tools = get_in_flight_tools();
/// for tool in tools {
///     println!("Tool {} running for {:?}", tool.tool_call_id, tool.started_at.elapsed());
/// }
/// ```
pub fn get_in_flight_tools() -> Vec<ToolState> {
    let binding = tracker();
    let tracker = binding.lock().unwrap_or_else(|e| e.into_inner());

    tracker
        .in_flight_tools
        .iter()
        .map(|(id, started)| ToolState {
            tool_call_id: id.clone(),
            started_at: *started,
        })
        .collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_mark_tool_start_end() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        // Use unique tool ID to avoid conflicts with parallel tests
        let tool_id = "tool_test_mark_tool_start_end";
        clear_in_flight_tools();

        mark_tool_start(tool_id.to_string(), true);
        assert_eq!(get_in_flight_tool_count(), 1);

        mark_tool_end(tool_id.to_string());
        assert_eq!(get_in_flight_tool_count(), 0);
    }

    #[test]
    fn test_mark_tool_start_inactive() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tool_id = "tool_test_mark_tool_start_inactive";
        clear_in_flight_tools();

        mark_tool_start(tool_id.to_string(), false);
        assert_eq!(get_in_flight_tool_count(), 0);
    }

    #[test]
    fn test_multiple_tools() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tool_id = "tool_test_multiple";
        clear_in_flight_tools();

        mark_tool_start(format!("{}_1", tool_id), true);
        mark_tool_start(format!("{}_2", tool_id), true);
        mark_tool_start(format!("{}_3", tool_id), true);

        assert_eq!(get_in_flight_tool_count(), 3);

        mark_tool_end(format!("{}_2", tool_id));
        assert_eq!(get_in_flight_tool_count(), 2);

        clear_in_flight_tools();
    }

    #[test]
    fn test_oldest_tool_age() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tool_id = "tool_test_oldest_age";
        clear_in_flight_tools();

        mark_tool_start(tool_id.to_string(), true);
        std::thread::sleep(std::time::Duration::from_millis(10));

        let age = get_oldest_in_flight_tool_age_ms();
        assert!(age >= 10);

        clear_in_flight_tools();
    }

    #[test]
    fn test_oldest_tool_age_empty() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        clear_in_flight_tools();

        let age = get_oldest_in_flight_tool_age_ms();
        assert_eq!(age, 0);
    }

    #[test]
    fn test_oldest_tool_start() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tool_id = "tool_test_oldest_start";
        clear_in_flight_tools();

        mark_tool_start(tool_id.to_string(), true);

        let oldest = get_oldest_in_flight_tool_start();
        assert!(oldest.is_some());

        let elapsed = oldest.unwrap().elapsed();
        assert!(elapsed.as_millis() < 100);

        clear_in_flight_tools();
    }

    #[test]
    fn test_oldest_tool_start_empty() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        clear_in_flight_tools();

        let oldest = get_oldest_in_flight_tool_start();
        assert!(oldest.is_none());
    }

    #[test]
    fn test_clear_in_flight_tools() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tool_id = "tool_test_clear";
        clear_in_flight_tools();

        mark_tool_start(format!("{}_1", tool_id), true);
        mark_tool_start(format!("{}_2", tool_id), true);

        assert_eq!(get_in_flight_tool_count(), 2);

        clear_in_flight_tools();

        assert_eq!(get_in_flight_tool_count(), 0);
    }

    #[test]
    fn test_get_in_flight_tools() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tool_id = "tool_test_get";
        clear_in_flight_tools();

        mark_tool_start(format!("{}_1", tool_id), true);
        mark_tool_start(format!("{}_2", tool_id), true);

        let tools = get_in_flight_tools();
        assert_eq!(tools.len(), 2);

        // Sort for consistent ordering (HashMap doesn't guarantee order)
        let mut tool_ids: Vec<_> = tools.iter().map(|t| &t.tool_call_id).collect();
        tool_ids.sort();

        assert!(tool_ids[0].contains("1"));
        assert!(tool_ids[1].contains("2"));

        clear_in_flight_tools();
    }

    #[test]
    fn test_concurrent_access() {
        let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tool_id = "tool_test_concurrent";
        clear_in_flight_tools();

        // Simulate concurrent access
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let id = format!("{}_{}", tool_id, i);
                std::thread::spawn(move || {
                    mark_tool_start(id.clone(), true);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    mark_tool_end(id);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // All tools should be completed
        assert_eq!(get_in_flight_tool_count(), 0);
    }
}
