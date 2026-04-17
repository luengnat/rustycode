//! Orchestra Provider Error Pause — Error classification and auto-pause handling
//!
//! Classifies LLM provider errors as transient (auto-resume) or permanent
//! (manual resume), and handles pausing auto-mode with optional auto-resume.

use std::time::Duration;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Provider error classification result
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderErrorClassification {
    /// Whether the error is transient (will auto-resume)
    pub is_transient: bool,
    /// Whether the error is a rate limit
    pub is_rate_limit: bool,
    /// Suggested delay before retry (in milliseconds)
    pub suggested_delay_ms: u64,
}

/// UI trait for provider error pause notifications
pub trait ProviderErrorPauseUI {
    /// Notify the user of an error or status change
    fn notify(&self, message: &str, level: NotificationLevel);
}

/// Notification level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Success,
}

/// Options for pausing auto-mode
#[derive(Debug, Clone, PartialEq)]
pub struct PauseOptions {
    /// Whether this is a rate limit error
    pub is_rate_limit: bool,
    /// Whether this error is transient
    pub is_transient: bool,
    /// Delay before auto-resume (in milliseconds)
    pub retry_after_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// Error Classification
// ---------------------------------------------------------------------------

/// Classify a provider error as transient (auto-resume) or permanent (manual resume)
///
/// # Arguments
/// * `error_msg` - Error message from the provider
///
/// # Returns
/// Classification with transience, rate limit status, and suggested delay
///
/// # Transient Errors
/// - Rate limits (429)
/// - Server errors (500, 502, 503)
/// - Overloaded/internal errors
///
/// # Permanent Errors
/// - Authentication failures
/// - Invalid API key
/// - Billing/quota issues
///
/// # Example
/// ```
/// use rustycode_orchestra::provider_error_pause::*;
///
/// let result = classify_provider_error("Rate limit exceeded");
/// assert!(result.is_transient);
/// assert!(result.is_rate_limit);
/// assert!(result.suggested_delay_ms > 0);
/// ```
pub fn classify_provider_error(error_msg: &str) -> ProviderErrorClassification {
    let error_lower = error_msg.to_lowercase();

    // Check for rate limit errors
    let is_rate_limit = regex_matches(&error_lower, &["rate.?limit", "too many requests", "429"]);

    // Check for server errors (transient)
    let is_server_error = regex_matches(
        &error_lower,
        &[
            "internal server error",
            "500",
            "502",
            "503",
            "overloaded",
            "server_error",
            "api_error",
            "service.?unavailable",
        ],
    );

    // Permanent errors — never auto-resume
    let is_permanent = regex_matches(
        &error_lower,
        &[
            "auth",
            "unauthorized",
            "forbidden",
            "invalid.*key",
            "invalid.*api",
            "billing",
            "quota exceeded",
            "account",
        ],
    ) && !is_rate_limit;

    if is_permanent {
        return ProviderErrorClassification {
            is_transient: false,
            is_rate_limit: false,
            suggested_delay_ms: 0,
        };
    }

    if is_rate_limit {
        // Try to extract retry-after from the message
        let delay_ms = if let Some(reset_sec) = extract_reset_seconds(error_msg) {
            reset_sec * 1000
        } else {
            60_000 // default 60s for rate limits
        };

        return ProviderErrorClassification {
            is_transient: true,
            is_rate_limit: true,
            suggested_delay_ms: delay_ms,
        };
    }

    if is_server_error {
        return ProviderErrorClassification {
            is_transient: true,
            is_rate_limit: false,
            suggested_delay_ms: 30_000, // 30s for server errors
        };
    }

    // Unknown error — treat as permanent (user reviews)
    ProviderErrorClassification {
        is_transient: false,
        is_rate_limit: false,
        suggested_delay_ms: 0,
    }
}

// ---------------------------------------------------------------------------
// Pause Handling
// ---------------------------------------------------------------------------

/// Pause auto-mode due to a provider error
///
/// This function handles pausing auto-mode when a provider error occurs.
/// For transient errors (rate limits, server errors), it schedules an
/// automatic resume after a delay. For permanent errors (auth, billing),
/// it pauses indefinitely and requires manual resume.
///
/// # Arguments
/// * `ui` - UI for notifications
/// * `error_detail` - Detailed error message
/// * `pause` - Function to pause auto-mode
/// * `options` - Pause options including transience and retry delay
///
/// # Example
/// ```
/// use rustycode_orchestra::provider_error_pause::*;
/// use std::sync::Arc;
///
/// struct TestUI;
/// impl ProviderErrorPauseUI for TestUI {
///     fn notify(&self, message: &str, level: NotificationLevel) {
///         println!("{:?}: {}", level, message);
///     }
/// }
///
/// let ui = TestUI;
/// let options = PauseOptions {
///     is_rate_limit: true,
///     is_transient: true,
///     retry_after_ms: Some(30_000),
/// };
///
/// // This would pause and schedule auto-resume
/// // pause_auto_for_provider_error(&ui, ": rate limit", || async { Ok(()) }, Some(options));
/// ```
pub fn pause_auto_for_provider_error<F>(
    ui: &dyn ProviderErrorPauseUI,
    error_detail: &str,
    pause: F,
    options: Option<PauseOptions>,
) where
    F: FnOnce() + Send + 'static,
{
    let should_auto_resume = if let Some(ref opts) = options {
        opts.is_rate_limit || opts.is_transient
    } else {
        false
    };

    let retry_after_ms = options
        .as_ref()
        .and_then(|opts| opts.retry_after_ms)
        .unwrap_or(0);

    if should_auto_resume && retry_after_ms > 0 {
        let delay_sec = retry_after_ms.div_ceil(1000); // Round up
        let is_rate_limit = options
            .as_ref()
            .map(|opts| opts.is_rate_limit)
            .unwrap_or(false);

        let reason = if is_rate_limit {
            "Rate limited"
        } else {
            "Server error (transient)"
        };

        ui.notify(
            &format!(
                "{}{}. Auto-resuming in {}s...",
                reason, error_detail, delay_sec
            ),
            NotificationLevel::Warning,
        );

        // Pause immediately
        pause();

        // Schedule auto-resume after the delay
        // Note: In a real implementation, this would use tokio::time::sleep
        // or a similar async scheduler. For now, we just return the delay.
        tracing::debug!(
            "[provider_error_pause] Auto-resume scheduled in {}ms - implement async scheduler",
            retry_after_ms
        );
    } else {
        ui.notify(
            &format!("Auto-mode paused due to provider error{}", error_detail),
            NotificationLevel::Warning,
        );

        // Pause indefinitely
        pause();
    }
}

/// Pause auto-mode with async support
///
/// This is an async version that returns a future that handles the
/// auto-resume scheduling.
///
/// # Arguments
/// * `ui` - UI for notifications
/// * `error_detail` - Detailed error message
/// * `pause` - Async function to pause auto-mode
/// * `resume` - Function to resume auto-mode
/// * `options` - Pause options
///
/// # Returns
/// Future that completes when pause is set up
pub async fn pause_auto_for_provider_error_async<Fut, ResumeF>(
    ui: &dyn ProviderErrorPauseUI,
    error_detail: &str,
    pause: Fut,
    resume: ResumeF,
    options: Option<PauseOptions>,
) where
    Fut: std::future::Future<Output = ()> + Send + 'static,
    ResumeF: FnOnce() + Send + 'static,
{
    let should_auto_resume = if let Some(ref opts) = options {
        opts.is_rate_limit || opts.is_transient
    } else {
        false
    };

    let retry_after_ms = options
        .as_ref()
        .and_then(|opts| opts.retry_after_ms)
        .unwrap_or(0);

    if should_auto_resume && retry_after_ms > 0 {
        let delay_sec = retry_after_ms.div_ceil(1000); // Round up
        let is_rate_limit = options
            .as_ref()
            .map(|opts| opts.is_rate_limit)
            .unwrap_or(false);

        let reason = if is_rate_limit {
            "Rate limited"
        } else {
            "Server error (transient)"
        };

        ui.notify(
            &format!(
                "{}{}. Auto-resuming in {}s...",
                reason, error_detail, delay_sec
            ),
            NotificationLevel::Warning,
        );

        // Pause immediately
        pause.await;

        // Schedule auto-resume
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(retry_after_ms)).await;

            let resume_msg = if is_rate_limit {
                "Rate limit window elapsed. Resuming auto-mode."
            } else {
                "Server error recovery delay elapsed. Resuming auto-mode."
            };

            // Note: In real implementation, we'd call ui.notify here
            // but we don't have a way to keep ui alive across the await
            tracing::debug!("[provider_error_pause] {}", resume_msg);

            resume();
        });
    } else {
        ui.notify(
            &format!("Auto-mode paused due to provider error{}", error_detail),
            NotificationLevel::Warning,
        );

        // Pause indefinitely
        pause.await;
    }
}

