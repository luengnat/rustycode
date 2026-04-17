//! Orchestra Desktop Notification Helper
//!
//! Cross-platform desktop notifications for auto-mode events.
//! macOS: osascript, Linux: notify-send, Windows: skipped.

use serde::{Deserialize, Serialize};
use std::env;
use std::process::Command;

/// Notification severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum NotifyLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Notification kind/category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum NotificationKind {
    Complete,
    Error,
    Budget,
    Milestone,
    Attention,
}

/// Notification preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub enabled: Option<bool>,
    pub on_complete: Option<bool>,
    pub on_error: Option<bool>,
    pub on_budget: Option<bool>,
    pub on_milestone: Option<bool>,
    pub on_attention: Option<bool>,
}

/// Send a native desktop notification
///
/// Non-blocking, non-fatal. macOS uses osascript, Linux uses notify-send,
/// Windows is skipped (no built-in notification mechanism).
///
/// # Arguments
/// * `title` - Notification title
/// * `message` - Notification message
/// * `level` - Severity level
/// * `kind` - Notification kind
///
/// # Example
/// ```
/// use rustycode_orchestra::notifications::*;
///
/// send_desktop_notification(
///     "Task Complete",
///     "M01/S01/T01 finished successfully",
///     NotifyLevel::Success,
///     NotificationKind::Complete
/// );
/// ```
pub fn send_desktop_notification(
    title: &str,
    message: &str,
    level: NotifyLevel,
    kind: NotificationKind,
) {
    if !should_send_desktop_notification(kind, None) {
        return;
    }

    if let Some(command) = build_desktop_notification_command(title, message, level) {
        let _ = Command::new(&command.file).args(&command.args).output();
    }
}

/// Check if desktop notification should be sent based on preferences
///
/// # Arguments
/// * `kind` - Notification kind
/// * `preferences` - Optional notification preferences
///
/// # Returns
/// true if notification should be sent
pub fn should_send_desktop_notification(
    kind: NotificationKind,
    preferences: Option<&NotificationPreferences>,
) -> bool {
    // Check if notifications are globally disabled
    if let Some(prefs) = preferences {
        if let Some(enabled) = prefs.enabled {
            if !enabled {
                return false;
            }
        }
    }

    // Check kind-specific preference
    match kind {
        NotificationKind::Error => {
            if let Some(prefs) = preferences {
                if let Some(on_error) = prefs.on_error {
                    return on_error;
                }
            }
            true // Default to true for errors
        }
        NotificationKind::Budget => {
            if let Some(prefs) = preferences {
                if let Some(on_budget) = prefs.on_budget {
                    return on_budget;
                }
            }
            true
        }
        NotificationKind::Milestone => {
            if let Some(prefs) = preferences {
                if let Some(on_milestone) = prefs.on_milestone {
                    return on_milestone;
                }
            }
            true
        }
        NotificationKind::Attention => {
            if let Some(prefs) = preferences {
                if let Some(on_attention) = prefs.on_attention {
                    return on_attention;
                }
            }
            true
        }
        NotificationKind::Complete => {
            if let Some(prefs) = preferences {
                if let Some(on_complete) = prefs.on_complete {
                    return on_complete;
                }
            }
            true
        }
    }
}

/// Build a desktop notification command for the current platform
///
/// # Arguments
/// * `title` - Notification title
/// * `message` - Notification message
/// * `level` - Severity level
///
/// # Returns
/// Command specification, or None if platform not supported
fn build_desktop_notification_command(
    title: &str,
    message: &str,
    level: NotifyLevel,
) -> Option<NotificationCommand> {
    let normalized_title = normalize_notification_text(title);
    let normalized_message = normalize_notification_text(message);

    let platform = env::consts::OS;

    if platform == "macos" {
        let sound = if level == NotifyLevel::Error {
            "sound name \"Basso\""
        } else {
            "sound name \"Glass\""
        };

        let script = format!(
            "display notification \"{}\" with title \"{}\" {}",
            escape_applescript(&normalized_message),
            escape_applescript(&normalized_title),
            sound
        );

        return Some(NotificationCommand {
            file: "osascript".to_string(),
            args: vec!["-e".to_string(), script],
        });
    }

    if platform == "linux" {
        let urgency = match level {
            NotifyLevel::Error => "critical",
            NotifyLevel::Warning => "normal",
            _ => "low",
        };

        return Some(NotificationCommand {
            file: "notify-send".to_string(),
            args: vec![
                "-u".to_string(),
                urgency.to_string(),
                normalized_title,
                normalized_message,
            ],
        });
    }

    // Windows: no built-in notification mechanism
    None
}

/// Normalize notification text
///
/// Removes newlines and trims whitespace.
fn normalize_notification_text(s: &str) -> String {
    s.replace(['\r', '\n'], " ").trim().to_string()
}

/// Escape text for AppleScript
///
/// Escapes backslashes and quotes.
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Notification command specification
#[derive(Debug, Clone)]
pub struct NotificationCommand {
    pub file: String,
    pub args: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_notification_text() {
        let text = "Hello\nWorld\r\n  Test  ";
        let result = normalize_notification_text(text);
        // Removes newlines and trims, but doesn't collapse multiple spaces
        assert!(!result.contains('\n'));
        assert!(!result.contains('\r'));
        assert_eq!(result.trim(), result);
    }

    #[test]
    fn test_escape_applescript() {
        assert_eq!(escape_applescript("test\"quote"), "test\\\"quote");
        assert_eq!(escape_applescript("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_should_send_desktop_notification() {
        let prefs = NotificationPreferences {
            enabled: Some(true),
            on_complete: Some(false),
            on_error: Some(true),
            on_budget: Some(true),
            on_milestone: Some(true),
            on_attention: Some(true),
        };

        // Complete is disabled
        assert!(!should_send_desktop_notification(
            NotificationKind::Complete,
            Some(&prefs)
        ));

        // Error is enabled
        assert!(should_send_desktop_notification(
            NotificationKind::Error,
            Some(&prefs)
        ));

        // Budget is enabled
        assert!(should_send_desktop_notification(
            NotificationKind::Budget,
            Some(&prefs)
        ));
    }

    #[test]
    fn test_should_send_desktop_notification_defaults() {
        // All should default to true when no preferences
        assert!(should_send_desktop_notification(
            NotificationKind::Complete,
            None
        ));
        assert!(should_send_desktop_notification(
            NotificationKind::Error,
            None
        ));
        assert!(should_send_desktop_notification(
            NotificationKind::Budget,
            None
        ));
        assert!(should_send_desktop_notification(
            NotificationKind::Milestone,
            None
        ));
        assert!(should_send_desktop_notification(
            NotificationKind::Attention,
            None
        ));
    }

    #[test]
    fn test_should_send_desktop_notification_disabled() {
        let prefs = NotificationPreferences {
            enabled: Some(false),
            on_complete: Some(true),
            on_error: Some(true),
            on_budget: Some(true),
            on_milestone: Some(true),
            on_attention: Some(true),
        };

        // All should be disabled when globally disabled
        assert!(!should_send_desktop_notification(
            NotificationKind::Complete,
            Some(&prefs)
        ));
        assert!(!should_send_desktop_notification(
            NotificationKind::Error,
            Some(&prefs)
        ));
    }

    #[test]
    fn test_notification_kind_serialization() {
        let kind = NotificationKind::Complete;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"complete\"");

        let parsed: NotificationKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, NotificationKind::Complete);
    }

    #[test]
    fn test_notify_level_serialization() {
        let level = NotifyLevel::Error;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"error\"");

        let parsed: NotifyLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, NotifyLevel::Error);
    }
}
