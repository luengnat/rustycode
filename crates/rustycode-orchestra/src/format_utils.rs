//! Orchestra Format Utilities — Shared formatting and layout utilities.
//!
//! Consolidates helpers for formatting durations, token counts, layouts,
//! text truncation, sparklines, dates, and ANSI handling.
//!
//! Matches orchestra-2's format-utils.ts implementation.

use std::collections::HashSet;

// ─── Duration Formatting ───────────────────────────────────────────────────────

/// Format a millisecond duration as a compact human-readable string.
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::format_utils::format_duration;
///
/// assert_eq!(format_duration(500), "500ms");
/// assert_eq!(format_duration(1500), "1s");
/// assert_eq!(format_duration(90000), "1m 30s");
/// assert_eq!(format_duration(3665000), "1h 1m");
/// ```
pub fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        return format!("{}ms", ms);
    }
    let s = ms / 1000;
    if s < 60 {
        return format!("{}s", s);
    }
    let m = s / 60;
    let rs = s % 60;
    if m < 60 {
        return format!("{}m {}s", m, rs);
    }
    let h = m / 60;
    let rm = m % 60;
    format!("{}h {}m", h, rm)
}

// ─── Token Count Formatting ────────────────────────────────────────────────────

/// Format a token count as a compact human-readable string (e.g. 1.5k, 1.50M).
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::format_utils::format_token_count;
///
/// assert_eq!(format_token_count(999), "999");
/// assert_eq!(format_token_count(1500), "1.5k");
/// assert_eq!(format_token_count(1_500_000), "1.50M");
/// ```
pub fn format_token_count(count: u64) -> String {
    if count < 1000 {
        return format!("{}", count);
    }
    if count < 1_000_000 {
        return format!("{:.1}k", count as f64 / 1000.0);
    }
    format!("{:.2}M", count as f64 / 1_000_000.0)
}

// ─── Layout Helpers ────────────────────────────────────────────────────────────

/// Pad a string with trailing spaces to fill `width`.
/// Note: This is a simplified version that doesn't handle ANSI codes.
pub fn pad_right(content: &str, width: usize) -> String {
    let vis = content.chars().count();
    let padding = width.saturating_sub(vis);
    format!("{}{}", content, " ".repeat(padding))
}

/// Build a line with left-aligned and right-aligned content.
/// Note: This is a simplified version that doesn't handle ANSI codes.
pub fn join_columns(left: &str, right: &str, width: usize) -> String {
    let left_w = left.chars().count();
    let right_w = right.chars().count();
    if left_w + right_w + 2 > width {
        // Truncate if too long
        let combined = format!("{}  {}", left, right);
        return truncate_with_ellipsis(&combined, width);
    }
    let padding = width - left_w - right_w;
    format!("{}{}{}", left, " ".repeat(padding), right)
}

/// Center content within `width`.
/// Note: This is a simplified version that doesn't handle ANSI codes.
pub fn center_line(content: &str, width: usize) -> String {
    let vis = content.chars().count();
    if vis >= width {
        return truncate_with_ellipsis(content, width);
    }
    let left_pad = (width - vis) / 2;
    format!("{}{}", " ".repeat(left_pad), content)
}

/// Join as many parts as fit within `width`, separated by `separator`.
/// Note: This is a simplified version that doesn't handle ANSI codes.
pub fn fit_columns(parts: &[&str], width: usize, separator: &str) -> String {
    let filtered: Vec<&str> = parts.iter().copied().filter(|s| !s.is_empty()).collect();
    if filtered.is_empty() {
        return String::new();
    }

    let mut result = filtered[0].to_string();
    for part in filtered.iter().skip(1) {
        let candidate = format!("{}{}{}", result, separator, part);
        if candidate.chars().count() > width {
            break;
        }
        result = candidate;
    }
    truncate_with_ellipsis(&result, width)
}

// ─── Text Truncation ───────────────────────────────────────────────────────────

/// Truncate a string to `max_length` characters, replacing the last character with an ellipsis if needed.
pub fn truncate_with_ellipsis(text: &str, max_length: usize) -> String {
    if text.chars().count() <= max_length {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_length.saturating_sub(1)).collect();
    format!("{}…", truncated)
}

// ─── Data Visualization ────────────────────────────────────────────────────────

/// Render a sparkline from numeric values using Unicode block characters.
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::format_utils::sparkline;
///
/// let values = vec![1, 3, 5, 2, 4];
/// let spark = sparkline(&values);
/// assert!(!spark.is_empty());
/// ```
pub fn sparkline(values: &[u64]) -> String {
    if values.is_empty() {
        return String::new();
    }

    // Unicode block characters from 1/8 to 8/8 (full block)
    let chars = [
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];

    let max = *values.iter().max().unwrap_or(&0);
    if max == 0 {
        return chars[0].to_string().repeat(values.len());
    }

    values
        .iter()
        .map(|&v| {
            let index = std::cmp::min(7, (v * 7 / max) as usize);
            chars[index]
        })
        .collect()
}

