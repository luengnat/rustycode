//! Unicode helper functions for proper text handling.
//!
//! This module provides utilities for working with Unicode text,
//! with special attention to complex scripts like Thai, Arabic,
//! and emoji that require grapheme cluster handling.

use unicode_width::UnicodeWidthStr;

/// Calculate true display width of text (accounts for combining marks).
///
/// This uses the `unicode-width` crate to properly calculate the display
/// width of text, taking into account:
/// - Wide characters (CJK, emoji)
/// - Combining marks (Thai vowels, Arabic diacritics)
/// - Zero-width characters
///
/// # Examples
///
/// ```rust,ignore
/// use rustycode_tui::unicode::display_width;
///
/// assert_eq!(display_width("Hello"), 5);
/// assert_eq!(display_width("สวัสดี"), 5); // Thai - each char is 1 column
/// assert_eq!(display_width("👨‍👩‍👧‍👦"), 2); // Family emoji is 2 columns
/// assert_eq!(display_width("🌍"), 2); // Globe emoji is 2 columns
/// ```
pub fn display_width(text: &str) -> usize {
    text.width()
}

/// Get the byte offset of the previous grapheme cluster.
///
/// # Arguments
///
/// * `text` - The text to search
/// * `byte_pos` - Current byte position
///
/// # Returns
///
/// Byte position of the previous grapheme, or 0 if at start
pub fn prev_grapheme_boundary(text: &str, byte_pos: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let mut prev = 0;
    for (i, _) in text.grapheme_indices(true) {
        if i >= byte_pos {
            break;
        }
        prev = i;
    }
    prev
}

/// Get the byte offset of the next grapheme cluster.
///
/// # Arguments
///
/// * `text` - The text to search
/// * `byte_pos` - Current byte position
///
/// # Returns
///
/// Byte position of the next grapheme, or text.len() if at end
pub fn next_grapheme_boundary(text: &str, byte_pos: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    text.grapheme_indices(true)
        .find(|(i, _)| *i > byte_pos)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// Truncate a string at a byte boundary that is safe for UTF-8.
///
/// Returns a string slice guaranteed to end on a valid char boundary.
/// If `max_bytes` lands inside a multi-byte character, it backs up to
/// the previous char boundary.
pub fn truncate_bytes(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    // Find the last valid char boundary at or before max_bytes
    let mut boundary = max_bytes;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &text[..boundary]
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;

    #[test]
    fn test_display_width_ascii() {
        assert_eq!(display_width("Hello"), 5);
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn test_display_width_thai() {
        // Thai characters are typically 1 column wide
        // Note: unicode-segmentation treats these as 4 graphemes, not 5
        assert_eq!(display_width("สวัสดี"), 4); // 4 graphemes
        assert_eq!(display_width("เขียน"), 4); // 4 graphemes
    }

    #[test]
    fn test_display_width_emoji() {
        // Most emoji are 2 columns wide
        assert_eq!(display_width("🌍"), 2);
        assert_eq!(display_width("😀"), 2);

        // Family emoji is 2 columns despite being multiple codepoints
        assert_eq!(display_width("👨‍👩‍👧‍👦"), 2);
    }

    #[test]
    fn test_prev_grapheme_boundary() {
        let text = "Hello";
        assert_eq!(prev_grapheme_boundary(text, 5), 4);
        assert_eq!(prev_grapheme_boundary(text, 1), 0);
        assert_eq!(prev_grapheme_boundary(text, 0), 0);

        let thai = "สวัสดี";
        let byte_pos = thai
            .grapheme_indices(true)
            .nth(2)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let prev = prev_grapheme_boundary(thai, byte_pos);
        let expected = thai
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| i)
            .unwrap_or(0);
        assert_eq!(prev, expected);
    }

    #[test]
    fn test_next_grapheme_boundary() {
        let text = "Hello";
        assert_eq!(next_grapheme_boundary(text, 0), 1);
        assert_eq!(next_grapheme_boundary(text, 4), 5);
        assert_eq!(next_grapheme_boundary(text, 5), 5);

        let thai = "สวัสดี";
        let byte_pos = thai
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let next = next_grapheme_boundary(thai, byte_pos);
        let expected = thai
            .grapheme_indices(true)
            .nth(2)
            .map(|(i, _)| i)
            .unwrap_or(thai.len());
        assert_eq!(next, expected);
    }

    #[test]
    fn test_zero_width_joiners() {
        // Family emoji: man + ZWJ + woman + ZWJ + girl + ZWJ + boy
        // This is 1 grapheme despite being 7 codepoints
        let family = "👨‍👩‍👧‍👦";
        // Family emoji should have display width >= 2
        assert!(display_width(family) >= 2);
    }

    #[test]
    fn test_combining_diacritics() {
        // 'é' can be represented as 'e' + combining acute accent
        let combined = "e\u{0301}"; // e + combining acute
        assert_eq!(display_width(combined), 1);
    }

    #[test]
    fn test_truncate_bytes_ascii() {
        assert_eq!(truncate_bytes("Hello World", 5), "Hello");
        assert_eq!(truncate_bytes("Hi", 10), "Hi");
        assert_eq!(truncate_bytes("", 5), "");
    }

    #[test]
    fn test_truncate_bytes_multibyte() {
        // "สวัสดี" is 6 code points, each 3 bytes = 18 bytes total
        let thai = "สวัสดี";
        // Truncate at 7 bytes — lands inside char, backs up to 6 = "สว"
        let truncated = truncate_bytes(thai, 7);
        assert_eq!(truncated, "สว");
        // At exact boundary
        assert_eq!(truncate_bytes(thai, 6), "สว");
        assert_eq!(truncate_bytes(thai, 9), "สวั");
        assert_eq!(truncate_bytes(thai, 18), "สวัสดี");
    }
}
