//! Orchestra Session History View
//!
//! Human-readable display of past auto-mode unit executions.
//! Matches orchestra-2's history.ts implementation.
//!
//! Provides formatting utilities for displaying unit execution history
//! with cost, tokens, and duration.

use crate::metrics::{format_cost, format_duration, format_token_count, UnitMetrics};

// ─── Formatting Helpers ───────────────────────────────────────────────────────────

/// Format relative time string
pub fn format_relative_time(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let diff = now.saturating_sub(timestamp);

    const MINUTE: u64 = 60_000;
    const HOUR: u64 = 3_600_000;
    const DAY: u64 = 86_400_000;

    if diff < MINUTE {
        "just now".to_string()
    } else if diff < HOUR {
        format!("{}m ago", diff / MINUTE)
    } else if diff < DAY {
        format!("{}h ago", diff / HOUR)
    } else {
        format!("{}d ago", diff / DAY)
    }
}

/// Shorten model name for display
pub fn short_model(model: &str) -> String {
    model.replace("claude-", "").replace("anthropic/", "")
}

/// Pad string to right
pub fn pad_right(s: &str, width: usize) -> String {
    format!("{:<width$}", s, width = width)
}

/// Truncate string with ellipsis
pub fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let truncate_len = max_len.saturating_sub(4); // Reserve space for "..."
        let truncated = &s[..truncate_len];
        format!("{}...", truncated)
    }
}

/// Format a single unit metric as a table row
pub fn format_unit_row(u: &UnitMetrics) -> String {
    let duration_ms = u.finished_at.saturating_sub(u.started_at) as u64;
    format!(
        "{}{}{}{}{}{}{}",
        pad_right(&format_relative_time(u.finished_at as u64), 14),
        pad_right(&u.unit_type, 20),
        pad_right(&truncate_with_ellipsis(&u.id, 15), 16),
        pad_right(&short_model(&u.model), 14),
        pad_right(&format_cost(u.cost), 10),
        pad_right(&format_token_count(u.tokens.total), 10),
        format_duration(duration_ms as i64)
    )
}

/// Format table header for history display
pub fn format_history_header() -> String {
    format!(
        "{}{}{}{}{}{}{}",
        pad_right("Time", 14),
        pad_right("Type", 20),
        pad_right("ID", 16),
        pad_right("Model", 14),
        pad_right("Cost", 10),
        pad_right("Tokens", 10),
        "Duration"
    )
}

// ─── Tests ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::TokenCounts;

    #[test]
    fn test_format_relative_time() {
        // Note: These tests are time-dependent and may be flaky
        let just_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(format_relative_time(just_now) == "just now");

        let one_hour_ago = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - 3_600_000;
        assert!(format_relative_time(one_hour_ago).contains("h ago"));
    }

    #[test]
    fn test_short_model() {
        assert_eq!(short_model("claude-3-sonnet"), "3-sonnet");
        assert_eq!(short_model("anthropic/claude-3-opus"), "3-opus");
        assert_eq!(short_model("gpt-4"), "gpt-4");
    }

    #[test]
    fn test_pad_right() {
        assert_eq!(pad_right("test", 10), "test      ");
        assert_eq!(pad_right("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("short", 10), "short");
        assert_eq!(truncate_with_ellipsis("very long string", 10), "very l...");
        assert_eq!(truncate_with_ellipsis("exactlen", 8), "exactlen");
    }

    #[test]
    fn test_format_unit_row() {
        let unit = UnitMetrics {
            unit_type: "task".to_string(),
            id: "M001-S001-T001".to_string(),
            started_at: 1000,
            finished_at: 2000,
            model: "claude-3-sonnet".to_string(),
            cost: 0.01,
            tokens: TokenCounts {
                input: 1000,
                output: 500,
                cache_read: 0,
                cache_write: 0,
                total: 1500,
            },
            tool_calls: 0,
            assistant_messages: 0,
            user_messages: 0,
            context_window_tokens: None,
            truncation_sections: None,
            continue_here_fired: None,
            prompt_char_count: None,
            baseline_char_count: None,
            tier: None,
            model_downgraded: None,
            cache_hit_rate: None,
            compression_savings: None,
            skills: None,
        };

        let row = format_unit_row(&unit);
        assert!(row.contains("task"));
        assert!(row.contains("M001-S001-T001"));
        assert!(row.contains("3-sonnet"));
    }

    #[test]
    fn test_format_history_header() {
        let header = format_history_header();
        assert!(header.contains("Time"));
        assert!(header.contains("Type"));
        assert!(header.contains("ID"));
        assert!(header.contains("Model"));
        assert!(header.contains("Cost"));
        assert!(header.contains("Tokens"));
        assert!(header.contains("Duration"));
    }
}
