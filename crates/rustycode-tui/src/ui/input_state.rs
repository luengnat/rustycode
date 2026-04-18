//! Input state management for multi-line input handling.
//!
//! This module provides the core state types for input management:
//! - Input mode states (single-line vs multi-line)
//! - Complete input state with cursor tracking
//! - Image attachment metadata

use crate::unicode::{display_width, next_grapheme_boundary, prev_grapheme_boundary};
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation;

// ── Input Mode States ───────────────────────────────────────────────────────

/// Input mode state
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub enum InputMode {
    /// Single-line mode (default)
    /// - Enter: Send message
    /// - Option+Enter: Insert newline, switch to MultiLine
    #[default]
    SingleLine,

    /// Multi-line mode
    /// - Enter: Insert newline
    /// - Option+Enter: Send message
    MultiLine,
}

// ── Input State ─────────────────────────────────────────────────────────────

/// Complete input state including text, cursor, and images
#[derive(Clone, Debug, Default)]
pub struct InputState {
    /// Current input mode
    pub mode: InputMode,
    /// Multiple lines for multi-line input
    pub lines: Vec<String>,
    /// Which line we're on (cursor row)
    pub cursor_row: usize,
    /// Position within line (cursor column)
    pub cursor_col: usize,
    /// Pasted images
    pub images: Vec<ImageAttachment>,
}

impl InputState {
    /// Create new input state
    pub fn new() -> Self {
        Self {
            mode: InputMode::SingleLine,
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            images: Vec::new(),
        }
    }

    /// Get current line content
    pub fn current_line(&self) -> String {
        self.lines.get(self.cursor_row).cloned().unwrap_or_default()
    }

