//! Shared rendering helpers used by multiple renderer backends.
//!
//! Centralises utilities that were previously duplicated across the polished
//! and brutalist render paths. Both `render/messages.rs` (polished) and
//! `brutalist_renderer.rs` should import from here.

use ratatui::layout::Rect;

// ============================================================================
// LINE ESTIMATION
// ============================================================================

/// Estimate line count for a string without iterating all content.
///
/// For small strings (<4 KB) an exact count is returned. For larger strings
/// the function counts newlines in a 4 KB prefix and extrapolates from the
/// byte ratio. This avoids O(n) scans on 100 KB+ thinking blocks every
/// render frame.
#[inline]
pub fn estimate_line_count(s: &str) -> usize {
    const SAMPLE_BYTES: usize = 4096;
    if s.len() < SAMPLE_BYTES {
        return s.lines().count();
    }
    let prefix = &s[..s.floor_char_boundary(SAMPLE_BYTES)];
    let prefix_newlines = prefix.bytes().filter(|&b| b == b'\n').count();
    let ratio = prefix_newlines as f64 / SAMPLE_BYTES as f64;
    (s.len() as f64 * ratio) as usize + 1
}

// ============================================================================
// SIZE / DURATION FORMATTING
// ============================================================================

/// Format a byte count as a human-readable string.
///
/// Examples: `"42b"`, `"3.1kb"`, `"1.2mb"`
pub fn format_byte_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{}b", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}kb", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}mb", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Format a duration in milliseconds as a human-readable string.
///
/// Examples: `"42ms"`, `"3.2s"`, `"1m4s"`
pub fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let secs = ms / 1000;
        let mins = secs / 60;
        let remain_secs = secs % 60;
        format!("{}m{}s", mins, remain_secs)
    }
}

// ============================================================================
// PATH SHORTENING
// ============================================================================

/// Goose-inspired smart path shortening for compact tool display.
///
/// Replaces the home-directory prefix with `~`, then abbreviates all middle
/// path components to their first character while preserving the root and
/// the filename:
///
/// ```text
/// /Users/nat/dev/rustycode/crates/main.rs  →  ~/d/r/c/main.rs
/// src/rustycode_tui/app/render/mod.rs      →  s/r/a/r/mod.rs
/// ```
///
/// Paths with ≤ 3 components (or ≤ 4 when rooted at `~`) are left unchanged.
pub fn shorten_path(path: &str) -> String {
    use std::path::Path;

    let display = match std::env::var("HOME") {
        Ok(home) if path.starts_with(&home) => format!("~{}", &path[home.len()..]),
        _ => path.to_string(),
    };

    let components: Vec<&str> = Path::new(&display)
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    let threshold = if display.starts_with("~/") { 4 } else { 3 };
    if components.len() <= threshold {
        return display;
    }

    let mut shortened = Vec::with_capacity(components.len());
    for (i, component) in components.iter().enumerate() {
        if i == 0 || i == components.len() - 1 {
            shortened.push((*component).to_string());
        } else if component.starts_with('.') {
            let second = component.chars().nth(1).unwrap_or('?');
            shortened.push(format!(".{}", second));
        } else {
            let first_char = component.chars().next().unwrap_or('?');
            shortened.push(first_char.to_string());
        }
    }

    shortened.join("/")
}

