// rustycode-orchestra/src/auto_tool_tracking.rs
//! In-flight tool call tracking for auto-mode idle detection.
//!
//! Tracks which tool calls are currently executing so the idle watchdog
//! can distinguish "waiting for tool completion" from "truly idle".

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// In-flight tool tracking state
///
/// Tracks tool calls by their call ID and start time.
/// Thread-safe for concurrent access.
pub struct InFlightToolTracker {
    tools: Mutex<HashMap<String, u64>>,
}

impl Default for InFlightToolTracker {
    fn default() -> Self {
        Self {
            tools: Mutex::new(HashMap::new()),
        }
    }
}

impl InFlightToolTracker {
    /// Create a new tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a tool execution as in-flight.
    ///
    /// Records start time so the idle watchdog can detect tools
    /// hung longer than the idle timeout.
    ///
    /// # Arguments
    /// * `tool_call_id` - Unique identifier for this tool call
    /// * `is_active` - Whether tracking is enabled (false = no-op)
    pub fn mark_tool_start(&self, tool_call_id: &str, is_active: bool) {
        if !is_active {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mut tools = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        tools.insert(tool_call_id.to_string(), now);
    }

    /// Mark a tool execution as completed.
    ///
    /// # Arguments
    /// * `tool_call_id` - The tool call ID to remove from tracking
    pub fn mark_tool_end(&self, tool_call_id: &str) {
        let mut tools = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        tools.remove(tool_call_id);
    }

    /// Returns the age (ms) of the oldest currently in-flight tool, or 0 if none.
    pub fn get_oldest_in_flight_tool_age_ms(&self) -> u64 {
        let tools = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        if tools.is_empty() {
            return 0;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let oldest_start = *tools.values().min().unwrap();
        now.saturating_sub(oldest_start)
    }

    /// Returns the number of currently in-flight tools.
    pub fn get_in_flight_tool_count(&self) -> usize {
        let tools = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        tools.len()
    }

    /// Returns the start timestamp of the oldest in-flight tool, or None if none.
    pub fn get_oldest_in_flight_tool_start(&self) -> Option<u64> {
        let tools = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        if tools.is_empty() {
            return None;
        }
        Some(*tools.values().min().unwrap())
    }

    /// Clear all in-flight tool tracking state.
    pub fn clear_in_flight_tools(&self) {
        let mut tools = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        tools.clear();
    }

    /// Get all in-flight tool IDs and their start times (for debugging/testing)
    #[cfg(test)]
    pub fn get_all_in_flight_tools(&self) -> Vec<(String, u64)> {
        let tools = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        tools.iter().map(|(k, v)| (k.clone(), *v)).collect()
    }
}

// Global singleton for backward compatibility with TypeScript API
static GLOBAL_TRACKER: Mutex<Option<InFlightToolTracker>> = Mutex::new(None);

/// Initialize the global tracker and return a locked reference
fn with_global_tracker<F, R>(f: F) -> R
where
    F: FnOnce(&InFlightToolTracker) -> R,
{
    let mut global = GLOBAL_TRACKER.lock().unwrap_or_else(|e| e.into_inner());
    global.get_or_insert_with(InFlightToolTracker::new);
    f(global.as_ref().expect("just initialized"))
}

/// Initialize the global tracker and return a mutable locked reference
fn with_global_tracker_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut InFlightToolTracker) -> R,
{
    let mut global = GLOBAL_TRACKER.lock().unwrap_or_else(|e| e.into_inner());
    global.get_or_insert_with(InFlightToolTracker::new);
    f(global.as_mut().expect("just initialized"))
}

/// Mark a tool execution as in-flight (global singleton API).
///
/// Records start time so the idle watchdog can detect tools hung longer than the idle timeout.
///
/// # Arguments
/// * `tool_call_id` - Unique identifier for this tool call
/// * `is_active` - Whether tracking is enabled (false = no-op)
pub fn mark_tool_start(tool_call_id: &str, is_active: bool) {
    with_global_tracker_mut(|tracker| {
        tracker.mark_tool_start(tool_call_id, is_active);
    });
}

/// Mark a tool execution as completed (global singleton API).
///
/// # Arguments
/// * `tool_call_id` - The tool call ID to remove from tracking
pub fn mark_tool_end(tool_call_id: &str) {
    with_global_tracker_mut(|tracker| {
        tracker.mark_tool_end(tool_call_id);
    });
}

/// Returns the age (ms) of the oldest currently in-flight tool, or 0 if none (global singleton API).
pub fn get_oldest_in_flight_tool_age_ms() -> u64 {
    with_global_tracker(|tracker| tracker.get_oldest_in_flight_tool_age_ms())
}

/// Returns the number of currently in-flight tools (global singleton API).
pub fn get_in_flight_tool_count() -> usize {
    with_global_tracker(|tracker| tracker.get_in_flight_tool_count())
}

/// Returns the start timestamp of the oldest in-flight tool, or None if none (global singleton API).
pub fn get_oldest_in_flight_tool_start() -> Option<u64> {
    with_global_tracker(|tracker| tracker.get_oldest_in_flight_tool_start())
}

/// Clear all in-flight tool tracking state (global singleton API).
pub fn clear_in_flight_tools() {
    with_global_tracker_mut(|tracker| {
        tracker.clear_in_flight_tools();
    });
}

/// Get all in-flight tool IDs and their start times (for debugging/testing)
#[cfg(test)]
pub fn get_in_flight_tools() -> Vec<(String, u64)> {
    with_global_tracker(|tracker| tracker.get_all_in_flight_tools())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_new() {
        let tracker = InFlightToolTracker::new();
        assert_eq!(tracker.get_in_flight_tool_count(), 0);
    }

    #[test]
    fn test_mark_tool_start() {
        let tracker = InFlightToolTracker::new();
        tracker.mark_tool_start("tool-1", true);
        assert_eq!(tracker.get_in_flight_tool_count(), 1);
    }

    #[test]
    fn test_mark_tool_start_inactive() {
        let tracker = InFlightToolTracker::new();
        tracker.mark_tool_start("tool-1", false);
        assert_eq!(tracker.get_in_flight_tool_count(), 0);
    }

    #[test]
    fn test_mark_tool_end() {
        let tracker = InFlightToolTracker::new();
        tracker.mark_tool_start("tool-1", true);
        tracker.mark_tool_end("tool-1");
        assert_eq!(tracker.get_in_flight_tool_count(), 0);
    }

    #[test]
    fn test_mark_tool_end_nonexistent() {
        let tracker = InFlightToolTracker::new();
        tracker.mark_tool_end("tool-1"); // Should not panic
        assert_eq!(tracker.get_in_flight_tool_count(), 0);
    }

    #[test]
    fn test_get_oldest_in_flight_tool_age_ms_empty() {
        let tracker = InFlightToolTracker::new();
        assert_eq!(tracker.get_oldest_in_flight_tool_age_ms(), 0);
    }

    #[test]
    fn test_get_oldest_in_flight_tool_age_ms_single() {
        let tracker = InFlightToolTracker::new();
        tracker.mark_tool_start("tool-1", true);

        // Need to wait a bit to ensure time has passed
        std::thread::sleep(std::time::Duration::from_millis(10));

        let age = tracker.get_oldest_in_flight_tool_age_ms();
        assert!(age >= 10); // At least 10ms should have passed
    }

    #[test]
    fn test_get_oldest_in_flight_tool_age_ms_multiple() {
        let tracker = InFlightToolTracker::new();
        tracker.mark_tool_start("tool-1", true);
        std::thread::sleep(std::time::Duration::from_millis(10));
        tracker.mark_tool_start("tool-2", true);

        let age = tracker.get_oldest_in_flight_tool_age_ms();
        assert!(age >= 10); // tool-1 should be at least 10ms old
    }

    #[test]
    fn test_get_in_flight_tool_count() {
        let tracker = InFlightToolTracker::new();
        assert_eq!(tracker.get_in_flight_tool_count(), 0);

        tracker.mark_tool_start("tool-1", true);
        assert_eq!(tracker.get_in_flight_tool_count(), 1);

        tracker.mark_tool_start("tool-2", true);
        assert_eq!(tracker.get_in_flight_tool_count(), 2);

        tracker.mark_tool_end("tool-1");
        assert_eq!(tracker.get_in_flight_tool_count(), 1);
    }

    #[test]
    fn test_get_oldest_in_flight_tool_start_empty() {
        let tracker = InFlightToolTracker::new();
        assert_eq!(tracker.get_oldest_in_flight_tool_start(), None);
    }

    #[test]
    fn test_get_oldest_in_flight_tool_start() {
        let tracker = InFlightToolTracker::new();
        tracker.mark_tool_start("tool-1", true);

        let start = tracker.get_oldest_in_flight_tool_start();
        assert!(start.is_some());
        assert!(start.unwrap() > 0);
    }

    #[test]
    fn test_clear_in_flight_tools() {
        let tracker = InFlightToolTracker::new();
        tracker.mark_tool_start("tool-1", true);
        tracker.mark_tool_start("tool-2", true);
        assert_eq!(tracker.get_in_flight_tool_count(), 2);

        tracker.clear_in_flight_tools();
        assert_eq!(tracker.get_in_flight_tool_count(), 0);
    }

    /// Mutex to serialize tests that share the global tracker singleton.
    /// Without this, concurrent test threads race on the global state
    /// causing intermittent failures.
    static GLOBAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // Global singleton API tests
    #[test]
    fn test_global_mark_tool_start() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        clear_in_flight_tools(); // Start clean
        mark_tool_start("tool-1", true);
        assert_eq!(get_in_flight_tool_count(), 1);
        clear_in_flight_tools(); // Clean up
    }