    /// Get all text as single string
    pub fn all_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Check if input is empty (no text content)
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.is_empty())
    }

    /// Get cursor position in display columns (for rendering)
    pub fn cursor_display_col(&self) -> usize {
        let text = self
            .lines
            .get(self.cursor_row)
            .map(|s| s.as_str())
            .unwrap_or("");
        display_width(&text[..self.cursor_col.min(text.len())])
    }

    /// Total display width of current line
    pub fn line_display_width(&self) -> usize {
        self.lines
            .get(self.cursor_row)
            .map(|line| display_width(line))
            .unwrap_or(0)
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            if self.cursor_col <= line.len() {
                line.insert(self.cursor_col, c);
                self.cursor_col += c.len_utf8();
            }
        }
    }

    /// Insert a string at the current cursor position
    pub fn insert_string(&mut self, s: &str) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            if self.cursor_col <= line.len() {
                line.insert_str(self.cursor_col, s);
                self.cursor_col += s.len();
            }
        }
    }

    /// Delete character before cursor (backspace)
    ///
    /// This now properly deletes entire grapheme clusters, not just bytes.
    /// For Thai text, this means deleting consonant + vowel combinations as one unit.
    pub fn backspace(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            if self.cursor_col > 0 {
                // Find the previous grapheme boundary
                let prev_boundary = prev_grapheme_boundary(line, self.cursor_col);

                // Remove the entire grapheme cluster (byte range from prev_boundary to cursor)
                line.replace_range(prev_boundary..self.cursor_col, "");

                // Move cursor to previous grapheme position
                self.cursor_col = prev_boundary;
            } else if self.cursor_row > 0 && self.mode == InputMode::MultiLine {
                // Join with previous line
                let prev_line = self.lines.remove(self.cursor_row);
                self.cursor_row -= 1;
                self.cursor_col = self.lines[self.cursor_row].len();
                self.lines[self.cursor_row].push_str(&prev_line);
            }
        }
    }

    /// Delete character at cursor (delete key)
    ///
    /// This now properly deletes entire grapheme clusters, not just bytes.
    pub fn delete(&mut self) {
        let line_len = self.lines.get(self.cursor_row).map_or(0, |l| l.len());

        if self.cursor_col < line_len {
            // Delete within current line
            if let Some(line) = self.lines.get_mut(self.cursor_row) {
                // Find the next grapheme boundary
                let next_boundary = next_grapheme_boundary(line, self.cursor_col);

                // Remove characters from cursor to next boundary
                let end = next_boundary.min(line.len());
                line.drain(self.cursor_col..end);
            }
        } else if self.cursor_row + 1 < self.lines.len() && self.mode == InputMode::MultiLine {
            // Join with next line - need to be careful with borrow checker
            let next_line = self.lines.remove(self.cursor_row + 1);
            if let Some(line) = self.lines.get_mut(self.cursor_row) {
                line.push_str(&next_line);
            }
        }
    }

    /// Delete word backward (Ctrl+Backspace)
    ///
    /// Deletes from the cursor back to the start of the current word.
    /// A word is defined as a sequence of alphanumeric characters or underscores.
    pub fn delete_word_backward(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            if self.cursor_col > 0 {
                // Find the start of the current word
                let text_before = &line[..self.cursor_col];

                // Find the last whitespace or word boundary
                let mut start_pos = 0;
                let mut found_non_ws = false;

                for (i, c) in text_before.char_indices() {
                    if c.is_whitespace() {
                        if found_non_ws {
                            // Found end of previous word
                            start_pos = i;
                        }
                    } else {
                        if !found_non_ws {
                            // Found start of current word
                            start_pos = i;
                        }
                        found_non_ws = true;
                    }
                }

                // Remove from word start to cursor
                line.replace_range(start_pos..self.cursor_col, "");
                self.cursor_col = start_pos;
            }
        }
    }

    /// Delete word forward (Ctrl+Delete)
    ///
    /// Deletes from the cursor to the end of the current word.
    /// A word is defined as a sequence of alphanumeric characters or underscores.
    pub fn delete_word_forward(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            let text_after = &line[self.cursor_col..];

            // Find the end of the current word
            let mut end_pos = self.cursor_col;

            for (offset, c) in text_after.char_indices() {
                if c.is_whitespace() {
                    break;
                }
                end_pos = self.cursor_col + offset + c.len_utf8();
            }

            // Remove from cursor to word end
            line.drain(self.cursor_col..end_pos);
        }
    }

    /// Move cursor left by one grapheme cluster
    ///
    /// This properly handles Thai characters, emoji, and other multi-codepoint graphemes.
    pub fn move_cursor_left(&mut self) {
        if let Some(line) = self.lines.get(self.cursor_row) {
            if self.cursor_col > 0 {
                // Move to previous grapheme boundary
                self.cursor_col = prev_grapheme_boundary(line, self.cursor_col);
            }
        }
    }

    /// Move cursor right by one grapheme cluster
    ///
    /// This properly handles Thai characters, emoji, and other multi-codepoint graphemes.
    pub fn move_cursor_right(&mut self) {
        if let Some(line) = self.lines.get(self.cursor_row) {
            if self.cursor_col < line.len() {
                // Move to next grapheme boundary
                self.cursor_col = next_grapheme_boundary(line, self.cursor_col);
            }
        }
    }

    /// Move cursor up (multi-line mode)
    ///
    /// Preserves visual column position when possible, using display width.
    pub fn move_cursor_up(&mut self) {
        if self.cursor_row > 0 {
            // Get current display column
            let current_display_col = self.cursor_display_col();

            self.cursor_row -= 1;

            // Try to preserve display column position
            if let Some(line) = self.lines.get(self.cursor_row) {
                // Find the byte position that gives us the closest display column
                let mut best_col = 0;
                let mut best_diff = usize::MAX;

                for (i, _) in line.grapheme_indices(true) {
                    let display_col = display_width(&line[..i]);
                    let diff = display_col.abs_diff(current_display_col);

                    if diff < best_diff {
                        best_diff = diff;
                        best_col = i;
                    }

                    // Stop if we've gone past the target
                    if display_col > current_display_col {
                        break;
                    }
                }

                self.cursor_col = best_col;
            }
        }
    }

    /// Move cursor down (multi-line mode)
    ///
    /// Preserves visual column position when possible, using display width.
    pub fn move_cursor_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            // Get current display column
            let current_display_col = self.cursor_display_col();

            self.cursor_row += 1;

            // Try to preserve display column position
            if let Some(line) = self.lines.get(self.cursor_row) {
                // Find the byte position that gives us the closest display column
                let mut best_col = 0;
                let mut best_diff = usize::MAX;

                for (i, _) in line.grapheme_indices(true) {
                    let display_col = display_width(&line[..i]);
                    let diff = display_col.abs_diff(current_display_col);

                    if diff < best_diff {
                        best_diff = diff;
                        best_col = i;
                    }

                    // Stop if we've gone past the target
                    if display_col > current_display_col {
                        break;
                    }
                }

                self.cursor_col = best_col;
            }
        }
    }

    /// Clear all input and cleanup temp files
    pub fn clear(&mut self) {
        self.mode = InputMode::SingleLine;
        self.lines = vec![String::new()];
        self.cursor_row = 0;
        self.cursor_col = 0;

        // Cleanup temp image files
        for img in &self.images {
            if let Err(e) = std::fs::remove_file(&img.path) {
                tracing::warn!("Failed to remove temp image file {:?}: {}", img.path, e);
            }
        }

        self.images.clear();
    }

    /// Set text content, replacing all current content
    ///
    /// Handles multi-line content by splitting on newlines.
    /// Cursor moves to the end of the last line.
    pub fn set_text(&mut self, text: &str) {
        if text.contains('\n') {
            self.mode = InputMode::MultiLine;
            self.lines = text.lines().map(|s| s.to_string()).collect();
            if self.lines.is_empty() {
                self.lines = vec![String::new()];
            }
            self.cursor_row = self.lines.len() - 1;
            self.cursor_col = self.lines.last().map(|l| l.len()).unwrap_or(0);
        } else {
            self.mode = InputMode::SingleLine;
            self.lines = vec![text.to_string()];
            self.cursor_row = 0;
            self.cursor_col = text.len();
        }
    }

    /// Insert newline at cursor position
    ///
    /// Splits the current line at the cursor position, preserving grapheme boundaries.
    pub fn insert_newline(&mut self) {
        if self.cursor_row < self.lines.len() {
            let current_line = &mut self.lines[self.cursor_row];

            // Ensure cursor is at a valid position
            if self.cursor_col > current_line.len() {
                self.cursor_col = current_line.len();
            }

            // Split at grapheme boundary
            let before = current_line[..self.cursor_col].to_string();
            let after = current_line[self.cursor_col..].to_string();

            *current_line = before;
            self.lines.insert(self.cursor_row + 1, after);
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    /// Collapse multi-line to single line
    ///
    /// Joins all lines with spaces, placing cursor at the end.
    pub fn flatten_to_single_line(&mut self) {
        let single = self.lines.join(" ");
        self.lines = vec![single];
        self.cursor_row = 0;
        self.cursor_col = self.lines[0].len();
    }

    /// Remove image by ID and cleanup temp file
    pub fn remove_image(&mut self, id: &str) -> bool {
        if let Some(pos) = self.images.iter().position(|img| img.id == id) {
            let img = self.images.remove(pos);

            // Cleanup temp file
            if let Err(e) = std::fs::remove_file(&img.path) {
                tracing::warn!("Failed to remove temp image file {:?}: {}", img.path, e);
            }

            true
        } else {
            false
        }
    }

    /// Cleanup all temp files (call on exit)
    pub fn cleanup(&mut self) {
        for img in &self.images {
            if let Err(e) = std::fs::remove_file(&img.path) {
                tracing::warn!("Failed to remove temp image file {:?}: {}", img.path, e);
            }
        }
        self.images.clear();
    }
}

// ── Image Attachments ─────────────────────────────────────────────────────────

/// Image attachment metadata
#[derive(Clone, Debug)]
pub struct ImageAttachment {
    /// Unique identifier
    pub id: String,
    /// Temp file path
    pub path: PathBuf,
    /// ASCII preview (24x6 chars)
    pub preview: String,
    /// MIME type
    pub mime_type: String,
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_state_new() {
        let state = InputState::new();
        assert_eq!(state.mode, InputMode::SingleLine);
        assert_eq!(state.lines.len(), 1);
        assert_eq!(state.lines[0], "");
        assert_eq!(state.cursor_row, 0);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_insert_char() {
        let mut state = InputState::new();
        state.insert_char('H');
        state.insert_char('i');
        assert_eq!(state.lines[0], "Hi");
        assert_eq!(state.cursor_col, 2);
    }

    #[test]
    fn test_backspace() {
        let mut state = InputState::new();
        state.lines[0] = "Hello".to_string();
        state.cursor_col = 5;
        state.backspace();
        assert_eq!(state.lines[0], "Hell");
        assert_eq!(state.cursor_col, 4);
    }

    #[test]
    fn test_insert_newline() {
        let mut state = InputState::new();
        state.lines[0] = "Hello World".to_string();
        state.cursor_col = 5;
        state.insert_newline();
        assert_eq!(state.lines.len(), 2);
        assert_eq!(state.lines[0], "Hello");
        assert_eq!(state.lines[1], " World");
        assert_eq!(state.cursor_row, 1);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_flatten_to_single_line() {
        let mut state = InputState::new();
        state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];
        state.flatten_to_single_line();
        assert_eq!(state.lines.len(), 1);
        assert_eq!(state.lines[0], "Line 1 Line 2");
        assert_eq!(state.cursor_row, 0);
    }

    #[test]
    fn test_multiline_navigation() {
        let mut state = InputState::new();
        state.mode = InputMode::MultiLine;
        state.lines = vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
        ];
        state.cursor_row = 1;
        state.cursor_col = 3;

        state.move_cursor_up();
        assert_eq!(state.cursor_row, 0);
        assert_eq!(state.cursor_col, 3); // Clamped to line length

        state.move_cursor_down();
        assert_eq!(state.cursor_row, 1);

        state.move_cursor_down();
        assert_eq!(state.cursor_row, 2);

        // Can't go past end
        state.move_cursor_down();
        assert_eq!(state.cursor_row, 2);
    }

    #[test]
    fn test_clear() {
        let mut state = InputState::new();
        state.mode = InputMode::MultiLine;
        state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];
        state.cursor_row = 1;
        state.cursor_col = 3;
        state.images.push(ImageAttachment {
            id: "test".to_string(),
            path: PathBuf::from("/tmp/test.png"),
            preview: "preview".to_string(),
            mime_type: "image/png".to_string(),
        });

        state.clear();

        assert_eq!(state.mode, InputMode::SingleLine);
        assert_eq!(state.lines.len(), 1);
        assert_eq!(state.lines[0], "");
        assert_eq!(state.cursor_row, 0);
        assert_eq!(state.cursor_col, 0);
        assert_eq!(state.images.len(), 0);
    }

    #[test]
    fn test_remove_image() {
        let mut state = InputState::new();
        state.images.push(ImageAttachment {
            id: "img1".to_string(),
            path: PathBuf::from("/tmp/img1.png"),
            preview: "preview1".to_string(),
            mime_type: "image/png".to_string(),
        });
        state.images.push(ImageAttachment {
            id: "img2".to_string(),
            path: PathBuf::from("/tmp/img2.png"),
            preview: "preview2".to_string(),
            mime_type: "image/png".to_string(),
        });

        assert!(state.remove_image("img1"));
        assert_eq!(state.images.len(), 1);
        assert_eq!(state.images[0].id, "img2");

        assert!(!state.remove_image("nonexistent"));
        assert_eq!(state.images.len(), 1);
    }

    #[test]
    fn test_all_text() {
        let mut state = InputState::new();
        state.lines = vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
        ];
        assert_eq!(state.all_text(), "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_current_line() {
        let mut state = InputState::new();
        state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];
        state.cursor_row = 1;
        assert_eq!(state.current_line(), "Line 2");
    }

    #[test]
    fn test_move_cursor_left_right() {
        let mut state = InputState::new();
        state.lines[0] = "Hello".to_string();
        state.cursor_col = 5;

        state.move_cursor_left();
        assert_eq!(state.cursor_col, 4);

        state.move_cursor_right();
        assert_eq!(state.cursor_col, 5);

        // Can't go past end
        state.move_cursor_right();
        assert_eq!(state.cursor_col, 5);

        // Can't go past start
        state.cursor_col = 0;
        state.move_cursor_left();
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn test_delete() {
        let mut state = InputState::new();
        state.lines[0] = "Hello".to_string();
        state.cursor_col = 1;

        state.delete();
        assert_eq!(state.lines[0], "Hllo");
        assert_eq!(state.cursor_col, 1);
    }

    #[test]
    fn test_backspace_thai() {
        let mut state = InputState::new();
        // Thai greeting: สวัสดี (sawatdee)
        // Unicode treats this as 4 graphemes: ส, วั (ว + combining vowel), ส, ดี (ด + combining vowel)
        state.lines[0] = "สวัสดี".to_string();
        state.cursor_col = state.lines[0].len();

        // Delete last Thai grapheme cluster (ดี = consonant ด + vowel ี)
        state.backspace();
        // Result should be สวัส (3 graphemes)
        assert_eq!(state.lines[0], "สวัส");
        assert_eq!(state.cursor_col, "สวัส".len());
    }

    #[test]
    fn test_delete_thai() {
        let mut state = InputState::new();
        // Thai greeting: สวัสดี (sawatdee)
        state.lines[0] = "สวัสดี".to_string();
        state.cursor_col = 0;

        // Delete first Thai character (should delete entire grapheme)
        state.delete();
        assert_eq!(state.lines[0], "วัสดี");
        assert_eq!(state.cursor_col, 0);
    }
}