/// Unicode-safe string truncation with ellipsis suffix.
///
/// Returns `s` unchanged when `s.chars().count() <= max_chars`.
/// Otherwise returns the first `max_chars - 3` characters followed by `"..."`.
pub fn safe_truncate(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

// ============================================================================
// LAYOUT HELPERS
// ============================================================================

/// Build a fixed-size rectangle centered within the given area.
///
/// The requested size is clamped to the available area so callers can use
/// it safely for both full-screen overlays and smaller modal panes.
pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

// ============================================================================
// TOOL KIND ICONS
// ============================================================================

/// Map a tool name to a single-character kind icon for compact display.
///
/// Used by the polished message render path. The brutalist renderer has its
/// own richer icon set in `brutalist_helpers::tool_type_icon`; this simpler
/// variant is sufficient for the inline summary rows.
pub fn tool_kind_icon(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("read") || lower.contains("cat") || lower.contains("view") {
        "R"
    } else if lower.contains("write") || lower.contains("create") || lower.contains("insert") {
        "W"
    } else if lower.contains("edit")
        || lower.contains("patch")
        || lower.contains("replace")
        || lower.contains("search_replace")
    {
        "E"
    } else if lower.contains("delete") || lower.contains("remove") {
        "D"
    } else if lower.contains("grep") || lower.contains("search") {
        "G"
    } else if lower.contains("glob") || lower.contains("find") || lower.contains("list") {
        "F"
    } else if lower.contains("bash")
        || lower.contains("exec")
        || lower.contains("shell")
        || lower.contains("run")
        || lower.contains("cmd")
    {
        "$"
    } else if lower.contains("git") {
        "G"
    } else if lower.contains("fetch")
        || lower.contains("http")
        || lower.contains("web")
        || lower.contains("curl")
        || lower.contains("download")
    {
        "~"
    } else if lower.contains("question")
        || lower.contains("ask")
        || lower.contains("think")
        || lower.contains("reason")
    {
        "?"
    } else if lower.contains("todo") {
        "T"
    } else if lower.contains("agent") || lower.contains("spawn") || lower.contains("team") {
        "A"
    } else {
        "*"
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_line_count_exact_for_small_strings() {
        let s = "line1\nline2\nline3";
        assert_eq!(estimate_line_count(s), 3);
    }

    #[test]
    fn estimate_line_count_empty() {
        assert_eq!(estimate_line_count(""), 0);
    }

    #[test]
    fn estimate_line_count_large_string() {
        // Build a string that exceeds the 4 KB sample threshold
        let line = "a".repeat(100) + "\n";
        let big = line.repeat(100); // 10100 bytes, 100 lines
        let est = estimate_line_count(&big);
        // Should be in the right ballpark (not exact, but within 2×)
        assert!((50..=200).contains(&est), "estimate out of range: {}", est);
    }

    #[test]
    fn format_byte_size_small() {
        assert_eq!(format_byte_size(42), "42b");
    }

    #[test]
    fn format_byte_size_kb() {
        assert_eq!(format_byte_size(2048), "2.0kb");
    }

    #[test]
    fn format_byte_size_mb() {
        assert_eq!(format_byte_size(2 * 1024 * 1024), "2.0mb");
    }

    #[test]
    fn format_duration_ms_millis() {
        assert_eq!(format_duration_ms(42), "42ms");
    }

    #[test]
    fn format_duration_ms_seconds() {
        assert_eq!(format_duration_ms(1500), "1.5s");
    }

    #[test]
    fn format_duration_ms_minutes() {
        assert_eq!(format_duration_ms(90_000), "1m30s");
    }

    #[test]
    fn safe_truncate_short() {
        assert_eq!(safe_truncate("hello", 10), "hello");
    }

    #[test]
    fn safe_truncate_long() {
        let result = safe_truncate("hello world", 8);
        assert_eq!(result, "hello...");
    }

    #[test]
    fn tool_kind_icon_read() {
        assert_eq!(tool_kind_icon("read_file"), "R");
    }

    #[test]
    fn tool_kind_icon_bash() {
        assert_eq!(tool_kind_icon("bash_exec"), "$");
    }

    #[test]
    fn tool_kind_icon_unknown() {
        assert_eq!(tool_kind_icon("something_exotic"), "*");
    }

    // ── shorten_path tests ──────────────────────────────────────────────────

    #[test]
    fn shorten_path_short_path_unchanged() {
        assert_eq!(shorten_path("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn shorten_path_three_components_unchanged() {
        assert_eq!(shorten_path("a/b/c"), "a/b/c");
    }

    #[test]
    fn shorten_path_long_path_abbreviated() {
        let result = shorten_path("src/rustycode_tui/app/render/mod.rs");
        assert_eq!(result, "s/r/a/r/mod.rs");
    }

    #[test]
    fn shorten_path_hidden_dir() {
        let result = shorten_path("src/.hidden/deep/file.rs");
        assert_eq!(result, "src/.h/d/file.rs");
    }

    // ── centered_rect tests ─────────────────────────────────────────────────

    #[test]
    fn centered_rect_exact_center() {
        let area = Rect::new(0, 0, 80, 24);
        let result = centered_rect(20, 10, area);
        assert_eq!(result.x, 30);
        assert_eq!(result.y, 7);
        assert_eq!(result.width, 20);
        assert_eq!(result.height, 10);
    }

    #[test]
    fn centered_rect_clamps_to_area() {
        let area = Rect::new(0, 0, 40, 10);
        let result = centered_rect(100, 50, area);
        assert_eq!(result.width, 40);
        assert_eq!(result.height, 10);
    }

    #[test]
    fn centered_rect_with_offset_area() {
        let area = Rect::new(10, 5, 60, 20);
        let result = centered_rect(20, 10, area);
        assert_eq!(result.x, 30);
        assert_eq!(result.y, 10);
    }

    // ── safe_truncate unicode ────────────────────────────────────────────────

    #[test]
    fn safe_truncate_unicode() {
        let s = "héllo wörld тест";
        let result = safe_truncate(s, 8);
        assert_eq!(result, "héllo...");
    }

    #[test]
    fn safe_truncate_exact_boundary() {
        assert_eq!(safe_truncate("hello", 5), "hello");
    }

    // ── tool_kind_icon coverage ──────────────────────────────────────────────

    #[test]
    fn tool_kind_icon_edit() {
        assert_eq!(tool_kind_icon("edit_file"), "E");
    }

    #[test]
    fn tool_kind_icon_delete() {
        assert_eq!(tool_kind_icon("delete_file"), "D");
    }

    #[test]
    fn tool_kind_icon_search() {
        assert_eq!(tool_kind_icon("grep_search"), "G");
    }

    #[test]
    fn tool_kind_icon_git() {
        assert_eq!(tool_kind_icon("git_commit"), "G");
    }

    #[test]
    fn tool_kind_icon_http() {
        assert_eq!(tool_kind_icon("web_fetch"), "~");
    }

    #[test]
    fn tool_kind_icon_question() {
        assert_eq!(tool_kind_icon("ask_question"), "?");
    }

    #[test]
    fn tool_kind_icon_todo() {
        assert_eq!(tool_kind_icon("todo_write"), "T");
    }

    #[test]
    fn tool_kind_icon_agent() {
        assert_eq!(tool_kind_icon("agent_spawn"), "A");
    }

    #[test]
    fn tool_kind_icon_case_insensitive() {
        assert_eq!(tool_kind_icon("Read_File"), "R");
        assert_eq!(tool_kind_icon("BASH"), "$");
    }
}