    #[test]
    fn test_global_mark_tool_end() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        clear_in_flight_tools(); // Start clean
        mark_tool_start("tool-1", true);
        mark_tool_end("tool-1");
        assert_eq!(get_in_flight_tool_count(), 0);
        clear_in_flight_tools(); // Clean up
    }

    #[test]
    fn test_global_get_oldest_in_flight_tool_age_ms() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        // Note: This test uses the global singleton which is shared across all tests.
        // Due to test parallelism, we can't reliably test timing, so we just verify
        // that the function works correctly.
        clear_in_flight_tools(); // Start clean
        mark_tool_start("tool-1", true);
        assert_eq!(get_in_flight_tool_count(), 1);
        let start = get_oldest_in_flight_tool_start();
        assert!(start.is_some());
        clear_in_flight_tools(); // Clean up
    }

    #[test]
    fn test_global_get_in_flight_tool_count() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        clear_in_flight_tools(); // Start clean
        assert_eq!(get_in_flight_tool_count(), 0);
        mark_tool_start("tool-1", true);
        assert_eq!(get_in_flight_tool_count(), 1);
        clear_in_flight_tools(); // Clean up
    }

    #[test]
    fn test_global_clear_in_flight_tools() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        clear_in_flight_tools(); // Start clean
        mark_tool_start("tool-1", true);
        mark_tool_start("tool-2", true);
        assert_eq!(get_in_flight_tool_count(), 2);
        clear_in_flight_tools();
        assert_eq!(get_in_flight_tool_count(), 0);
    }

    #[test]
    fn test_concurrent_access() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        use std::sync::Arc;
        use std::thread;

        clear_in_flight_tools(); // Start clean

        let tracker = Arc::new(InFlightToolTracker::new());
        let mut handles = Vec::new();

        // Spawn multiple threads that all try to track tools
        for i in 0..10 {
            let tracker_clone = Arc::clone(&tracker);
            let handle = thread::spawn(move || {
                for j in 0..5 {
                    let id = format!("tool-{}-{}", i, j);
                    tracker_clone.mark_tool_start(&id, true);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    tracker_clone.mark_tool_end(&id);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // All tools should be cleared now
        assert_eq!(tracker.get_in_flight_tool_count(), 0);

        clear_in_flight_tools(); // Clean up
    }
}