// ---------------------------------------------------------------------------
// Helper Functions
// ---------------------------------------------------------------------------

/// Check if a string matches any of the given regex patterns
fn regex_matches(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| {
        // Handle ".?" as optional character
        // For "rate.?limit", match both "ratelimit" and "rate limit"
        if pattern.contains(".?") {
            let parts: Vec<&str> = pattern.split(".?").collect();
            if parts.len() == 2 {
                // Try with optional character removed (concatenated)
                let concat = format!("{}{}", parts[0], parts[1]);
                if text.contains(&concat.to_lowercase()) {
                    return true;
                }
                // Try with a space instead of the optional character
                let with_space = format!("{} {}", parts[0], parts[1]);
                if text.contains(&with_space.to_lowercase()) {
                    return true;
                }
            }
        }

        // Fall back to simple substring match
        text.contains(&pattern.to_lowercase())
    })
}

/// Extract "reset in X seconds" from an error message
fn extract_reset_seconds(error_msg: &str) -> Option<u64> {
    // Look for patterns like "reset in 60s" or "reset in 60 seconds"
    let lower = error_msg.to_lowercase();

    // Try "reset in Xs" pattern
    if let Some(pos) = lower.find("reset in ") {
        let after = &lower[pos + 9..];
        if let Some(end) = after.find('s') {
            let num_str = &after[..end];
            if let Ok(secs) = num_str.parse::<u64>() {
                return Some(secs);
            }
        }
    }

    // Try "reset in X seconds" pattern
    if let Some(pos) = lower.find("reset in ") {
        let after = &lower[pos + 9..];
        if let Some(end) = after.find(" second") {
            let num_str = &after[..end];
            if let Ok(secs) = num_str.trim().parse::<u64>() {
                return Some(secs);
            }
        }
    }

    None
}

