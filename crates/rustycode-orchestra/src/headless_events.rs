//! Headless event detection — notification classification and command detection.
//!
//! Detects terminal notifications, blocked notifications, milestone-ready signals,
//! and classifies commands as quick (single-turn) vs long-running.
//!
//! Matches orchestra-2's headless-events.ts implementation.

use std::collections::HashSet;
use std::time::Duration;

/// Prefixes that indicate genuine auto-mode termination
pub const TERMINAL_PREFIXES: &[&str] = &["auto-mode stopped", "step-mode stopped"];

/// Default idle timeout for auto-mode (15 seconds)
pub const IDLE_TIMEOUT_MS: u64 = 15_000;

/// Idle timeout for new-milestone (120 seconds)
///
/// new-milestone is a long-running creative task where the LLM may pause
/// between tool calls (e.g. after mkdir, before writing files). Use a
/// longer idle timeout to avoid killing the session prematurely (#808).
pub const NEW_MILESTONE_IDLE_TIMEOUT_MS: u64 = 120_000;

/// Methods that don't require waiting for a response
pub const FIRE_AND_FORGET_METHODS: &[&str] = &[
    "notify",
    "setStatus",
    "setWidget",
    "setTitle",
    "set_editor_text",
];

/// Commands that complete quickly (single-turn)
pub const QUICK_COMMANDS: &[&str] = &[
    "status",
    "queue",
    "history",
    "hooks",
    "export",
    "stop",
    "pause",
    "capture",
    "skip",
    "undo",
    "knowledge",
    "config",
    "prefs",
    "cleanup",
    "migrate",
    "doctor",
    "remote",
    "help",
    "steer",
    "triage",
    "visualize",
];

/// Event structure for headless events
#[derive(Debug, Clone, Default)]
pub struct HeadlessEvent {
    /// Event type (e.g., "extension_ui_request")
    pub event_type: Option<String>,
    /// Event method (e.g., "notify")
    pub method: Option<String>,
    /// Event message
    pub message: Option<String>,
    /// Additional event data
    pub data: serde_json::Value,
}

impl HeadlessEvent {
    /// Create a new empty event
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an event with a message
    pub fn with_message(message: impl Into<String>) -> Self {
        Self {
            event_type: Some("extension_ui_request".to_string()),
            method: Some("notify".to_string()),
            message: Some(message.into()),
            ..Self::default()
        }
    }

    /// Check if this is a terminal notification
    pub fn is_terminal(&self) -> bool {
        is_terminal_notification(self)
    }

    /// Check if this is a blocked notification
    pub fn is_blocked(&self) -> bool {
        is_blocked_notification(self)
    }

    /// Check if this is a milestone-ready notification
    pub fn is_milestone_ready(&self) -> bool {
        is_milestone_ready_notification(self)
    }
}

/// Detect genuine auto-mode termination notifications.
///
/// Only matches the actual stop signals emitted by stopAuto():
///   "Auto-mode stopped..."
///   "Step-mode stopped..."
///
/// Does NOT match progress notifications that happen to contain words like
/// "complete" or "stopped" (e.g., "Override resolved — rewrite-docs completed",
/// "All slices are complete — nothing to discuss", "Skipped 5+ completed units").
///
/// Blocked detection is separate — checked via is_blocked_notification.
///
/// # Arguments
/// * `event` - Event to check
///
/// # Returns
/// true if the event is a terminal notification
///
/// # Examples
/// ```
/// use rustycode_orchestra::headless_events::{HeadlessEvent, is_terminal_notification};
///
/// let event = HeadlessEvent::with_message("Auto-mode stopped: All tasks complete");
/// assert!(is_terminal_notification(&event));
///
/// let event = HeadlessEvent::with_message("Override resolved — rewrite-docs completed");
/// assert!(!is_terminal_notification(&event));
/// ```
pub fn is_terminal_notification(event: &HeadlessEvent) -> bool {
    if event.event_type.as_deref() != Some("extension_ui_request") {
        return false;
    }
    if event.method.as_deref() != Some("notify") {
        return false;
    }

    let message = event.message.as_deref().unwrap_or("").to_lowercase();
    TERMINAL_PREFIXES
        .iter()
        .any(|prefix| message.starts_with(prefix))
}