// ─── Date Formatting ───────────────────────────────────────────────────────────

/// Format an ISO date string as a compact locale string.
/// Returns the input string unchanged if parsing fails.
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::format_utils::format_date_short;
///
/// let iso = "2025-03-17T14:30:00Z";
/// let formatted = format_date_short(iso);
/// assert!(!formatted.is_empty());
/// ```
pub fn format_date_short(iso: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(iso) {
        Ok(dt) => dt.format("%b %-d, %Y, %H:%M").to_string(),
        Err(_) => iso.to_string(),
    }
}

// ─── ANSI Stripping ─────────────────────────────────────────────────────────────

/// Strip ANSI escape sequences from a string.
pub fn strip_ansi(s: &str) -> String {
    // ANSI escape sequence pattern: ESC[ ... m
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

// ─── String Array Normalization ─────────────────────────────────────────────────

/// Normalize an unknown value to a string array.
/// Filters to string items, trims whitespace, removes empty strings.
/// Optionally deduplicates.
pub fn normalize_string_array(value: &serde_json::Value, dedupe: bool) -> Vec<String> {
    if !value.is_array() {
        return Vec::new();
    }

    let items: Vec<String> = value
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if dedupe {
        let seen: HashSet<String> = items.into_iter().collect();
        seen.into_iter().collect()
    } else {
        items
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(1500), "1s");
        assert_eq!(format_duration(90000), "1m 30s");
        assert_eq!(format_duration(3665000), "1h 1m");
        assert_eq!(format_duration(0), "0ms");
    }

    #[test]
    fn test_format_token_count() {
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1000), "1.0k");
        assert_eq!(format_token_count(1500), "1.5k");
        assert_eq!(format_token_count(1_000_000), "1.00M");
        assert_eq!(format_token_count(1_500_000), "1.50M");
        assert_eq!(format_token_count(0), "0");
    }

    #[test]
    fn test_pad_right() {
        assert_eq!(pad_right("hello", 10), "hello     ");
        assert_eq!(pad_right("hello", 5), "hello");
        assert_eq!(pad_right("hello", 3), "hello");
    }

    #[test]
    fn test_join_columns() {
        assert_eq!(join_columns("left", "right", 15), "left      right");
        assert_eq!(join_columns("left", "right", 10), "left  rig…");
    }

    #[test]
    fn test_center_line() {
        assert_eq!(center_line("hi", 10), "    hi");
        assert_eq!(center_line("hello world", 11), "hello world");
        assert_eq!(center_line("hello world", 8), "hello w…");
    }

    #[test]
    fn test_fit_columns() {
        assert_eq!(fit_columns(&["a", "b", "c"], 10, "  "), "a  b  c");
        assert_eq!(fit_columns(&["a", "b", "c"], 5, "  "), "a  b");
        assert_eq!(fit_columns(&[], 10, "  "), "");
    }

    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_with_ellipsis("hello world", 8), "hello w…");
        assert_eq!(truncate_with_ellipsis("", 5), "");
    }

    #[test]
    fn test_sparkline() {
        assert_eq!(sparkline(&[]), "");
        let spark = sparkline(&[1, 3, 5, 2, 4]);
        assert!(!spark.is_empty());
        assert!(sparkline(&[0, 0, 0]).contains('▁'));
    }

    #[test]
    fn test_format_date_short() {
        let iso = "2025-03-17T14:30:00Z";
        let formatted = format_date_short(iso);
        assert!(!formatted.is_empty());
        assert!(!formatted.contains('T'));

        let invalid = "not a date";
        assert_eq!(format_date_short(invalid), "not a date");
    }

    #[test]
    fn test_strip_ansi() {
        let with_ansi = "\x1b[31mError\x1b[0m";
        assert_eq!(strip_ansi(with_ansi), "Error");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn test_normalize_string_array() {
        let json = serde_json::json!(["foo", " bar ", "", "baz", "foo"]);
        let result = normalize_string_array(&json, false);
        assert_eq!(result, vec!["foo", "bar", "baz", "foo"]);

        let result_dedupe = normalize_string_array(&json, true);
        // HashSet doesn't preserve order, so just check the length and unique items
        assert_eq!(result_dedupe.len(), 3);
        assert!(result_dedupe.contains(&"foo".to_string()));
        assert!(result_dedupe.contains(&"bar".to_string()));
        assert!(result_dedupe.contains(&"baz".to_string()));

        let not_array = serde_json::json!("string");
        assert_eq!(
            normalize_string_array(&not_array, false),
            Vec::<String>::new()
        );
    }
}
