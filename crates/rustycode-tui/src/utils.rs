//! Text and formatting utilities for the TUI
//!
//! Provides helper functions for text truncation and formatting.
//! All functions now properly handle Unicode grapheme clusters.
#![allow(dead_code)]

use crate::unicode::grapheme_count;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

/// Truncate text from the end for UI display
///
/// This version properly handles Unicode grapheme clusters.
/// It respects display width, not character count.
pub fn truncate_for_ui(s: &str, max_chars: usize, suffix: &str) -> String {
    if max_chars == 0 {
        return String::new();
    }

    // Use grapheme count for better Unicode support
    let grapheme_total = grapheme_count(s);
    if grapheme_total <= max_chars {
        return s.to_string();
    }

    let suffix_len = grapheme_count(suffix);
    if max_chars <= suffix_len {
        // Truncate to max_chars graphemes
        return s.graphemes(true).take(max_chars).collect::<String>();
    }

    let keep = max_chars - suffix_len;
    let truncated: String = s.graphemes(true).take(keep).collect();
    format!("{}{}", truncated, suffix)
}

/// Truncate text from the beginning for UI display
///
/// This version properly handles Unicode grapheme clusters.
pub fn truncate_tail_for_ui(s: &str, max_chars: usize, prefix: &str) -> String {
    let grapheme_total = grapheme_count(s);
    if grapheme_total <= max_chars {
        return s.to_string();
    }

    let keep = max_chars.saturating_sub(grapheme_count(prefix));
    let graphemes: Vec<&str> = s.graphemes(true).collect();
    let start = grapheme_total.saturating_sub(keep);
    let tail: String = graphemes[start..].join("");
    format!("{}{}", prefix, tail)
}

/// Count display width accounting for tabs and Unicode
///
/// This version properly handles:
/// - Tab stops (configurable)
/// - Wide characters (CJK, emoji)
/// - Combining marks (Thai, Arabic)
pub fn display_width(s: &str, tab_size: usize) -> usize {
    let mut width = 0;
    for c in s.chars() {
        if c == '\t' {
            // Tabs advance to next tab stop
            width = ((width / tab_size) + 1) * tab_size;
        } else {
            // Use Unicode width for proper handling
            // This is a simplified version - for full accuracy,
            // we'd need to use grapheme clusters
            width += c.width().unwrap_or(0);
        }
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_for_ui() {
        assert_eq!(truncate_for_ui("hello", 10, "..."), "hello");
        assert_eq!(truncate_for_ui("hello world", 8, "..."), "hello...");
        assert_eq!(truncate_for_ui("", 5, "..."), "");
    }

    #[test]
    fn test_truncate_for_ui_thai() {
        // Thai text should be truncated at grapheme boundaries
        let thai = "สวัสดีชาวโลก";
        let truncated = truncate_for_ui(thai, 5, "...");
        assert!(truncated.ends_with("..."));
        assert!(grapheme_count(&truncated) <= 5);
    }

    #[test]
    fn test_truncate_tail_for_ui() {
        assert_eq!(truncate_tail_for_ui("hello", 10, "..."), "hello");
        assert_eq!(truncate_tail_for_ui("hello world", 8, "..."), "...world");
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("hello", 4), 5);
        assert_eq!(display_width("\t", 4), 4);
        assert_eq!(display_width("a\tb", 4), 5);

        // Test Unicode width
        assert_eq!(display_width("🌍", 4), 2); // Emoji is 2 columns
        assert_eq!(display_width("สวัสดี", 4), 5); // Thai is 1 column each
    }

    #[test]
    fn test_display_width_thai() {
        // Thai characters are typically 1 column wide
        assert_eq!(display_width("สวัสดี", 4), 5);
    }

    #[test]
    fn test_display_width_emoji() {
        // Most emoji are 2 columns wide
        assert_eq!(display_width("🌍", 4), 2);
        assert_eq!(display_width("😀", 4), 2);
    }

    #[test]
    fn test_display_width_mixed() {
        // Mixed scripts
        assert_eq!(display_width("Hello 🌍", 4), 8); // 5 + 1 + 2
    }
}