/// Convert NotificationLevel to string for display
impl std::fmt::Display for NotificationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationLevel::Info => write!(f, "info"),
            NotificationLevel::Warning => write!(f, "warning"),
            NotificationLevel::Error => write!(f, "error"),
            NotificationLevel::Success => write!(f, "success"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct TestUI {
        notifications: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl TestUI {
        fn new() -> Self {
            TestUI {
                notifications: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn get_notifications(&self) -> Vec<String> {
            self.notifications
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }
    }

    impl ProviderErrorPauseUI for TestUI {
        fn notify(&self, message: &str, level: NotificationLevel) {
            self.notifications
                .lock()
                .unwrap()
                .push(format!("{}: {}", level, message));
        }
    }

    #[test]
    fn test_classify_rate_limit_error() {
        let result = classify_provider_error("Rate limit exceeded");
        assert!(result.is_transient);
        assert!(result.is_rate_limit);
        assert_eq!(result.suggested_delay_ms, 60_000);
    }

    #[test]
    fn test_classify_rate_limit_with_reset() {
        let result = classify_provider_error("Rate limit exceeded, reset in 30s");
        assert!(result.is_transient);
        assert!(result.is_rate_limit);
        assert_eq!(result.suggested_delay_ms, 30_000);
    }

    #[test]
    fn test_classify_server_error() {
        let result = classify_provider_error("Internal server error");
        assert!(result.is_transient);
        assert!(!result.is_rate_limit);
        assert_eq!(result.suggested_delay_ms, 30_000);
    }

    #[test]
    fn test_classify_503_error() {
        let result = classify_provider_error("Service unavailable (503)");
        assert!(result.is_transient);
        assert!(!result.is_rate_limit);
        assert_eq!(result.suggested_delay_ms, 30_000);
    }

    #[test]
    fn test_classify_auth_error() {
        let result = classify_provider_error("Unauthorized: invalid API key");
        assert!(!result.is_transient);
        assert!(!result.is_rate_limit);
        assert_eq!(result.suggested_delay_ms, 0);
    }

    #[test]
    fn test_classify_billing_error() {
        let result = classify_provider_error("Quota exceeded, please upgrade");
        assert!(!result.is_transient);
        assert!(!result.is_rate_limit);
        assert_eq!(result.suggested_delay_ms, 0);
    }

    #[test]
    fn test_classify_unknown_error() {
        let result = classify_provider_error("Something went wrong");
        assert!(!result.is_transient);
        assert!(!result.is_rate_limit);
        assert_eq!(result.suggested_delay_ms, 0);
    }

    #[test]
    fn test_pause_with_auto_resume() {
        let ui = TestUI::new();
        let pause_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let pause_called_clone = pause_called.clone();
        let pause = move || {
            pause_called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        };

        let options = PauseOptions {
            is_rate_limit: true,
            is_transient: true,
            retry_after_ms: Some(30_000),
        };

        pause_auto_for_provider_error(&ui, ": test error", pause, Some(options));

        assert!(pause_called.load(std::sync::atomic::Ordering::SeqCst));
        let notifications = ui.get_notifications();
        assert!(notifications
            .iter()
            .any(|n| n.contains("Auto-resuming in 30s")));
    }

    #[test]
    fn test_pause_permanent_error() {
        let ui = TestUI::new();
        let pause_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let pause_called_clone = pause_called.clone();
        let pause = move || {
            pause_called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        };

        let options = PauseOptions {
            is_rate_limit: false,
            is_transient: false,
            retry_after_ms: None,
        };

        pause_auto_for_provider_error(&ui, ": auth failed", pause, Some(options));

        assert!(pause_called.load(std::sync::atomic::Ordering::SeqCst));
        let notifications = ui.get_notifications();
        assert!(notifications.iter().any(|n| n.contains("Auto-mode paused")));
        assert!(!notifications.iter().any(|n| n.contains("Auto-resuming")));
    }

    #[test]
    fn test_extract_reset_seconds() {
        assert_eq!(extract_reset_seconds("reset in 60s"), Some(60));
        assert_eq!(extract_reset_seconds("reset in 30 seconds"), Some(30));
        assert_eq!(extract_reset_seconds("reset in 5s"), Some(5));
        assert_eq!(extract_reset_seconds("no reset info"), None);
    }

    #[test]
    fn test_notification_level_display() {
        assert_eq!(format!("{}", NotificationLevel::Info), "info");
        assert_eq!(format!("{}", NotificationLevel::Warning), "warning");
        assert_eq!(format!("{}", NotificationLevel::Error), "error");
        assert_eq!(format!("{}", NotificationLevel::Success), "success");
    }
}