/// Detect blocked notifications.
///
/// Blocked notifications come through stopAuto as "Auto-mode stopped (Blocked: ...)"
///
/// # Arguments
/// * `event` - Event to check
///
/// # Returns
/// true if the event contains a blocked notification
///
/// # Examples
/// ```
/// use rustycode_orchestra::headless_events::{HeadlessEvent, is_blocked_notification};
///
/// let event = HeadlessEvent::with_message("Auto-mode stopped (Blocked: waiting for user input)");
/// assert!(is_blocked_notification(&event));
///
/// let event = HeadlessEvent::with_message("Auto-mode stopped: All tasks complete");
/// assert!(!is_blocked_notification(&event));
/// ```
pub fn is_blocked_notification(event: &HeadlessEvent) -> bool {
    if event.event_type.as_deref() != Some("extension_ui_request") {
        return false;
    }
    if event.method.as_deref() != Some("notify") {
        return false;
    }

    let message = event.message.as_deref().unwrap_or("").to_lowercase();
    message.contains("blocked:")
}

/// Detect milestone-ready notifications.
///
/// Matches messages like "milestone M1 ready" or "milestone M2.3 ready"
///
/// # Arguments
/// * `event` - Event to check
///
/// # Returns
/// true if the event indicates a milestone is ready
///
/// # Examples
/// ```
/// use rustycode_orchestra::headless_events::{HeadlessEvent, is_milestone_ready_notification};
///
/// let event = HeadlessEvent::with_message("Milestone M1 ready for execution");
/// assert!(is_milestone_ready_notification(&event));
///
/// let event = HeadlessEvent::with_message("Milestone M2.3 is ready");
/// assert!(is_milestone_ready_notification(&event));
///
/// let event = HeadlessEvent::with_message("Task completed");
/// assert!(!is_milestone_ready_notification(&event));
/// ```
pub fn is_milestone_ready_notification(event: &HeadlessEvent) -> bool {
    if event.event_type.as_deref() != Some("extension_ui_request") {
        return false;
    }
    if event.method.as_deref() != Some("notify") {
        return false;
    }

    let message = event.message.as_deref().unwrap_or("");
    // Regex for "milestone M<digits>" (case-insensitive)
    message.to_lowercase().contains("milestone m") && message.to_lowercase().contains("ready")
}

/// Check if a command is a quick (single-turn) command.
///
/// Quick commands complete immediately without requiring multi-turn execution.
///
/// # Arguments
/// * `command` - Command name to check
///
/// # Returns
/// true if the command is a quick command
///
/// # Examples
/// ```
/// use rustycode_orchestra::headless_events::is_quick_command;
///
/// assert!(is_quick_command("status"));
/// assert!(is_quick_command("help"));
/// assert!(is_quick_command("doctor"));
/// assert!(!is_quick_command("auto"));  // auto is long-running
/// assert!(!is_quick_command("new-milestone"));  // new-milestone is long-running
/// ```
pub fn is_quick_command(command: &str) -> bool {
    QUICK_COMMANDS.contains(&command)
}

/// Check if a method is fire-and-forget (doesn't require waiting for response).
///
/// # Arguments
/// * `method` - Method name to check
///
/// # Returns
/// true if the method is fire-and-forget
///
/// # Examples
/// ```
/// use rustycode_orchestra::headless_events::is_fire_and_forget_method;
///
/// assert!(is_fire_and_forget_method("notify"));
/// assert!(is_fire_and_forget_method("setStatus"));
/// assert!(!is_fire_and_forget_method("execute"));  // execute requires response
/// ```
pub fn is_fire_and_forget_method(method: &str) -> bool {
    FIRE_AND_FORGET_METHODS.contains(&method)
}

/// Get the default idle timeout duration
///
/// # Returns
/// Duration for IDLE_TIMEOUT_MS
///
/// # Examples
/// ```
/// use rustycode_orchestra::headless_events::idle_timeout;
/// use std::time::Duration;
///
/// let timeout = idle_timeout();
/// assert_eq!(timeout, Duration::from_millis(15_000));
/// ```
pub fn idle_timeout() -> Duration {
    Duration::from_millis(IDLE_TIMEOUT_MS)
}

/// Get the new-milestone idle timeout duration
///
/// # Returns
/// Duration for NEW_MILESTONE_IDLE_TIMEOUT_MS
///
/// # Examples
/// ```
/// use rustycode_orchestra::headless_events::new_milestone_idle_timeout;
/// use std::time::Duration;
///
/// let timeout = new_milestone_idle_timeout();
/// assert_eq!(timeout, Duration::from_millis(120_000));
/// ```
pub fn new_milestone_idle_timeout() -> Duration {
    Duration::from_millis(NEW_MILESTONE_IDLE_TIMEOUT_MS)
}

/// Get a HashSet of all quick commands
///
/// # Returns
/// HashSet containing all quick command names
///
/// # Examples
/// ```
/// use rustycode_orchestra::headless_events::get_quick_commands_set;
///
/// let quick_cmds = get_quick_commands_set();
/// assert!(quick_cmds.contains("status"));
/// assert!(quick_cmds.contains("help"));
/// assert!(!quick_cmds.contains("auto"));
/// ```
pub fn get_quick_commands_set() -> HashSet<&'static str> {
    QUICK_COMMANDS.iter().copied().collect()
}

/// Get a HashSet of all fire-and-forget methods
///
/// # Returns
/// HashSet containing all fire-and-forget method names
///
/// # Examples
/// ```
/// use rustycode_orchestra::headless_events::get_fire_and_forget_methods_set;
///
/// let methods = get_fire_and_forget_methods_set();
/// assert!(methods.contains("notify"));
/// assert!(methods.contains("setStatus"));
/// assert!(!methods.contains("execute"));
/// ```
pub fn get_fire_and_forget_methods_set() -> HashSet<&'static str> {
    FIRE_AND_FORGET_METHODS.iter().copied().collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(message: &str) -> HeadlessEvent {
        HeadlessEvent {
            event_type: Some("extension_ui_request".to_string()),
            method: Some("notify".to_string()),
            message: Some(message.to_string()),
            data: serde_json::json!({}),
        }
    }

    #[test]
    fn test_is_terminal_notification_auto_mode_stopped() {
        let event = make_event("Auto-mode stopped: All tasks complete");
        assert!(is_terminal_notification(&event));
    }

    #[test]
    fn test_is_terminal_notification_step_mode_stopped() {
        let event = HeadlessEvent::with_message("Step-mode stopped: Paused by user");
        assert!(is_terminal_notification(&event));
    }

    #[test]
    fn test_is_terminal_notification_case_insensitive() {
        let event = HeadlessEvent::with_message("AUTO-MODE STOPPED: Complete");
        assert!(is_terminal_notification(&event));
    }

    #[test]
    fn test_is_terminal_notification_false_for_progress() {
        let event = HeadlessEvent::with_message("Override resolved — rewrite-docs completed");
        assert!(!is_terminal_notification(&event));
    }

    #[test]
    fn test_is_terminal_notification_false_for_other_messages() {
        let event = HeadlessEvent::with_message("Task completed successfully");
        assert!(!is_terminal_notification(&event));
    }

    #[test]
    fn test_is_terminal_notification_requires_correct_type() {
        let mut event = HeadlessEvent::with_message("Auto-mode stopped");
        event.event_type = Some("other_type".to_string());
        assert!(!is_terminal_notification(&event));
    }

    #[test]
    fn test_is_terminal_notification_requires_correct_method() {
        let mut event = HeadlessEvent::with_message("Auto-mode stopped");
        event.method = Some("other_method".to_string());
        assert!(!is_terminal_notification(&event));
    }

    #[test]
    fn test_is_blocked_notification_with_blocked() {
        let event =
            HeadlessEvent::with_message("Auto-mode stopped (Blocked: waiting for user input)");
        assert!(is_blocked_notification(&event));
    }

    #[test]
    fn test_is_blocked_notification_case_insensitive() {
        let event = HeadlessEvent::with_message("AUTO-MODE STOPPED (BLOCKED: need input)");
        assert!(is_blocked_notification(&event));
    }

    #[test]
    fn test_is_blocked_notification_false_without_blocked() {
        let event = make_event("Auto-mode stopped: All tasks complete");
        assert!(!is_blocked_notification(&event));
    }

    #[test]
    fn test_is_blocked_notification_requires_correct_type() {
        let mut event = HeadlessEvent::with_message("Auto-mode stopped (Blocked: test)");
        event.event_type = Some("other_type".to_string());
        assert!(!is_blocked_notification(&event));
    }

    #[test]
    fn test_is_milestone_ready_notification_basic() {
        let event = HeadlessEvent::with_message("Milestone M1 ready for execution");
        assert!(is_milestone_ready_notification(&event));
    }

    #[test]
    fn test_is_milestone_ready_notification_with_decimal() {
        let event = HeadlessEvent::with_message("Milestone M2.3 is ready");
        assert!(is_milestone_ready_notification(&event));
    }

    #[test]
    fn test_is_milestone_ready_notification_case_insensitive() {
        let event = HeadlessEvent::with_message("MILESTONE M1 READY");
        assert!(is_milestone_ready_notification(&event));
    }

    #[test]
    fn test_is_milestone_ready_notification_false_without_milestone() {
        let event = HeadlessEvent::with_message("Task completed successfully");
        assert!(!is_milestone_ready_notification(&event));
    }

    #[test]
    fn test_is_milestone_ready_notification_false_without_ready() {
        let event = HeadlessEvent::with_message("Milestone M1 created");
        assert!(!is_milestone_ready_notification(&event));
    }

    #[test]
    fn test_is_quick_command_known_quick_commands() {
        assert!(is_quick_command("status"));
        assert!(is_quick_command("queue"));
        assert!(is_quick_command("history"));
        assert!(is_quick_command("help"));
        assert!(is_quick_command("doctor"));
        assert!(is_quick_command("export"));
    }

    #[test]
    fn test_is_quick_command_false_for_long_running() {
        assert!(!is_quick_command("auto"));
        assert!(!is_quick_command("new-milestone"));
        assert!(!is_quick_command("execute"));
    }

    #[test]
    fn test_is_fire_and_forget_method_known_methods() {
        assert!(is_fire_and_forget_method("notify"));
        assert!(is_fire_and_forget_method("setStatus"));
        assert!(is_fire_and_forget_method("setWidget"));
        assert!(is_fire_and_forget_method("setTitle"));
        assert!(is_fire_and_forget_method("set_editor_text"));
    }

    #[test]
    fn test_is_fire_and_forget_method_false_for_others() {
        assert!(!is_fire_and_forget_method("execute"));
        assert!(!is_fire_and_forget_method("complete"));
        assert!(!is_fire_and_forget_method("query"));
    }

    #[test]
    fn test_idle_timeout_duration() {
        let timeout = idle_timeout();
        assert_eq!(timeout, Duration::from_millis(15_000));
    }

    #[test]
    fn test_new_milestone_idle_timeout_duration() {
        let timeout = new_milestone_idle_timeout();
        assert_eq!(timeout, Duration::from_millis(120_000));
    }

    #[test]
    fn test_new_milestone_timeout_longer_than_default() {
        let default_timeout = idle_timeout();
        let milestone_timeout = new_milestone_idle_timeout();
        assert!(milestone_timeout > default_timeout);
    }

    #[test]
    fn test_get_quick_commands_set_contains_all() {
        let quick_cmds = get_quick_commands_set();
        assert_eq!(quick_cmds.len(), QUICK_COMMANDS.len());
        assert!(quick_cmds.contains("status"));
        assert!(quick_cmds.contains("help"));
        assert!(quick_cmds.contains("doctor"));
    }

    #[test]
    fn test_get_fire_and_forget_methods_set_contains_all() {
        let methods = get_fire_and_forget_methods_set();
        assert_eq!(methods.len(), FIRE_AND_FORGET_METHODS.len());
        assert!(methods.contains("notify"));
        assert!(methods.contains("setStatus"));
    }

    #[test]
    fn test_headless_event_default() {
        let event = HeadlessEvent::new();
        assert!(event.event_type.is_none());
        assert!(event.method.is_none());
        assert!(event.message.is_none());
    }

    #[test]
    fn test_headless_event_with_message() {
        let event = HeadlessEvent::with_message("Test message");
        assert_eq!(event.message, Some("Test message".to_string()));
    }

    #[test]
    fn test_headless_event_convenience_methods() {
        let event = HeadlessEvent::with_message("Auto-mode stopped");
        assert!(event.is_terminal());
        assert!(!event.is_blocked());

        let event = HeadlessEvent::with_message("Auto-mode stopped (Blocked: test)");
        assert!(event.is_blocked());
    }

    #[test]
    fn test_terminal_prefixes_constant() {
        assert_eq!(TERMINAL_PREFIXES.len(), 2);
        assert!(TERMINAL_PREFIXES.contains(&"auto-mode stopped"));
        assert!(TERMINAL_PREFIXES.contains(&"step-mode stopped"));
    }

    #[test]
    fn test_quick_commands_constant() {
        assert!(QUICK_COMMANDS.contains(&"status"));
        assert!(QUICK_COMMANDS.contains(&"help"));
        assert!(QUICK_COMMANDS.contains(&"doctor"));
        assert!(!QUICK_COMMANDS.contains(&"auto"));
    }

    #[test]
    fn test_fire_and_forget_methods_constant() {
        assert!(FIRE_AND_FORGET_METHODS.contains(&"notify"));
        assert!(FIRE_AND_FORGET_METHODS.contains(&"setStatus"));
        assert!(!FIRE_AND_FORGET_METHODS.contains(&"execute"));
    }

    #[test]
    fn test_timeout_constants() {
        assert_eq!(IDLE_TIMEOUT_MS, 15_000);
        assert_eq!(NEW_MILESTONE_IDLE_TIMEOUT_MS, 120_000);
    }
}
