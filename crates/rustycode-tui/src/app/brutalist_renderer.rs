//! Brutalist TUI rendering — distinctive, asymmetric, raw
//!
//! Design philosophy:
//! - Heavy left border for visual anchor
//! - Light separators for structure without clutter
//! - Single-character margins for maximum content space
//! - Lowercase typography for modern feel
//! - Inline tool display instead of wasted panel

use crate::app::context_usage::ContextUsage;
use crate::app::thinking_messages;
use crate::theme::ThemeColors;
use crate::ui::input::InputMode;
use crate::ui::message::{ExpansionLevel, Message, MessageRole, ToolExecution, ToolStatus};
use crate::ui::message_search::MatchPosition;
use chrono::Utc;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;

use crate::app::brutalist_helpers::{
    count_consecutive, extract_tool_key_param, find_byte, find_byte_pair, find_consecutive,
    format_elapsed_short, format_tokens_compact, shorten_tool_param, tool_type_icon,
};

/// Maximum lines to show in a code block before truncating
const MAX_CODE_BLOCK_LINES: usize = 50;

/// Configuration for BrutalistRenderer construction
///
/// Use `BrutalistRendererBuilder` to construct this ergonomically.
#[derive(Default)]
pub struct BrutalistRendererConfig<'a> {
    pub messages: &'a [Message],
    pub current_stream_content: &'a str,
    pub is_streaming: bool,
    pub scroll_offset_line: usize,
    pub user_scrolled: bool,
    pub selected_message: usize,
    pub viewport_height: usize,
    pub theme_colors: Option<Arc<Mutex<ThemeColors>>>,
    pub agent_status: &'a str,
    pub auto_memory_status: &'a str,
    pub input_mode: InputMode,
    pub rate_limit_until: Option<Instant>,
    pub chunks_received: usize,
    pub animation_frame: usize,
    pub input_text: &'a str,
    pub context_usage: ContextUsage,
    pub active_tool_count: usize,
    pub active_tool_names: String,
    pub session_cost: f64,
    pub api_key_warning: String,
    pub header_collapsed: bool,
    pub footer_collapsed: bool,
    pub input_line_count: usize,
    pub has_queued_message: bool,
    /// Preview text of the queued message (shown during streaming)
    pub queued_message_preview: String,
    pub last_response_duration: Option<Duration>,
    pub stream_elapsed: Option<Duration>,
    pub current_model: &'a str,
    pub session_input_tokens: usize,
    pub session_output_tokens: usize,
    pub git_branch: &'a str,
    pub reverse_search_query: String,
    pub reverse_search_match: usize,
    pub reverse_search_total: usize,
    pub history_position: usize,
    pub history_total: usize,
    pub search_query: String,
    pub search_matches: Vec<MatchPosition>,
    pub search_current_match_index: usize,
    pub session_start: Option<std::time::Instant>,
    /// Cursor column position within input text (0-indexed, for cursor rendering)
    pub cursor_col: usize,
    /// Cursor row position within input text (0-indexed, for multiline cursor)
    pub cursor_row: usize,
    /// Current working directory (for display)
    pub cwd: std::path::PathBuf,
}

/// Builder for BrutalistRendererConfig
pub struct BrutalistRendererBuilder<'a> {
    config: BrutalistRendererConfig<'a>,
}

impl<'a> BrutalistRendererBuilder<'a> {
    pub fn new(messages: &'a [Message], input_text: &'a str) -> Self {
        Self {
            config: BrutalistRendererConfig {
                messages,
                input_text,
                ..Default::default()
            },
        }
    }

    pub fn stream_content(mut self, content: &'a str) -> Self {
        self.config.current_stream_content = content;
        self
    }

    pub fn is_streaming(mut self, streaming: bool) -> Self {
        self.config.is_streaming = streaming;
        self
    }

    pub fn scroll(mut self, offset: usize, user_scrolled: bool) -> Self {
        self.config.scroll_offset_line = offset;
        self.config.user_scrolled = user_scrolled;
        self
    }

    pub fn selection(mut self, selected: usize, viewport: usize) -> Self {
        self.config.selected_message = selected;
        self.config.viewport_height = viewport;
        self
    }

    pub fn theme(mut self, colors: Arc<Mutex<ThemeColors>>) -> Self {
        self.config.theme_colors = Some(colors);
        self
    }

    pub fn statuses(mut self, agent: &'a str, auto_memory: &'a str) -> Self {
        self.config.agent_status = agent;
        self.config.auto_memory_status = auto_memory;
        self
    }

    pub fn input_mode(mut self, mode: InputMode) -> Self {
        self.config.input_mode = mode;
        self
    }

    pub fn rate_limit(mut self, until: Option<Instant>) -> Self {
        self.config.rate_limit_until = until;
        self
    }

    pub fn streaming_state(mut self, chunks: usize, frame: usize) -> Self {
        self.config.chunks_received = chunks;
        self.config.animation_frame = frame;
        self
    }

    pub fn context_usage(mut self, usage: ContextUsage) -> Self {
        self.config.context_usage = usage;
        self
    }

    pub fn tool_status(mut self, count: usize, names: String) -> Self {
        self.config.active_tool_count = count;
        self.config.active_tool_names = names;
        self
    }

    pub fn session_info(
        mut self,
        cost: f64,
        input_tokens: usize,
        output_tokens: usize,
        model: &'a str,
    ) -> Self {
        self.config.session_cost = cost;
        self.config.session_input_tokens = input_tokens;
        self.config.session_output_tokens = output_tokens;
        self.config.current_model = model;
        self
    }

    pub fn warnings(mut self, api_key: String) -> Self {
        self.config.api_key_warning = api_key;
        self
    }

    pub fn collapsed(mut self, header: bool, footer: bool) -> Self {
        self.config.header_collapsed = header;
        self.config.footer_collapsed = footer;
        self
    }

    pub fn input_state(
        mut self,
        line_count: usize,
        has_queued: bool,
        queued_preview: String,
    ) -> Self {
        self.config.input_line_count = line_count;
        self.config.has_queued_message = has_queued;
        self.config.queued_message_preview = queued_preview;
        self
    }

    pub fn timing(
        mut self,
        last_response: Option<Duration>,
        stream_elapsed: Option<Duration>,
    ) -> Self {
        self.config.last_response_duration = last_response;
        self.config.stream_elapsed = stream_elapsed;
        self
    }

    pub fn git_branch(mut self, branch: &'a str) -> Self {
        self.config.git_branch = branch;
        self
    }

    pub fn reverse_search(mut self, query: String, match_idx: usize, total: usize) -> Self {
        self.config.reverse_search_query = query;
        self.config.reverse_search_match = match_idx;
        self.config.reverse_search_total = total;
        self
    }

    pub fn history_browsing(mut self, position: usize, total: usize) -> Self {
        self.config.history_position = position;
        self.config.history_total = total;
        self
    }

    pub fn search(
        mut self,
        query: String,
        matches: Vec<MatchPosition>,
        current_match: usize,
    ) -> Self {
        self.config.search_query = query;
        self.config.search_matches = matches;
        self.config.search_current_match_index = current_match;
        self
    }

    pub fn session_start(mut self, start: Option<std::time::Instant>) -> Self {
        self.config.session_start = start;
        self
    }

    pub fn cursor_position(mut self, col: usize, row: usize) -> Self {
        self.config.cursor_col = col;
        self.config.cursor_row = row;
        self
    }

    pub fn cwd(mut self, cwd: std::path::PathBuf) -> Self {
        self.config.cwd = cwd;
        self
    }

    pub fn build(self) -> BrutalistRenderer<'a> {
        BrutalistRenderer::from_config(self.config)
    }
}

/// Brutalist renderer for distinctive TUI appearance
pub struct BrutalistRenderer<'a> {
    /// Messages to display
    pub messages: &'a [Message],
    /// Current stream content
    pub current_stream_content: &'a str,
    /// Whether currently streaming
    pub is_streaming: bool,
    /// Scroll offset (line-based)
    pub scroll_offset_line: usize,
    /// Whether user has manually scrolled (disables auto-scroll)
    pub user_scrolled: bool,
    /// Selected message index
    pub selected_message: usize,
    /// Viewport height
    pub viewport_height: usize,
    /// Theme colors
    pub theme_colors: Arc<Mutex<ThemeColors>>,
    /// Agent status
    pub agent_status: &'a str,
    /// Auto-memory status
    pub auto_memory_status: &'a str,
    /// Current input mode
    pub input_mode: InputMode,
    /// Rate limit until time
    pub rate_limit_until: Option<std::time::Instant>,
    /// Chunks received (for streaming animation)
    pub chunks_received: usize,
    /// Animation frame (for streaming pulse)
    pub animation_frame: usize,
    /// Current input text
    pub input_text: &'a str,
    /// Context usage tracking (token counts)
    pub context_usage: ContextUsage,
    /// Number of active/running tools
    pub active_tool_count: usize,
    /// Comma-separated names of active tools (for status bar, capped at 3)
    pub active_tool_names: String,
    /// Session cost in USD
    pub session_cost: f64,
    /// Pre-computed API key warning (empty if no warning needed)
    pub api_key_warning: String,
    /// Whether header/status bar is collapsed (Ctrl+Shift+H toggle)
    pub header_collapsed: bool,
    /// Whether footer is collapsed (Ctrl+Shift+H toggle)
    pub footer_collapsed: bool,
    /// Number of input content lines (for dynamic input area sizing)
    pub input_line_count: usize,
    /// Whether a message is queued for sending after stream completes
    pub has_queued_message: bool,
    /// Preview text of the queued message (shown during streaming)
    pub queued_message_preview: String,
    /// Duration of the last completed response (Goose pattern: response timing)
    pub last_response_duration: Option<std::time::Duration>,
    /// Elapsed time since stream started (for live timing during streaming)
    pub stream_elapsed: Option<std::time::Duration>,
    /// Current model name (for header display, Goose pattern)
    pub current_model: &'a str,
    /// Input tokens used this session (for context bar split display)
    pub session_input_tokens: usize,
    /// Output tokens used this session (for context bar split display)
    pub session_output_tokens: usize,
    /// Cached git branch name (avoids running git rev-parse per frame)
    pub git_branch: &'a str,
    /// Reverse search query (empty when not in reverse search mode)
    pub reverse_search_query: String,
    /// Reverse search match position (1-indexed, 0 when not searching)
    pub reverse_search_match: usize,
    /// Reverse search total matches (0 when not searching)
    pub reverse_search_total: usize,
    /// History browsing position (0 when not browsing, 1-indexed)
    pub history_position: usize,
    /// Total history items (0 when not browsing)
    pub history_total: usize,
    /// Active search query (empty when search not visible)
    pub search_query: String,
    /// Search match positions for highlighting
    pub search_matches: Vec<crate::ui::message_search::MatchPosition>,
    /// Current search match index (for current-match highlighting)
    pub search_current_match_index: usize,
    /// Session start time (for duration display)
    pub session_start: Option<std::time::Instant>,
    /// Cursor column position within input text (0-indexed)
    pub cursor_col: usize,
    /// Cursor row position within input text (0-indexed)
    pub cursor_row: usize,
    /// Current working directory (for display)
    pub cwd: std::path::PathBuf,
}

impl<'a> BrutalistRenderer<'a> {
    /// Create a new brutalist renderer from configuration
    fn from_config(config: BrutalistRendererConfig<'a>) -> Self {
        Self {
            messages: config.messages,
            current_stream_content: config.current_stream_content,
            is_streaming: config.is_streaming,
            scroll_offset_line: config.scroll_offset_line,
            user_scrolled: config.user_scrolled,
            selected_message: config.selected_message,
            viewport_height: config.viewport_height,
            theme_colors: config
                .theme_colors
                .expect("brutalist renderer requires theme_colors to be initialized before render"),
            agent_status: config.agent_status,
            auto_memory_status: config.auto_memory_status,
            input_mode: config.input_mode,
            rate_limit_until: config.rate_limit_until,
            chunks_received: config.chunks_received,
            animation_frame: config.animation_frame,
            input_text: config.input_text,
            context_usage: config.context_usage,
            active_tool_count: config.active_tool_count,
            active_tool_names: config.active_tool_names,
            session_cost: config.session_cost,
            api_key_warning: config.api_key_warning,
            header_collapsed: config.header_collapsed,
            footer_collapsed: config.footer_collapsed,
            input_line_count: config.input_line_count,
            has_queued_message: config.has_queued_message,
            queued_message_preview: config.queued_message_preview,
            last_response_duration: config.last_response_duration,
            stream_elapsed: config.stream_elapsed,
            current_model: config.current_model,
            session_input_tokens: config.session_input_tokens,
            session_output_tokens: config.session_output_tokens,
            git_branch: config.git_branch,
            reverse_search_query: config.reverse_search_query,
            reverse_search_match: config.reverse_search_match,
            reverse_search_total: config.reverse_search_total,
            history_position: config.history_position,
            history_total: config.history_total,
            search_query: config.search_query,
            search_matches: config.search_matches,
            search_current_match_index: config.search_current_match_index,
            session_start: config.session_start,
            cursor_col: config.cursor_col,
            cursor_row: config.cursor_row,
            cwd: config.cwd,
        }
    }

    /// Compute a map from message index to chain position.
    ///
    /// A "chain" is 2+ consecutive assistant messages where content is empty
    /// (tool-only). Returns a map: message_index → (is_chained, is_last_in_chain).
    ///
    /// Chained messages get a minimal continuation marker instead of the full
    /// "▐ ai (HH:MM)" header. The last message in a chain gets the full header.
    fn compute_tool_chain_map(&self) -> std::collections::HashMap<usize, (bool, bool)> {
        let mut map = std::collections::HashMap::new();
        let msgs = &self.messages;

        if msgs.len() < 2 {
            return map;
        }

        // Identify chain boundaries: consecutive assistant messages with empty content
        let mut chain_start: Option<usize> = None;
        let mut chain_len: usize = 0;

        for (i, msg) in msgs.iter().enumerate() {
            let is_tool_only = msg.role == MessageRole::Assistant
                && msg.content.trim().is_empty()
                && msg.tool_executions.as_ref().is_some_and(|t| !t.is_empty());

            if is_tool_only {
                if chain_start.is_none() {
                    chain_start = Some(i);
                }
                chain_len += 1;
            } else {
                // End current chain if any
                if chain_len >= 2 {
                    if let Some(start) = chain_start {
                        for j in start..start + chain_len {
                            let is_last = j == start + chain_len - 1;
                            map.insert(j, (true, is_last));
                        }
                    }
                }
                chain_start = None;
                chain_len = 0;
            }
        }

        // Handle trailing chain
        if chain_len >= 2 {
            if let Some(start) = chain_start {
                for j in start..start + chain_len {
                    let is_last = j == start + chain_len - 1;
                    map.insert(j, (true, is_last));
                }
            }
        }

        map
    }

    /// Apply search highlighting to spans for a given message.
    ///
    /// Highlights matching text with a yellow background for current match
    /// and a dim yellow for other matches. Returns new spans with highlights.
    fn apply_search_highlight<'b>(
        &self,
        spans: Vec<Span<'b>>,
        message_index: usize,
        byte_offset_start: usize,
    ) -> Vec<Span<'b>> {
        if self.search_query.is_empty() || self.search_matches.is_empty() {
            return spans;
        }

        // Collect matches for this message, capped for performance
        let matches: Vec<&MatchPosition> = self
            .search_matches
            .iter()
            .filter(|m| m.message_index == message_index)
            .take(50)
            .collect();

        if matches.is_empty() {
            return spans;
        }

        let current_match = self.search_matches.get(self.search_current_match_index);
        let mut result = Vec::with_capacity(spans.len() * 2);
        let mut byte_offset = byte_offset_start;

        for span in spans {
            let span_text = span.content.as_ref();
            let span_bytes = span_text.as_bytes();
            let span_len = span_bytes.len();
            let span_end = byte_offset + span_len;

            // Collect overlapping match intervals within this span
            let mut intervals: Vec<(usize, usize, bool)> = Vec::new(); // (start_in_span, end_in_span, is_current)
            for match_pos in &matches {
                if match_pos.end <= byte_offset || match_pos.start >= span_end {
                    continue;
                }
                let start = match_pos.start.saturating_sub(byte_offset);
                let end = (match_pos.end - byte_offset).min(span_len);
                let is_current = current_match == Some(*match_pos);
                intervals.push((start, end, is_current));
            }

            if intervals.is_empty() {
                result.push(span);
                byte_offset = span_end;
                continue;
            }

            // Sort and merge overlapping intervals
            intervals.sort_by_key(|(s, _, _)| *s);
            let mut merged: Vec<(usize, usize, bool)> = Vec::with_capacity(intervals.len());
            for (start, end, is_current) in intervals {
                if let Some(last) = merged.last_mut() {
                    if start <= last.1 {
                        // Overlapping — extend, prefer current
                        last.1 = last.1.max(end);
                        if is_current {
                            last.2 = true;
                        }
                        continue;
                    }
                }
                merged.push((start, end, is_current));
            }

            // Build highlighted spans by splitting at match boundaries
            let mut pos = 0;
            for (start, end, is_current) in &merged {
                // Text before match
                if *start > pos {
                    let before = &span_text
                        [span_text.floor_char_boundary(pos)..span_text.floor_char_boundary(*start)];
                    result.push(Span::styled(before.to_string(), span.style));
                }
                // Match text with highlight
                let match_text = &span_text
                    [span_text.floor_char_boundary(*start)..span_text.floor_char_boundary(*end)];
                let highlight_style = if *is_current {
                    Style::default()
                        .fg(Color::Rgb(30, 30, 30))
                        .bg(Color::Rgb(255, 220, 80))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(Color::Rgb(220, 200, 80))
                        .bg(Color::Rgb(50, 50, 60))
                };
                result.push(Span::styled(match_text.to_string(), highlight_style));
                pos = *end;
            }
            // Remaining text after last match
            if pos < span_len {
                let after = &span_text[span_text.floor_char_boundary(pos)..];
                result.push(Span::styled(after.to_string(), span.style));
            }

            byte_offset = span_end;
        }

        result
    }

    /// Compute message heights and total line count.
    ///
    /// Returns (total_lines, heights_vec) — used for scroll computation
    /// and click area registration. Avoids recomputing inside render_messages().
    pub fn compute_message_layout(&self, width: usize) -> (usize, Vec<usize>) {
        let mut total_lines: usize = 0;
        let mut heights = Vec::with_capacity(self.messages.len());
        for msg in self.messages {
            let h = self.estimate_message_height(msg, width);
            heights.push(h);
            total_lines += h;
        }
        (total_lines, heights)
    }

    /// Render messages with precomputed heights (avoids redundant estimation).
    ///
    /// Use this when heights have already been computed (e.g., via compute_message_layout)
    /// to avoid estimating heights twice per frame. This is the lower-level rendering
    /// method that renders only the messages area.
    pub fn render_messages_with_heights(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        heights: &[usize],
    ) {
        let colors = self
            .theme_colors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        self.render_messages_with_heights_and_colors(frame, area, heights, &colors);
    }

    /// Internal messages rendering with pre-provided colors (avoids redundant mutex lock).
    fn render_messages_with_heights_and_colors(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        heights: &[usize],
        colors: &ThemeColors,
    ) {
        use ratatui::style::Style;
        use ratatui::widgets::{Block, Paragraph, Wrap};

        // Clear the message area background first
        let bg = Block::default().style(Style::default().bg(colors.background));
        frame.render_widget(bg, area);

        // Show welcome when there's no user/assistant conversation (system messages don't count)
        let has_conversation = self.messages.iter().any(|m| {
            matches!(
                m.role,
                crate::ui::message::MessageRole::User | crate::ui::message::MessageRole::Assistant
            )
        });
        if !has_conversation && !self.is_streaming {
            self.render_welcome(frame, area, colors);
            return;
        }

        let _width = area.width as usize;
        let safe_viewport = (area.height as usize).max(1);

        // Use precomputed heights instead of re-estimating
        let total_lines: usize = heights.iter().sum();

        // Compute effective scroll offset (auto-scroll to bottom when not user-scrolled)
        let max_scroll = total_lines.saturating_sub(safe_viewport);
        let effective_offset = if self.user_scrolled {
            self.scroll_offset_line.min(max_scroll)
        } else {
            max_scroll // Auto-scroll to bottom
        };

        // Calculate visible range — find which message the effective offset falls within
        let mut current_line = 0;
        let mut start_idx = 0;
        let mut skip_lines_in_first = 0;

        for (idx, &msg_height) in heights.iter().enumerate() {
            if current_line + msg_height > effective_offset {
                start_idx = idx;
                skip_lines_in_first = effective_offset.saturating_sub(current_line);
                break;
            }
            current_line += msg_height;
            if idx == self.messages.len() - 1 {
                start_idx = idx;
            }
        }

        let mut y_offset = 0u16;

        // Compute tool chain map for compact rendering of consecutive tool-only messages
        let chain_map = self.compute_tool_chain_map();

        // Render each visible message using precomputed heights
        let mut first_message = true;
        for (rel_idx, msg) in self.messages.iter().skip(start_idx).enumerate() {
            let msg_idx = start_idx + rel_idx;
            let chained = chain_map.get(&msg_idx).copied();
            let msg_lines =
                self.render_message_brutalist(msg, msg_idx, area.width as usize, colors, chained);

            // For the first visible message, skip lines that are above the viewport
            let skip = if first_message {
                skip_lines_in_first
            } else {
                0
            };
            first_message = false;

            for (line_idx, line) in msg_lines.iter().enumerate() {
                if line_idx < skip {
                    continue;
                }
                if y_offset >= area.height {
                    break;
                }

                // Calculate wrapped height for this line
                let line_width = line.width();
                let content_width = (area.width as usize).max(1);
                let wrapped_rows = if line_width == 0 {
                    1u16
                } else {
                    (line_width.div_ceil(content_width) as u16).max(1)
                };

                let remaining = area.height.saturating_sub(y_offset);
                let render_rows = wrapped_rows.min(remaining);

                if render_rows == 0 {
                    break;
                }

                let line_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: render_rows,
                };
                frame.render_widget(
                    Paragraph::new(line.clone()).wrap(Wrap { trim: false }),
                    line_area,
                );
                y_offset += render_rows;
            }

            if y_offset >= area.height {
                break;
            }
        }

        // Messages above indicator at top of viewport
        let is_scrolled = effective_offset > 0;
        if start_idx > 0 && is_scrolled && area.height > 2 {
            let indicator_area = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            };
            let indicator = Paragraph::new(Line::from(vec![Span::styled(
                format!(
                    "  ▲ {} message{} above",
                    start_idx,
                    if start_idx != 1 { "s" } else { "" }
                ),
                Style::default()
                    .fg(Color::Rgb(80, 80, 100))
                    .add_modifier(Modifier::DIM),
            )]));
            frame.render_widget(indicator, indicator_area);
        }

        // Messages below indicator at bottom of viewport
        let messages_below = total_lines.saturating_sub(current_line + y_offset as usize);
        if messages_below > 0 && is_scrolled && area.height > 2 {
            let indicator_y = area.y + area.height.saturating_sub(1);
            let indicator_area = Rect {
                x: area.x,
                y: indicator_y,
                width: area.width,
                height: 1,
            };
            let pulse_color = if self.is_streaming {
                Color::Rgb(80, 200, 220)
            } else {
                Color::Rgb(80, 80, 100)
            };
            let indicator = Paragraph::new(Line::from(vec![Span::styled(
                format!(
                    "  ▼ {} message{} below",
                    messages_below,
                    if messages_below != 1 { "s" } else { "" }
                ),
                Style::default().fg(pulse_color).add_modifier(Modifier::DIM),
            )]));
            frame.render_widget(indicator, indicator_area);
        }

        // Show streaming indicator at the bottom of the messages area.
        if self.is_streaming && y_offset < area.height {
            let colors_inner = &colors;

            // Streaming header with animated indicator and stats
            if y_offset < area.height {
                let header_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: 1,
                };

                let (status_label, label_color) = if self.active_tool_count > 0 {
                    ("tools", Color::Rgb(100, 180, 255))
                } else if self.current_stream_content.is_empty() {
                    ("thinking", colors_inner.primary)
                } else {
                    ("", colors_inner.primary)
                };

                let mut header_spans = vec![
                    Span::styled("❯ ", Style::default().fg(Color::Rgb(220, 80, 100))),
                    Span::styled(
                        status_label,
                        Style::default()
                            .fg(label_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {} ", self.streaming_char()),
                        Style::default().fg(Color::Rgb(255, 200, 80)),
                    ),
                ];
                if let Some(elapsed) = self.stream_elapsed {
                    let secs = elapsed.as_secs();
                    if secs >= 1 {
                        header_spans.push(Span::styled(
                            format!("{} ", format_elapsed_short(secs)),
                            Style::default()
                                .fg(colors_inner.muted)
                                .add_modifier(Modifier::DIM),
                        ));
                    }
                }
                if self.chunks_received > 0 {
                    header_spans.push(Span::styled(
                        format!("· {} chunks ", self.chunks_received),
                        Style::default()
                            .fg(colors_inner.muted)
                            .add_modifier(Modifier::DIM),
                    ));
                }
                // Word count during streaming (Goose pattern: live content stats)
                if !self.current_stream_content.is_empty() {
                    let words = self.current_stream_content.split_whitespace().count();
                    if words > 10 {
                        header_spans.push(Span::styled(
                            format!("· {} words ", words),
                            Style::default()
                                .fg(colors_inner.muted)
                                .add_modifier(Modifier::DIM),
                        ));
                        // Words per second throughput
                        if let Some(elapsed) = self.stream_elapsed {
                            let secs = elapsed.as_secs();
                            if secs >= 3 {
                                let wps = words as f64 / secs as f64;
                                header_spans.push(Span::styled(
                                    format!("({:.0}w/s) ", wps),
                                    Style::default()
                                        .fg(colors_inner.muted)
                                        .add_modifier(Modifier::DIM),
                                ));
                            }
                        }
                    }
                }
                if self.active_tool_count > 0 && !self.active_tool_names.is_empty() {
                    header_spans.push(Span::styled(
                        format!("· {} ", self.active_tool_names),
                        Style::default()
                            .fg(Color::Rgb(100, 180, 255))
                            .add_modifier(Modifier::DIM),
                    ));
                }
                let header = Paragraph::new(Line::from(header_spans));
                frame.render_widget(header, header_area);
                y_offset += 1;
            }

            // Skip preview when message list already shows this text (dedup)
            let last_assistant_has_content = self
                .messages
                .iter()
                .rev()
                .find(|m| m.role == crate::ui::message::MessageRole::Assistant)
                .is_some_and(|m| !m.content.is_empty());
            if !self.current_stream_content.is_empty()
                && y_offset < area.height
                && !last_assistant_has_content
            {
                // Live content preview: show first 2 lines of streaming content
                let preview_lines: Vec<&str> =
                    self.current_stream_content.lines().take(2).collect();
                for preview_line in &preview_lines {
                    if y_offset >= area.height {
                        break;
                    }
                    let truncated = if preview_line.len() > area.width as usize - 4 {
                        let end = (*preview_line).floor_char_boundary(area.width as usize - 5);
                        format!("{}…", &preview_line[..end])
                    } else {
                        preview_line.to_string()
                    };
                    let preview_area = Rect {
                        x: area.x,
                        y: area.y + y_offset,
                        width: area.width,
                        height: 1,
                    };
                    let preview_spans = vec![
                        Span::styled("  ", Style::default().fg(colors_inner.foreground)),
                        Span::styled(truncated, Style::default().fg(Color::Rgb(160, 170, 190))),
                    ];
                    frame.render_widget(Paragraph::new(Line::from(preview_spans)), preview_area);
                    y_offset += 1;
                }
            } else if y_offset < area.height {
                let slow_frame = self.animation_frame / 8;
                let thinking_msg = crate::app::thinking_messages::get_thinking_message(slow_frame);
                let think_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: 1,
                };
                let mut think_spans = vec![
                    Span::styled("  ", Style::default().fg(colors_inner.foreground)),
                    Span::styled(
                        format!("{}...", thinking_msg),
                        Style::default()
                            .fg(Color::Rgb(120, 120, 140))
                            .add_modifier(Modifier::ITALIC),
                    ),
                ];
                if let Some(elapsed) = self.stream_elapsed {
                    let secs = elapsed.as_secs();
                    if secs >= 2 {
                        think_spans.push(Span::styled(
                            format!(" ({} elapsed)", format_elapsed_short(secs)),
                            Style::default()
                                .fg(Color::Rgb(90, 90, 110))
                                .add_modifier(Modifier::DIM),
                        ));
                    }
                }
                let think_line = Paragraph::new(Line::from(think_spans));
                frame.render_widget(think_line, think_area);
            }

            // Show queued message preview (Goose pattern: gold/italic "will send when finished")
            if self.has_queued_message
                && !self.queued_message_preview.is_empty()
                && y_offset < area.height
            {
                let preview: String = self.queued_message_preview.chars().take(60).collect();
                let ellipsis = if self.queued_message_preview.chars().count() > 60 {
                    "…"
                } else {
                    ""
                };
                let queued_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: 1,
                };
                let queued_line = Paragraph::new(Line::from(vec![
                    Span::styled("  ⏳ ", Style::default().fg(Color::Rgb(255, 200, 80))),
                    Span::styled(
                        format!("{}{} — will send when finished", preview, ellipsis),
                        Style::default()
                            .fg(Color::Rgb(180, 160, 100))
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
                frame.render_widget(queued_line, queued_area);
            }
        }
    }

    /// Render with precomputed message heights (avoids redundant estimation).
    ///
    /// Use this when heights have already been computed (e.g., via compute_message_layout)
    /// to avoid estimating heights twice per frame.
    pub fn render_with_heights(&self, frame: &mut ratatui::Frame, heights: &[usize]) {
        use ratatui::layout::{Constraint, Direction, Layout};
        use ratatui::style::{Color, Style};
        use ratatui::widgets::{Block, Clear};

        let size = frame.area();

        frame.render_widget(Clear, size);
        let bg = Block::default().style(Style::default().bg(Color::Black));
        frame.render_widget(bg, size);

        let colors = self
            .theme_colors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        let header_height: u16 = if self.header_collapsed { 0 } else { 1 };
        let footer_height: u16 = if self.footer_collapsed { 0 } else { 1 };

        let input_height: u16 = if self.input_line_count > 1 {
            2u16.saturating_add(self.input_line_count.min(6) as u16)
        } else {
            2
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height),
                Constraint::Min(0),
                Constraint::Length(input_height),
                Constraint::Length(footer_height),
            ])
            .split(size);

        let main_area = chunks[1];
        let input_area = chunks[2];

        if !self.header_collapsed {
            let header = self.render_header(&colors);
            self.render_header_widget(frame, chunks[0], &colors, header);
        }

        self.render_messages_with_heights_and_colors(frame, main_area, heights, &colors);
        self.render_input(frame, input_area, &colors);

        if !self.footer_collapsed {
            self.render_footer(frame, chunks[3], &colors);
        }
    }

    /// Render only the footer component (for integration with existing TUI)
    pub fn render_footer_area(&self, frame: &mut ratatui::Frame, area: Rect) {
        let colors = self
            .theme_colors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        self.render_footer(frame, area, &colors);
    }

    /// Build header line content
    fn render_header(&self, colors: &ThemeColors) -> Line<'a> {
        // Brutalist header: status + turn count + mode
        let mode_indicator = match self.input_mode {
            InputMode::SingleLine => "ins",
            InputMode::MultiLine => "mult",
        };

        // Count user turns for display
        let turn_count = self
            .messages
            .iter()
            .filter(|m| matches!(m.role, MessageRole::User))
            .count();

        // Dynamic status indicator with rotating thinking messages
        let status_str = if self.is_streaming {
            if self.active_tool_count > 0 {
                "tools"
            } else {
                // Rotate through thinking messages using animation frame
                thinking_messages::get_thinking_message(self.chunks_received)
            }
        } else {
            "ready"
        };

        let mut spans = vec![
            Span::styled("╺─", Style::default().fg(colors.muted)),
            Span::styled(
                "RustyCode",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ─", Style::default().fg(colors.muted)),
        ];

        // Status indicator with session health (Goose pattern: session indicators)
        let has_recent_error = self.messages.iter().rev().take(5).any(|m| {
            m.role == MessageRole::System && {
                let c = m.content.to_lowercase();
                c.starts_with("error") || c.contains("failed") || c.contains("rate limit")
            }
        });
        let status_color = if has_recent_error && !self.is_streaming {
            Color::Rgb(255, 100, 100) // Red for error state
        } else if self.is_streaming {
            Color::Rgb(255, 200, 80)
        } else {
            Color::Rgb(80, 200, 120)
        };
        let status_prefix = if has_recent_error && !self.is_streaming {
            "✗ "
        } else {
            ""
        };
        spans.push(Span::styled(
            format!(" {}{} ", status_prefix, status_str),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::DIM),
        ));

        // Turn count
        if turn_count > 0 {
            spans.push(Span::styled(
                format!("│ turn {} ", turn_count),
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Mode indicator
        spans.push(Span::styled(
            format!("│ {} ", mode_indicator),
            Style::default()
                .fg(colors.secondary)
                .add_modifier(Modifier::DIM),
        ));

        // Model name (Goose pattern: show active model in header)
        if !self.current_model.is_empty() {
            let model_short = self
                .current_model
                .rsplit('/')
                .next()
                .unwrap_or(self.current_model);
            spans.push(Span::styled(
                format!("│ {} ", model_short),
                Style::default()
                    .fg(Color::Rgb(120, 140, 180))
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Streaming animation
        if self.is_streaming {
            spans.push(Span::styled(
                self.streaming_char(),
                Style::default().fg(Color::Rgb(255, 200, 80)),
            ));
            // Show live elapsed time during streaming (Goose pattern)
            if let Some(elapsed) = self.stream_elapsed {
                let secs = elapsed.as_secs();
                if secs >= 1 {
                    spans.push(Span::styled(
                        format!(" {}", format_elapsed_short(secs)),
                        Style::default()
                            .fg(Color::Rgb(255, 200, 80))
                            .add_modifier(Modifier::DIM),
                    ));
                }
            }
        } else if let Some(dur) = self.last_response_duration {
            // Show last response time when idle (Goose pattern: response timing)
            let secs = dur.as_secs();
            if secs > 0 {
                spans.push(Span::styled(
                    format!("│ {} ", format_elapsed_short(secs)),
                    Style::default()
                        .fg(colors.muted)
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        // Session duration (Goose pattern: show total session time)
        if let Some(start) = self.session_start {
            let elapsed = start.elapsed().as_secs();
            if elapsed >= 60 {
                spans.push(Span::styled(
                    format!("│ {} ", format_elapsed_short(elapsed)),
                    Style::default()
                        .fg(Color::Rgb(90, 100, 120))
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        Line::from(spans)
    }

    /// Get animated streaming character
    fn streaming_char(&self) -> &'static str {
        // Pulse animation: ◐ ◑ ◒ ◓
        const FRAMES: &[&str] = &["◐", "◑", "◒", "◓"];
        FRAMES[self.animation_frame % FRAMES.len()]
    }

    /// Render header widget
    fn render_header_widget(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        colors: &ThemeColors,
        content: Line<'a>,
    ) {
        use ratatui::layout::Alignment;
        use ratatui::widgets::Paragraph;

        // Fill rest with light separator
        let width = area.width as usize;
        let content_width = content.width();
        let remaining = width.saturating_sub(content_width);

        let mut spans = content.spans.clone();

        if remaining > 0 {
            spans.push(Span::styled(
                "─".repeat(remaining),
                Style::default().fg(colors.muted),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

    /// Render welcome message
    fn render_welcome(&self, frame: &mut ratatui::Frame, area: Rect, colors: &ThemeColors) {
        use ratatui::layout::Alignment;
        use ratatui::widgets::Paragraph;

        // Compact welcome message that fits in small terminals
        // Use vertical centering for better appearance
        let available_height = area.height as usize;

        // Calculate top padding to center the welcome message
        let welcome_lines = 14; // title + spacer + model + cwd + spacer + shortcuts(2) + spacer + instructions + suggestions(5)
        let top_padding = available_height.saturating_sub(welcome_lines) / 2;

        let mut welcome = Vec::new();

        // Add top padding for centering
        for _ in 0..top_padding {
            welcome.push(Line::from(""));
        }

        // Title line
        welcome.push(Line::from(vec![
            Span::styled("  ", Style::default().fg(colors.foreground)),
            Span::styled("╶─ ", Style::default().fg(colors.muted)),
            Span::styled(
                "RustyCode",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " — autonomous development framework",
                Style::default().fg(colors.foreground),
            ),
            Span::styled(" ─╴", Style::default().fg(colors.muted)),
        ]));

        // Spacer
        welcome.push(Line::from(""));

        // Model line (Goose pattern: show active model on welcome)
        if !self.current_model.is_empty() {
            let model_short = self
                .current_model
                .rsplit('/')
                .next()
                .unwrap_or(self.current_model);
            welcome.push(Line::from(vec![
                Span::styled("  ", Style::default().fg(colors.foreground)),
                Span::styled("  model: ", Style::default().fg(colors.muted)),
                Span::styled(
                    model_short.to_string(),
                    Style::default().fg(colors.secondary),
                ),
            ]));
        }

        // Working directory
        {
            let cwd = if self.cwd.as_os_str().is_empty() {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            } else {
                self.cwd.clone()
            };
            let cwd_str = cwd.display().to_string();
            // Replace home dir with ~ for brevity
            let display = if let Ok(home) = std::env::var("HOME") {
                if cwd_str.starts_with(&home) {
                    format!("~{}", &cwd_str[home.len()..])
                } else {
                    cwd_str
                }
            } else {
                cwd_str
            };
            // Truncate long paths
            let path_display = if display.len() > 50 {
                format!("…{}", &display[display.len().saturating_sub(47)..])
            } else {
                display
            };

            // Use cached git branch (computed once when workspace loads)
            let git_branch: Option<&str> = if self.git_branch.is_empty() {
                None
            } else {
                Some(self.git_branch)
            };

            let mut cwd_spans = vec![
                Span::styled("  ", Style::default().fg(colors.foreground)),
                Span::styled("  cwd: ", Style::default().fg(colors.muted)),
                Span::styled(path_display, Style::default().fg(Color::Rgb(140, 150, 170))),
            ];
            if let Some(branch) = git_branch {
                cwd_spans.push(Span::styled(
                    " │ ",
                    Style::default().fg(Color::Rgb(60, 60, 70)),
                ));
                cwd_spans.push(Span::styled("branch: ", Style::default().fg(colors.muted)));
                cwd_spans.push(Span::styled(
                    branch,
                    Style::default().fg(Color::Rgb(100, 180, 140)),
                ));
            }
            welcome.push(Line::from(cwd_spans));
        }

        // Spacer before shortcuts
        welcome.push(Line::from(""));

        // Keyboard shortcuts (kilocode pattern: discoverable shortcuts)
        welcome.push(Line::from(vec![
            Span::styled("  ", Style::default().fg(colors.foreground)),
            Span::styled(
                "  /",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" commands  ", Style::default().fg(colors.muted)),
            Span::styled(
                "?",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" help  ", Style::default().fg(colors.muted)),
            Span::styled(
                "!",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" bash  ", Style::default().fg(colors.muted)),
            Span::styled(
                "Ctrl+K",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" palette", Style::default().fg(colors.muted)),
        ]));

        // Second line of shortcuts
        welcome.push(Line::from(vec![
            Span::styled("  ", Style::default().fg(colors.foreground)),
            Span::styled(
                "  Ctrl+R",
                Style::default()
                    .fg(Color::Rgb(100, 140, 180))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" search  ", Style::default().fg(colors.muted)),
            Span::styled(
                "Ctrl+X",
                Style::default()
                    .fg(Color::Rgb(100, 140, 180))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" editor  ", Style::default().fg(colors.muted)),
            Span::styled(
                "Ctrl+Y",
                Style::default()
                    .fg(Color::Rgb(100, 140, 180))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" copy", Style::default().fg(colors.muted)),
        ]));

        // Third line: streaming tips
        welcome.push(Line::from(vec![
            Span::styled("  ", Style::default().fg(colors.foreground)),
            Span::styled(
                "  type during streaming to queue",
                Style::default()
                    .fg(Color::Rgb(80, 90, 110))
                    .add_modifier(Modifier::DIM),
            ),
        ]));

        // Spacer
        welcome.push(Line::from(""));

        // Instructions line
        welcome.push(Line::from(vec![
            Span::styled("  ", Style::default().fg(colors.foreground)),
            Span::styled(
                "type a message and press ",
                Style::default().fg(colors.muted),
            ),
            Span::styled(
                "enter",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to start", Style::default().fg(colors.muted)),
        ]));

        // Suggested prompts (Goose pattern: popular chat topics on welcome)
        // Only show if there's enough vertical space
        if available_height >= 14 {
            welcome.push(Line::from(""));

            // Pick suggestions based on git branch context
            let suggestions = if self.git_branch == "main" || self.git_branch == "master" {
                &[
                    ("what does this project do?", "understand"),
                    ("find bugs in the codebase", "analyze"),
                    ("list the main modules", "explore"),
                    ("suggest improvements", "review"),
                ][..]
            } else {
                &[
                    ("what changed on this branch?", "diff"),
                    ("summarize recent commits", "log"),
                    ("are there any TODO comments?", "search"),
                    ("run the test suite", "verify"),
                ][..]
            };

            welcome.push(Line::from(vec![
                Span::styled("  ", Style::default().fg(colors.foreground)),
                Span::styled("  try: ", Style::default().fg(colors.muted)),
            ]));

            for (i, (prompt, _category)) in suggestions.iter().enumerate() {
                let num = format!("  {}. ", i + 1);
                welcome.push(Line::from(vec![
                    Span::styled("  ", Style::default().fg(colors.foreground)),
                    Span::styled(
                        num,
                        Style::default()
                            .fg(Color::Rgb(80, 90, 110))
                            .add_modifier(Modifier::DIM),
                    ),
                    Span::styled(
                        *prompt,
                        Style::default()
                            .fg(Color::Rgb(120, 130, 150))
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }
        }

        // Check for missing API key and show warning (pre-computed to avoid per-frame config reads)
        if !self.api_key_warning.is_empty() {
            welcome.push(Line::from(""));
            welcome.push(Line::from(vec![
                Span::styled("  ", Style::default().fg(colors.foreground)),
                Span::styled(
                    self.api_key_warning.to_string(),
                    Style::default().fg(Color::Rgb(255, 200, 80)),
                ),
            ]));
        }

        let paragraph = Paragraph::new(welcome).alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

    /// Render a single message in brutalist style
    fn render_message_brutalist<'b>(
        &self,
        message: &'b Message,
        message_index: usize,
        width: usize,
        colors: &ThemeColors,
        chained: Option<(bool, bool)>,
    ) -> Vec<Line<'b>> {
        let mut lines = Vec::new();

        // System messages: compact for short notices, rich rendering for diffs/long content
        if message.role == MessageRole::System {
            let content = message.content.trim();
            if content.is_empty() {
                return lines;
            }

            // Detect git diff content — render with syntax coloring
            if content.starts_with("diff --git") {
                let diff_lines = crate::ui::diff_renderer::render_diff(content);
                // Wrap each diff line with brutalist indent
                for diff_line in &diff_lines {
                    let mut spans = vec![Span::styled("  ", Style::default().fg(colors.muted))];
                    spans.extend(diff_line.spans.iter().cloned());
                    lines.push(Line::from(spans));
                }
                return lines;
            }

            // Multi-line system messages: render each line (for /diff, /stats, etc.)
            let line_count = content.lines().count();
            if line_count > 1 {
                // Detect error content for header coloring (Goose pattern)
                let content_lower = content.to_lowercase();
                let (header_icon, header_color) = if content_lower.starts_with("error")
                    || content_lower.contains("failed")
                    || content_lower.contains("rate limit")
                {
                    ("✗ ", Color::Rgb(200, 80, 80))
                } else if content_lower.starts_with("warning")
                    || content_lower.contains("cancelled")
                {
                    ("⚠ ", Color::Rgb(200, 170, 80))
                } else {
                    ("─ ", Color::Rgb(50, 50, 60))
                };
                // Header
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default().fg(colors.muted)),
                    Span::styled(header_icon, Style::default().fg(header_color)),
                ]));
                // Content lines with dim styling, capped at 50 lines
                for line in content.lines().take(50) {
                    let truncated = if line.len() > width.saturating_sub(4) {
                        format!(
                            "{}…",
                            &line[..line.floor_char_boundary(width.saturating_sub(5))]
                        )
                    } else {
                        line.to_string()
                    };
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.muted)),
                        Span::styled(
                            truncated,
                            Style::default()
                                .fg(Color::Rgb(120, 120, 140))
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                }
                if line_count > 50 {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.muted)),
                        Span::styled(
                            format!("... {} more lines", line_count.saturating_sub(50)),
                            Style::default()
                                .fg(Color::Rgb(80, 80, 100))
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                }
                return lines;
            }

            // Single-line system message: compact notice with type-based coloring
            let display: String = if content.len() > 100 {
                format!("{}...", &content[..content.floor_char_boundary(97)])
            } else {
                content.to_string()
            };

            // Detect error/warning messages for visual distinction (Goose pattern)
            let content_lower = content.to_lowercase();
            let (prefix, text_color, prefix_color) = if content_lower.starts_with("error")
                || content_lower.contains("failed")
                || content_lower.contains("rate limit")
            {
                ("✗ ", Color::Rgb(200, 80, 80), Color::Rgb(200, 80, 80))
            } else if content_lower.starts_with("warning") || content_lower.contains("cancelled") {
                ("⚠ ", Color::Rgb(200, 170, 80), Color::Rgb(200, 170, 80))
            } else {
                ("─ ", Color::Rgb(100, 100, 120), Color::Rgb(50, 50, 60))
            };

            lines.push(Line::from(vec![
                Span::styled("  ", Style::default().fg(colors.muted)),
                Span::styled(prefix, Style::default().fg(prefix_color)),
                Span::styled(
                    display,
                    Style::default().fg(text_color).add_modifier(Modifier::DIM),
                ),
            ]));
            return lines;
        }

        // Role color for the vertical bar (pink = user, cyan = ai)
        let role_color = match message.role {
            MessageRole::User => colors.secondary,
            MessageRole::Assistant => colors.primary,
            MessageRole::System => unreachable!(), // handled above
        };

        // Tool call chaining: suppress header for chained messages (except last)
        let is_chained_mid = chained.is_some_and(|(is_chained, is_last)| is_chained && !is_last);
        if !is_chained_mid {
            // Just a colored vertical bar — no text label, saves vertical space
            lines.push(Line::from(vec![Span::styled(
                "▐ ",
                Style::default().fg(role_color).add_modifier(Modifier::BOLD),
            )]));
        } else {
            // Minimal continuation marker for chained tool-only messages
            lines.push(Line::from(vec![Span::styled(
                "│ ",
                Style::default().fg(Color::Rgb(60, 60, 70)),
            )]));
        }

        // Collapsed message: show first line + "N more lines" indicator
        if message.collapsed {
            let content_line_count = message.content.lines().count();
            if content_line_count > 0 {
                let first_line = message.content.lines().next().unwrap_or("");
                let preview = if first_line.len() > 60 {
                    format!("{}…", &first_line[..first_line.floor_char_boundary(59)])
                } else {
                    first_line.to_string()
                };
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default().fg(colors.foreground)),
                    Span::styled(
                        preview,
                        Style::default()
                            .fg(colors.muted)
                            .add_modifier(Modifier::DIM),
                    ),
                ]));
                if content_line_count > 1 {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.foreground)),
                        Span::styled(
                            format!(
                                "╌ {} more lines (click to expand)",
                                content_line_count.saturating_sub(1)
                            ),
                            Style::default()
                                .fg(Color::Rgb(70, 70, 90))
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                }
            }

            // Still show tool summary for collapsed messages
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    let total = tools.len();
                    let passed = tools
                        .iter()
                        .filter(|t| matches!(t.status, ToolStatus::Complete))
                        .count();
                    let failed = tools
                        .iter()
                        .filter(|t| matches!(t.status, ToolStatus::Failed))
                        .count();
                    lines.push(Line::from(vec![
                        Span::styled("  ╶ ", Style::default().fg(colors.muted)),
                        Span::styled(
                            format!("{} tool{}", total, if total != 1 { "s" } else { "" }),
                            Style::default().fg(colors.muted),
                        ),
                        if passed > 0 {
                            Span::styled(
                                format!(" {} ok", passed),
                                Style::default().fg(Color::Rgb(80, 200, 120)),
                            )
                        } else {
                            Span::styled(String::new(), Style::default())
                        },
                        if failed > 0 {
                            Span::styled(
                                format!(" {} fail", failed),
                                Style::default().fg(Color::Rgb(255, 80, 80)),
                            )
                        } else {
                            Span::styled(String::new(), Style::default())
                        },
                        Span::styled(" ╴", Style::default().fg(colors.muted)),
                    ]));
                }
            }

            lines.push(Line::from(""));
            return lines;
        }

        // Content with inline markdown rendering
        let mut in_code_block = false;
        let mut code_block_line_count: usize = 0;
        let mut in_table = false;

        // Handle messages with only tools (no text content)
        if message.content.trim().is_empty() {
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    // Show minimal indicator for tool-only messages
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.foreground)),
                        Span::styled(
                            "(running tools)",
                            Style::default()
                                .fg(colors.muted)
                                .add_modifier(Modifier::ITALIC | Modifier::DIM),
                        ),
                    ]));
                }
            }
        }

        let content_lines: Vec<&str> = message.content.lines().collect();

        // Precompute byte offsets for each content line (for search highlighting)
        let line_byte_offsets: Vec<usize> = {
            let mut offsets = Vec::with_capacity(content_lines.len());
            let mut offset = 0;
            for line in &content_lines {
                offsets.push(offset);
                offset += line.len() + 1; // +1 for newline
            }
            offsets
        };

        let mut line_idx = 0;
        while line_idx < content_lines.len() {
            let content_line = content_lines[line_idx];
            let trimmed = content_line.trim();

            // Detect markdown table rows: | ... | ... |
            if !in_code_block
                && trimmed.starts_with('|')
                && trimmed.ends_with('|')
                && trimmed.contains('|')
            {
                // Check if this is a table by looking for separator row
                let is_separator = trimmed.trim_matches('|').split('|').all(|cell| {
                    let t = cell.trim();
                    !t.is_empty() && t.chars().all(|c| c == '-' || c == ':' || c == ' ')
                });

                if !in_table && line_idx + 1 < content_lines.len() {
                    let next_trimmed = content_lines[line_idx + 1].trim();
                    let next_is_sep = next_trimmed.starts_with('|')
                        && next_trimmed.ends_with('|')
                        && next_trimmed.trim_matches('|').split('|').all(|cell| {
                            let t = cell.trim();
                            !t.is_empty() && t.chars().all(|c| c == '-' || c == ':' || c == ' ')
                        });

                    if next_is_sep || is_separator {
                        // Start table rendering
                        in_table = true;
                        // Render header row
                        let cells: Vec<&str> = trimmed
                            .trim_matches('|')
                            .split('|')
                            .map(|s| s.trim())
                            .collect();
                        let mut header_spans =
                            vec![Span::styled("  ", Style::default().fg(colors.foreground))];
                        for (ci, cell) in cells.iter().enumerate() {
                            if ci > 0 {
                                header_spans
                                    .push(Span::styled(" │ ", Style::default().fg(colors.muted)));
                            }
                            header_spans.push(Span::styled(
                                Cow::Borrowed(*cell),
                                Style::default()
                                    .fg(colors.primary)
                                    .add_modifier(Modifier::BOLD),
                            ));
                        }
                        lines.push(Line::from(header_spans));
                        line_idx += 1;

                        // Skip separator row
                        line_idx += 1;

                        // Render remaining data rows
                        while line_idx < content_lines.len() {
                            let row_line = content_lines[line_idx].trim();
                            if !row_line.starts_with('|') || !row_line.ends_with('|') {
                                in_table = false;
                                break;
                            }
                            let cells: Vec<&str> = row_line
                                .trim_matches('|')
                                .split('|')
                                .map(|s| s.trim())
                                .collect();
                            let mut row_spans =
                                vec![Span::styled("  ", Style::default().fg(colors.foreground))];
                            for (ci, cell) in cells.iter().enumerate() {
                                if ci > 0 {
                                    row_spans.push(Span::styled(
                                        " │ ",
                                        Style::default().fg(colors.muted),
                                    ));
                                }
                                row_spans.push(Span::styled(
                                    Cow::Borrowed(*cell),
                                    Style::default().fg(colors.foreground),
                                ));
                            }
                            lines.push(Line::from(row_spans));
                            line_idx += 1;
                        }
                        // Add border line after table (adapt to terminal width)
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default().fg(colors.foreground)),
                            Span::styled(
                                "─".repeat(width.saturating_sub(4).clamp(10, 60)),
                                Style::default().fg(colors.muted),
                            ),
                        ]));
                        continue;
                    }
                }

                if in_table {
                    // Continue table rendering
                    let cells: Vec<&str> = trimmed
                        .trim_matches('|')
                        .split('|')
                        .map(|s| s.trim())
                        .collect();
                    let mut row_spans =
                        vec![Span::styled("  ", Style::default().fg(colors.foreground))];
                    for (ci, cell) in cells.iter().enumerate() {
                        if ci > 0 {
                            row_spans.push(Span::styled(" │ ", Style::default().fg(colors.muted)));
                        }
                        row_spans.push(Span::styled(
                            Cow::Borrowed(*cell),
                            Style::default().fg(colors.foreground),
                        ));
                    }
                    lines.push(Line::from(row_spans));
                    line_idx += 1;
                    continue;
                }
            } else {
                in_table = false;
            }

            // Detect code block fences
            let trimmed = content_line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if in_code_block {
                    // Close code block — show truncation indicator if lines were hidden
                    let hidden = code_block_line_count.saturating_sub(MAX_CODE_BLOCK_LINES);
                    if hidden > 0 {
                        lines.push(Line::from(vec![
                            Span::styled("  │ ", Style::default().fg(colors.muted)),
                            Span::styled(
                                format!(
                                    "... {} more line{}",
                                    hidden,
                                    if hidden != 1 { "s" } else { "" }
                                ),
                                Style::default()
                                    .fg(Color::Rgb(100, 100, 120))
                                    .add_modifier(Modifier::DIM),
                            ),
                        ]));
                    }
                    in_code_block = false;
                    code_block_line_count = 0;
                    lines.push(Line::from(vec![Span::styled(
                        "  ╰",
                        Style::default().fg(colors.muted),
                    )]));
                    line_idx += 1;
                    continue;
                } else {
                    // Open code block — extract language tag
                    in_code_block = true;
                    code_block_line_count = 0;
                    let lang_str = trimmed.trim_start_matches(['`', '~']).trim();
                    let lang_badge = if lang_str.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", lang_str)
                    };
                    lines.push(Line::from(vec![
                        Span::styled("  ╭", Style::default().fg(colors.muted)),
                        Span::styled(lang_badge, Style::default().fg(colors.secondary)),
                    ]));
                    line_idx += 1;
                    continue;
                }
            }

            if in_code_block {
                code_block_line_count += 1;
                if code_block_line_count > MAX_CODE_BLOCK_LINES {
                    // Skip lines beyond the limit, but keep counting
                    // to know when the closing fence arrives
                    line_idx += 1;
                    continue;
                }
                // Code block line: line number + monospace content
                let line_num = format!("{:>3} ", code_block_line_count);
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(colors.muted)),
                    Span::styled(line_num, Style::default().fg(Color::Rgb(70, 70, 90))),
                    Span::styled(
                        Cow::Borrowed(content_line),
                        Style::default().fg(Color::Rgb(180, 190, 210)),
                    ),
                ]));
            } else {
                // Regular content line with inline markdown
                let spans = self.render_inline_markdown(content_line, colors);
                // Apply search highlighting if search is active
                let byte_offset = line_byte_offsets.get(line_idx).copied().unwrap_or(0);
                let highlighted = self.apply_search_highlight(spans, message_index, byte_offset);
                let mut line_spans =
                    vec![Span::styled("  ", Style::default().fg(colors.foreground))];
                line_spans.extend(highlighted);
                lines.push(Line::from(line_spans));
            }

            line_idx += 1;
        }

        // Handle unclosed code block — show truncation + close indicator
        if in_code_block {
            let hidden = code_block_line_count.saturating_sub(MAX_CODE_BLOCK_LINES);
            if hidden > 0 {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(colors.muted)),
                    Span::styled(
                        format!(
                            "... {} more line{}",
                            hidden,
                            if hidden != 1 { "s" } else { "" }
                        ),
                        Style::default()
                            .fg(Color::Rgb(100, 100, 120))
                            .add_modifier(Modifier::DIM),
                    ),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("  ╰", Style::default().fg(colors.muted)),
                Span::styled(
                    " (unclosed)",
                    Style::default()
                        .fg(Color::Rgb(70, 70, 85))
                        .add_modifier(Modifier::DIM),
                ),
            ]));
        }

        // Tool executions — compact inline display with animated status
        if let Some(tools) = &message.tool_executions {
            if !tools.is_empty() {
                // Tool summary line: "╶ 3 tools: 2 passed, 1 failed ─╴"
                let total = tools.len();
                let passed = tools
                    .iter()
                    .filter(|t| matches!(t.status, ToolStatus::Complete))
                    .count();
                let failed = tools
                    .iter()
                    .filter(|t| matches!(t.status, ToolStatus::Failed))
                    .count();
                let running = tools
                    .iter()
                    .filter(|t| matches!(t.status, ToolStatus::Running))
                    .count();

                if total > 1 {
                    let mut summary_spans =
                        vec![Span::styled("  ╶ ", Style::default().fg(colors.muted))];
                    if running > 0 {
                        summary_spans.push(Span::styled(
                            self.streaming_char(),
                            Style::default().fg(Color::Rgb(255, 200, 80)),
                        ));
                    }
                    summary_spans.push(Span::styled(
                        format!(" {} tool{}", total, if total != 1 { "s" } else { "" }),
                        Style::default().fg(colors.muted),
                    ));
                    if passed > 0 {
                        summary_spans.push(Span::styled(
                            format!(" {} passed", passed),
                            Style::default().fg(Color::Rgb(80, 200, 120)),
                        ));
                    }
                    if failed > 0 {
                        summary_spans.push(Span::styled(
                            format!(" {} failed", failed),
                            Style::default().fg(Color::Rgb(255, 80, 80)),
                        ));
                    }
                    if running > 0 {
                        summary_spans.push(Span::styled(
                            format!(" {} running", running),
                            Style::default().fg(Color::Rgb(255, 200, 80)),
                        ));
                    }
                    summary_spans.push(Span::styled(" ╴", Style::default().fg(colors.muted)));
                    lines.push(Line::from(summary_spans));
                }

                // Individual tool lines
                for tool in tools {
                    lines.push(self.render_tool_line(tool, colors));

                    // Show error preview for failed tools (Goose pattern: inline error context)
                    if tool.status == ToolStatus::Failed {
                        let error_source = tool
                            .detailed_output
                            .as_deref()
                            .unwrap_or(&tool.result_summary);
                        if !error_source.is_empty() {
                            // Take first meaningful line, truncate for inline display
                            let first_line = error_source
                                .lines()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or("");
                            let error_preview = if first_line.len() > 80 {
                                format!("{}…", &first_line[..first_line.floor_char_boundary(79)])
                            } else {
                                first_line.to_string()
                            };
                            lines.push(Line::from(vec![
                                Span::styled("      ", Style::default().fg(colors.foreground)),
                                Span::styled(
                                    error_preview,
                                    Style::default()
                                        .fg(Color::Rgb(180, 100, 100))
                                        .add_modifier(Modifier::DIM),
                                ),
                            ]));
                        }
                    }

                    // Show output preview for running tools (Goose pattern: live output preview)
                    if tool.status == ToolStatus::Running {
                        let output = tool
                            .detailed_output
                            .as_deref()
                            .filter(|o| !o.is_empty())
                            .unwrap_or(&tool.result_summary);
                        if !output.is_empty() {
                            // Take last meaningful line (most recent output)
                            let last_line = output
                                .lines()
                                .rev()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or("");
                            let preview = if last_line.len() > 70 {
                                format!("{}…", &last_line[..last_line.floor_char_boundary(69)])
                            } else {
                                last_line.to_string()
                            };
                            if !preview.is_empty() {
                                lines.push(Line::from(vec![
                                    Span::styled("      ", Style::default().fg(colors.foreground)),
                                    Span::styled(
                                        preview,
                                        Style::default()
                                            .fg(Color::Rgb(140, 160, 180))
                                            .add_modifier(Modifier::DIM),
                                    ),
                                ]));
                            }
                        }
                    }
                }

                // Expanded tool details (input JSON and output)
                if message.tools_expansion == ExpansionLevel::Expanded {
                    for tool in tools {
                        // Show tool input JSON
                        if let Some(input_json) = &tool.input_json {
                            lines.push(Line::from(vec![Span::styled(
                                "      ╭─ input ─╴",
                                Style::default().fg(colors.muted),
                            )]));
                            let json_str = serde_json::to_string_pretty(input_json)
                                .unwrap_or_else(|_| "{}".to_string());
                            for json_line in json_str.lines().take(15) {
                                lines.push(Line::from(vec![
                                    Span::styled("      │ ", Style::default().fg(colors.muted)),
                                    Span::styled(
                                        json_line.to_string(),
                                        Style::default().fg(Color::Rgb(180, 180, 200)),
                                    ),
                                ]));
                            }
                        }

                        // Show detailed output (Goose pattern: head/tail truncation)
                        if let Some(output) = &tool.detailed_output {
                            if tool.input_json.is_some() {
                                lines.push(Line::from(vec![Span::styled(
                                    "      ╰─ output ─╴",
                                    Style::default().fg(colors.muted),
                                )]));
                            } else {
                                lines.push(Line::from(vec![Span::styled(
                                    "      ╭─ output ─╴",
                                    Style::default().fg(colors.muted),
                                )]));
                            }
                            let all_lines: Vec<&str> = output.lines().collect();
                            let max_lines = 10;
                            if all_lines.len() <= max_lines {
                                for out_line in &all_lines {
                                    lines.push(Line::from(vec![
                                        Span::styled("      │ ", Style::default().fg(colors.muted)),
                                        Span::styled(
                                            out_line.to_string(),
                                            Style::default().fg(Color::Rgb(180, 190, 210)),
                                        ),
                                    ]));
                                }
                            } else {
                                // Show first half and last half with hidden count in between
                                let head = max_lines / 2;
                                let tail = max_lines - head;
                                for out_line in &all_lines[..head] {
                                    lines.push(Line::from(vec![
                                        Span::styled("      │ ", Style::default().fg(colors.muted)),
                                        Span::styled(
                                            out_line.to_string(),
                                            Style::default().fg(Color::Rgb(180, 190, 210)),
                                        ),
                                    ]));
                                }
                                lines.push(Line::from(vec![
                                    Span::styled("      │ ", Style::default().fg(colors.muted)),
                                    Span::styled(
                                        format!(
                                            "... ({} lines hidden)",
                                            all_lines.len() - head - tail
                                        ),
                                        Style::default()
                                            .fg(colors.muted)
                                            .add_modifier(Modifier::ITALIC),
                                    ),
                                ]));
                                for out_line in &all_lines[all_lines.len() - tail..] {
                                    lines.push(Line::from(vec![
                                        Span::styled("      │ ", Style::default().fg(colors.muted)),
                                        Span::styled(
                                            out_line.to_string(),
                                            Style::default().fg(Color::Rgb(180, 190, 210)),
                                        ),
                                    ]));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Thinking (expanded)
        if let Some(thinking) = &message.thinking {
            if !thinking.is_empty() && message.thinking_expansion == ExpansionLevel::Expanded {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default().fg(colors.foreground)),
                    Span::styled(
                        "╶─ thinking ─╴",
                        Style::default()
                            .fg(colors.muted)
                            .add_modifier(Modifier::DIM),
                    ),
                ]));

                let all_lines: Vec<&str> = thinking.lines().collect();
                let max_lines = 20;

                if all_lines.len() <= max_lines {
                    for think_line in &all_lines {
                        lines.push(Line::from(vec![
                            Span::styled("    ", Style::default().fg(colors.foreground)),
                            Span::styled(
                                Cow::Borrowed(*think_line),
                                Style::default()
                                    .fg(colors.muted)
                                    .add_modifier(Modifier::DIM),
                            ),
                        ]));
                    }
                } else {
                    // Head/tail truncation (Goose pattern)
                    let head = max_lines / 2;
                    let tail = max_lines - head;
                    for think_line in &all_lines[..head] {
                        lines.push(Line::from(vec![
                            Span::styled("    ", Style::default().fg(colors.foreground)),
                            Span::styled(
                                Cow::Borrowed(*think_line),
                                Style::default()
                                    .fg(colors.muted)
                                    .add_modifier(Modifier::DIM),
                            ),
                        ]));
                    }
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default().fg(colors.foreground)),
                        Span::styled(
                            format!("... ({} lines hidden)", all_lines.len() - head - tail),
                            Style::default()
                                .fg(colors.muted)
                                .add_modifier(Modifier::DIM | Modifier::ITALIC),
                        ),
                    ]));
                    for think_line in &all_lines[all_lines.len() - tail..] {
                        lines.push(Line::from(vec![
                            Span::styled("    ", Style::default().fg(colors.foreground)),
                            Span::styled(
                                Cow::Borrowed(*think_line),
                                Style::default()
                                    .fg(colors.muted)
                                    .add_modifier(Modifier::DIM),
                            ),
                        ]));
                    }
                }
            }
        }

        // Turn summary footer (Goose pattern: completion summary)
        // Skip for mid-chain messages — only the last message in a chain gets the footer
        if !is_chained_mid && message.role == MessageRole::Assistant {
            let content_lines = message.content.lines().count();
            let has_tools = message
                .tool_executions
                .as_ref()
                .is_some_and(|t| !t.is_empty());

            if has_tools {
                // Tool summary: tool count + pass/fail + duration + content lines
                if let Some(tools) = &message.tool_executions {
                    if !tools.is_empty() {
                        let total = tools.len();
                        let passed = tools
                            .iter()
                            .filter(|t| matches!(t.status, ToolStatus::Complete))
                            .count();
                        let failed = tools
                            .iter()
                            .filter(|t| matches!(t.status, ToolStatus::Failed))
                            .count();
                        let total_ms: u64 = tools.iter().filter_map(|t| t.duration_ms).sum();

                        let mut footer_spans: Vec<Span<'b>> = vec![
                            Span::styled("  ╶ ", Style::default().fg(Color::Rgb(50, 50, 60))),
                            Span::styled(
                                format!("{} tool{}", total, if total != 1 { "s" } else { "" }),
                                Style::default()
                                    .fg(Color::Rgb(80, 80, 95))
                                    .add_modifier(Modifier::DIM),
                            ),
                        ];

                        if passed > 0 && failed == 0 {
                            footer_spans.push(Span::styled(
                                " ✓".to_string(),
                                Style::default()
                                    .fg(Color::Rgb(60, 120, 80))
                                    .add_modifier(Modifier::DIM),
                            ));
                        } else if failed > 0 {
                            footer_spans.push(Span::styled(
                                format!(" {}✗{}", passed, failed),
                                Style::default()
                                    .fg(Color::Rgb(130, 80, 80))
                                    .add_modifier(Modifier::DIM),
                            ));
                        }

                        if total_ms > 0 {
                            let dur_str = if total_ms < 1000 {
                                format!(" {}ms", total_ms)
                            } else {
                                format!(" {:.1}s", total_ms as f64 / 1000.0)
                            };
                            footer_spans.push(Span::styled(
                                dur_str,
                                Style::default()
                                    .fg(Color::Rgb(70, 70, 85))
                                    .add_modifier(Modifier::DIM),
                            ));
                        }

                        // Content line count
                        if content_lines > 0 {
                            footer_spans.push(Span::styled(
                                format!(" · {} lines", content_lines),
                                Style::default()
                                    .fg(Color::Rgb(70, 70, 85))
                                    .add_modifier(Modifier::DIM),
                            ));
                        }

                        footer_spans.push(Span::styled(
                            " ╴",
                            Style::default().fg(Color::Rgb(50, 50, 60)),
                        ));
                        lines.push(Line::from(footer_spans));
                    }
                }
            } else if content_lines > 3 {
                // Text-only response summary: word count + line count for longer messages
                let word_count = message.content.split_whitespace().count();
                let footer_spans: Vec<Span<'b>> = vec![
                    Span::styled("  ╶ ", Style::default().fg(Color::Rgb(50, 50, 60))),
                    Span::styled(
                        format!("{} words · {} lines", word_count, content_lines),
                        Style::default()
                            .fg(Color::Rgb(70, 70, 85))
                            .add_modifier(Modifier::DIM),
                    ),
                    Span::styled(" ╴", Style::default().fg(Color::Rgb(50, 50, 60))),
                ];
                lines.push(Line::from(footer_spans));
            }
        }

        // Blank line separator (skip for mid-chain to visually group)
        if !is_chained_mid {
            lines.push(Line::from(""));
        }

        lines
    }

    /// Render a single tool execution line with compact inline format.
    ///
    /// Shows: icon name [duration] summary
    /// Plus collapsible output below if present.
    fn render_tool_line<'b>(&self, tool: &'b ToolExecution, colors: &ThemeColors) -> Line<'b> {
        let (icon, color) = match tool.status {
            ToolStatus::Running => {
                let frames = ["◐", "◑", "◒", "◓"];
                (
                    frames[self.animation_frame % frames.len()],
                    Color::Rgb(255, 200, 80),
                )
            }
            ToolStatus::Complete => ("●", Color::Rgb(80, 200, 120)),
            ToolStatus::Failed => ("✗", Color::Rgb(255, 80, 80)),
            ToolStatus::Cancelled => ("⚠", Color::Rgb(200, 150, 50)),
        };

        // Tool-type icon (Goose pattern: distinguish tool types visually)
        let type_icon = tool_type_icon(&tool.name);

        let mut spans = vec![
            Span::styled("    ", Style::default().fg(colors.foreground)),
            Span::styled(icon, Style::default().fg(color)),
            Span::styled(" ", Style::default().fg(colors.foreground)),
        ];
        if !type_icon.is_empty() {
            spans.push(Span::styled(
                format!("{} ", type_icon),
                Style::default()
                    .fg(Color::Rgb(100, 140, 180))
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Display name: shorten common tool names for readability
        let display_name = match tool.name.as_str() {
            "read_file" => "read",
            "write_file" => "write",
            "edit_file" | "search_replace" => "edit",
            "execute_command" | "bash" => "sh",
            "list_dir" | "list_files" => "ls",
            n => n,
        };
        spans.push(Span::styled(
            display_name,
            Style::default().fg(colors.foreground),
        ));

        // Key parameter extraction (Goose pattern: show file path, command, etc.)
        if let Some(key_param) =
            extract_tool_key_param(&tool.name, tool.input_json.as_ref(), &tool.result_summary)
        {
            let truncated = if key_param.len() > 50 {
                shorten_tool_param(&key_param, 50)
            } else {
                key_param
            };
            spans.push(Span::styled(
                format!(" {}", truncated),
                Style::default()
                    .fg(Color::Rgb(140, 150, 170))
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Duration badge
        if let Some(dur_ms) = tool.duration_ms {
            spans.push(Span::styled(
                format!(
                    " {}",
                    crate::app::tool_output_format::format_duration(dur_ms)
                ),
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::DIM),
            ));
        } else if tool.status == ToolStatus::Running {
            // Show elapsed for running tools
            let elapsed = Utc::now()
                .signed_duration_since(tool.start_time)
                .num_milliseconds()
                .max(0) as u64;
            spans.push(Span::styled(
                format!(
                    " {}",
                    crate::app::tool_output_format::format_duration(elapsed)
                ),
                Style::default()
                    .fg(Color::Rgb(255, 200, 80))
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Progress bar for running tools with progress tracking
        if tool.status == ToolStatus::Running {
            if let (Some(current), Some(total), Some(desc)) = (
                tool.progress_current,
                tool.progress_total,
                &tool.progress_description,
            ) {
                let pct = (current as f64 / total as f64 * 100.0) as u8;
                let bar_width = 10;
                let filled = ((pct as usize * bar_width) / 100).min(bar_width);
                let empty = bar_width - filled;
                let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

                spans.push(Span::styled(
                    format!(" [{} {}%]", bar, pct),
                    Style::default().fg(Color::Rgb(100, 180, 255)),
                ));
                // Truncate progress description to prevent line overflow
                let desc_display = if desc.len() > 40 {
                    format!("{}…", &desc[..desc.floor_char_boundary(39)])
                } else {
                    desc.clone()
                };
                spans.push(Span::styled(
                    format!(" {}", desc_display),
                    Style::default().fg(colors.muted),
                ));
            }
        }

        // Summary inline — truncate to prevent overflowing the terminal
        if !tool.result_summary.is_empty() {
            let summary = if tool.result_summary.len() > 80 {
                format!(
                    "{}…",
                    &tool.result_summary[..tool.result_summary.floor_char_boundary(79)]
                )
            } else {
                tool.result_summary.clone()
            };
            spans.push(Span::styled(
                format!(" {}", summary),
                Style::default().fg(colors.muted),
            ));
        }

        Line::from(spans)
    }

    /// Render input area with context info line (Goose pattern)
    pub fn render_input(&self, frame: &mut ratatui::Frame, area: Rect, colors: &ThemeColors) {
        use ratatui::layout::Alignment;
        use ratatui::text::Text;
        use ratatui::widgets::Paragraph;

        let mode_char = match self.input_mode {
            InputMode::SingleLine => "▸",
            InputMode::MultiLine => "▪",
        };

        // Line 1: Context info bar (Goose pattern: show context near input)
        let mut info_spans = vec![Span::styled("  ", Style::default().fg(colors.muted))];

        // Context usage bar
        if self.context_usage.context_limit > 0 {
            let bar_text = self.context_usage.format_bar(10);
            let bar_color = match self.context_usage.color_level() {
                super::context_usage::UsageLevel::Low => Color::Rgb(80, 200, 120),
                super::context_usage::UsageLevel::Medium => Color::Rgb(255, 200, 80),
                super::context_usage::UsageLevel::High => Color::Rgb(255, 80, 80),
            };
            info_spans.push(Span::styled(
                format!("ctx:{} ", bar_text),
                Style::default().fg(bar_color).add_modifier(Modifier::DIM),
            ));
        }

        // Token split (Goose pattern: show input/output breakdown)
        if self.session_input_tokens > 0 || self.session_output_tokens > 0 {
            let in_fmt = format_tokens_compact(self.session_input_tokens);
            let out_fmt = format_tokens_compact(self.session_output_tokens);
            info_spans.push(Span::styled(
                format!("↑{} ↓{} ", in_fmt, out_fmt),
                Style::default()
                    .fg(Color::Rgb(100, 120, 150))
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Session cost
        if self.session_cost > 0.0 {
            let cost_str = if self.session_cost < 0.01 {
                format!("${:.4}", self.session_cost)
            } else {
                format!("${:.2}", self.session_cost)
            };
            info_spans.push(Span::styled(
                format!("{} ", cost_str),
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Fill with separator
        let info_width: usize = info_spans.iter().map(|s| s.content.len()).sum();
        let remaining = area.width as usize;
        if remaining > info_width + 20 {
            // Show readline-style hints on the info bar
            let hints = if !self.reverse_search_query.is_empty() && self.reverse_search_total > 0 {
                // Reverse search mode: show query and match position
                format!(
                    "(reverse-i-search)`{}': {}/{}",
                    self.reverse_search_query, self.reverse_search_match, self.reverse_search_total
                )
            } else if !self.reverse_search_query.is_empty() {
                format!(
                    "(reverse-i-search)`{}': no matches",
                    self.reverse_search_query
                )
            } else if self.history_position > 0 {
                format!("history {}/{}", self.history_position, self.history_total)
            } else if self.user_scrolled && !self.is_streaming {
                "G bottom · ↑↓ scroll".to_string()
            } else if self.is_streaming {
                if self.has_queued_message {
                    "Ctrl+C cancel · next ready".to_string()
                } else {
                    "Ctrl+C cancel".to_string()
                }
            } else if self.input_mode == InputMode::MultiLine {
                "Opt+Enter send · Enter newline".to_string()
            } else {
                "Enter send · Shift+Enter newline".to_string()
            };
            let hints_len = hints.len();
            let sep_count = remaining.saturating_sub(info_width + hints_len + 4);
            info_spans.push(Span::styled(
                "─".repeat(sep_count),
                Style::default().fg(Color::Rgb(40, 40, 50)),
            ));
            info_spans.push(Span::styled(
                format!(" {} ", hints),
                Style::default().fg(if !self.reverse_search_query.is_empty() {
                    Color::Rgb(255, 200, 80) // Amber for reverse search
                } else if self.history_position > 0 {
                    Color::Rgb(120, 160, 200) // Blue for history browsing
                } else {
                    Color::Rgb(70, 70, 85)
                }),
            ));
        }

        let info_line = Line::from(info_spans);

        let mut all_lines = vec![info_line];

        // Cursor blink: alternate between bright and dim every ~0.5s (frames 0-3 bright, 4-7 dim)
        let cursor_bright = self.animation_frame % 8 < 4;
        let cursor_char = "▏";
        let cursor_style = if cursor_bright {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Rgb(100, 100, 120))
        };

        if self.input_text.is_empty() {
            // Empty input — show blinking cursor with placeholder
            let placeholder = if self.is_streaming {
                ""
            } else {
                " Ask me anything..."
            };
            let mut spans = vec![
                Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))),
                Span::styled(format!("{} ", mode_char), Style::default().fg(colors.muted)),
                Span::styled(cursor_char, cursor_style),
            ];
            if !placeholder.is_empty() {
                spans.push(Span::styled(
                    placeholder.to_string(),
                    Style::default().fg(colors.muted),
                ));
            }
            all_lines.push(Line::from(spans));
        } else if self.input_line_count > 1 {
            // Multiline content — render each line with line numbers and cursor
            let lines: Vec<&str> = self.input_text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let line_num = format!("{:>2} ", i + 1);
                let is_cursor_line = i == self.cursor_row.min(lines.len().saturating_sub(1));

                let prefix = if i == 0 {
                    vec![
                        Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))),
                        Span::styled(format!("{} ", mode_char), Style::default().fg(colors.muted)),
                        Span::styled(
                            line_num.clone(),
                            Style::default().fg(Color::Rgb(70, 70, 85)),
                        ),
                    ]
                } else {
                    vec![
                        Span::styled("  ", Style::default().fg(colors.muted)),
                        Span::styled(
                            line_num.clone(),
                            Style::default().fg(Color::Rgb(70, 70, 85)),
                        ),
                    ]
                };

                if is_cursor_line {
                    // Split at cursor position for inline cursor rendering
                    let col = self.cursor_col.min(line.len());
                    let (before, after) = line.split_at(col);
                    let mut spans = prefix;
                    if !before.is_empty() {
                        spans.push(Span::styled(
                            before.to_string(),
                            Style::default().fg(colors.foreground),
                        ));
                    }
                    spans.push(Span::styled(cursor_char, cursor_style));
                    if !after.is_empty() {
                        spans.push(Span::styled(
                            after.to_string(),
                            Style::default().fg(colors.foreground),
                        ));
                    }
                    all_lines.push(Line::from(spans));
                } else {
                    let mut spans = prefix;
                    spans.push(Span::styled(
                        line.to_string(),
                        Style::default().fg(colors.foreground),
                    ));
                    all_lines.push(Line::from(spans));
                }
            }
        } else {
            // Single-line content with cursor
            let col = self.cursor_col.min(self.input_text.len());
            let (before, after) = self.input_text.split_at(col);
            let mut spans = vec![
                Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))),
                Span::styled(format!("{} ", mode_char), Style::default().fg(colors.muted)),
            ];
            if !before.is_empty() {
                spans.push(Span::styled(before, Style::default().fg(colors.foreground)));
            }
            spans.push(Span::styled(cursor_char, cursor_style));
            if !after.is_empty() {
                spans.push(Span::styled(
                    after.to_string(),
                    Style::default().fg(colors.foreground),
                ));
            }
            all_lines.push(Line::from(spans));
        }

        let text = Text::from(all_lines);
        let paragraph = Paragraph::new(text).alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

    /// Render footer with status
    pub fn render_footer(&self, frame: &mut ratatui::Frame, area: Rect, colors: &ThemeColors) {
        use ratatui::layout::Alignment;
        use ratatui::widgets::Paragraph;

        // Dynamic footer status with thinking messages
        let footer_status = if self.is_streaming && self.active_tool_count == 0 {
            thinking_messages::get_thinking_message(self.chunks_received)
        } else {
            self.agent_status
        };

        let mut spans = vec![
            Span::styled("│", Style::default().fg(colors.muted)),
            Span::styled(
                format!(" {} ", footer_status),
                Style::default().fg(colors.primary),
            ),
            Span::styled("│", Style::default().fg(colors.muted)),
            Span::styled(
                format!(" mem:{} ", self.auto_memory_status),
                Style::default().fg(colors.secondary),
            ),
        ];

        // Model name in footer (Goose pattern: always visible)
        if !self.current_model.is_empty() {
            let model_short = self
                .current_model
                .rsplit('/')
                .next()
                .unwrap_or(self.current_model);
            // Truncate long model names for footer
            let display = if model_short.len() > 20 {
                format!("{}…", &model_short[..model_short.floor_char_boundary(19)])
            } else {
                model_short.to_string()
            };
            spans.push(Span::styled("│", Style::default().fg(colors.muted)));
            spans.push(Span::styled(
                format!(" {} ", display),
                Style::default()
                    .fg(Color::Rgb(120, 140, 180))
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Active tools indicator
        if self.active_tool_count > 0 {
            let tool_icon = self.streaming_char();
            spans.push(Span::styled("│", Style::default().fg(colors.muted)));
            spans.push(Span::styled(
                format!(" {}{} ", tool_icon, self.active_tool_count),
                Style::default().fg(Color::Rgb(255, 200, 80)),
            ));
            if !self.active_tool_names.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", self.active_tool_names),
                    Style::default()
                        .fg(colors.muted)
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        // Last response duration (Goose pattern: show timing after completion)
        if !self.is_streaming {
            if let Some(dur) = self.last_response_duration {
                let ms = dur.as_millis();
                let timing_str = if ms < 1000 {
                    format!("{}ms", ms)
                } else {
                    format_elapsed_short(dur.as_secs())
                };
                spans.push(Span::styled("│", Style::default().fg(colors.muted)));
                spans.push(Span::styled(
                    format!(" {} ", timing_str),
                    Style::default()
                        .fg(colors.muted)
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        // Rate limit countdown
        if let Some(until) = self.rate_limit_until {
            let remaining = until.saturating_duration_since(std::time::Instant::now());
            let remaining_secs = remaining.as_secs();
            if remaining_secs > 0 {
                spans.push(Span::styled("│", Style::default().fg(colors.muted)));
                spans.push(Span::styled(
                    format!(" ◯ rate:{}s ", remaining_secs),
                    Style::default()
                        .fg(colors.muted)
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        // Fill with light separators and keyboard hints
        let width = area.width as usize;
        let content_width: usize = spans.iter().map(|s| s.content.len()).sum();
        let remaining = width.saturating_sub(content_width);

        if remaining > 30 {
            // Show compact keyboard hints in the fill area
            let hints = if self.is_streaming {
                "Ctrl+C stop"
            } else {
                "? help · / cmds · Ctrl+D quit"
            };
            let hint_len = hints.len();
            let sep_count = remaining.saturating_sub(hint_len + 2);
            if sep_count > 0 {
                spans.push(Span::styled(
                    "─".repeat(sep_count / 2),
                    Style::default().fg(colors.muted),
                ));
                spans.push(Span::styled(
                    format!(" {} ", hints),
                    Style::default().fg(Color::DarkGray),
                ));
                let right_sep = remaining.saturating_sub(sep_count / 2 + hint_len + 2);
                spans.push(Span::styled(
                    "─".repeat(right_sep),
                    Style::default().fg(colors.muted),
                ));
            } else {
                spans.push(Span::styled(
                    "─".repeat(remaining),
                    Style::default().fg(colors.muted),
                ));
            }
        } else if remaining > 0 {
            spans.push(Span::styled(
                "─".repeat(remaining),
                Style::default().fg(colors.muted),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

    /// Render a single line with inline markdown styling.
    ///
    /// Handles: `code`, **bold**, *italic*, ~~strikethrough~~,
    /// [links](url), # headings, and > blockquotes.
    fn render_inline_markdown<'b>(&self, line: &'b str, colors: &ThemeColors) -> Vec<Span<'b>> {
        let mut spans = Vec::new();

        // Detect heading
        let heading_level = if line.starts_with("### ") {
            Some(3)
        } else if line.starts_with("## ") {
            Some(2)
        } else if line.starts_with("# ") {
            Some(1)
        } else {
            None
        };

        if let Some(level) = heading_level {
            let hashes = match level {
                1 => "# ",
                2 => "## ",
                3 => "### ",
                _ => "# ",
            };
            let content = &line[hashes.len()..];
            spans.push(Span::styled(
                hashes,
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD | Modifier::DIM),
            ));
            spans.push(Span::styled(
                Cow::Borrowed(content),
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ));
            return spans;
        }

        // Detect blockquote
        if let Some(rest) = line.strip_prefix("> ") {
            spans.push(Span::styled("▎ ", Style::default().fg(colors.muted)));
            spans.push(Span::styled(
                Cow::Borrowed(rest),
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::ITALIC),
            ));
            return spans;
        }

        // Detect unordered list items: "- item", "* item", "+ item"
        let trimmed_line = line.trim_start();
        if trimmed_line.starts_with("- ")
            || trimmed_line.starts_with("* ")
            || trimmed_line.starts_with("+ ")
        {
            let indent = line.len() - trimmed_line.len();
            let bullet_char = &trimmed_line[..1];
            let rest = &trimmed_line[2..];
            if indent > 0 {
                spans.push(Span::styled(
                    " ".repeat(indent),
                    Style::default().fg(colors.foreground),
                ));
            }
            spans.push(Span::styled(
                format!("{} ", bullet_char),
                Style::default().fg(colors.primary),
            ));
            // Parse inline markdown in the list item content
            let content_spans = Self::parse_inline_content(rest, colors);
            spans.extend(content_spans);
            return spans;
        }

        // Detect ordered list items: "1. item", "2. item", etc.
        if let Some(dot_pos) = trimmed_line.find(". ") {
            let prefix = &trimmed_line[..dot_pos];
            if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
                let indent = line.len() - trimmed_line.len();
                let rest = &trimmed_line[dot_pos + 2..];
                if indent > 0 {
                    spans.push(Span::styled(
                        " ".repeat(indent),
                        Style::default().fg(colors.foreground),
                    ));
                }
                spans.push(Span::styled(
                    format!("{}. ", prefix),
                    Style::default().fg(colors.primary),
                ));
                let content_spans = Self::parse_inline_content(rest, colors);
                spans.extend(content_spans);
                return spans;
            }
        }

        // Detect horizontal rule: --- or *** or ___
        let rule_trimmed = trimmed_line.trim();
        if (rule_trimmed.chars().all(|c| c == '-') && rule_trimmed.len() >= 3)
            || (rule_trimmed.chars().all(|c| c == '*') && rule_trimmed.len() >= 3)
            || (rule_trimmed.chars().all(|c| c == '_') && rule_trimmed.len() >= 3)
        {
            spans.push(Span::styled(
                "─".repeat(line.len().clamp(10, 60)),
                Style::default().fg(colors.muted),
            ));
            return spans;
        }

        // Inline parsing (delegates to shared method)
        Self::parse_inline_content(line, colors)
    }

    /// Parse inline markdown content: `code`, **bold**, *italic*, ~~strikethrough~~.
    /// Shared by render_inline_markdown (full lines) and list item content.
    ///
    /// Optimized: uses byte-based scanning instead of `Vec<char>` allocation,
    /// returns `Cow::Borrowed` spans to avoid string allocations, and has a
    /// fast path for lines with no markdown characters.
    fn parse_inline_content<'b>(line: &'b str, colors: &ThemeColors) -> Vec<Span<'b>> {
        let bytes = line.as_bytes();
        let len = bytes.len();

        // Fast path: no markdown characters — single borrowed span, zero allocation
        if !bytes.contains(&b'`')
            && !bytes.contains(&b'*')
            && !bytes.contains(&b'~')
            && !bytes.contains(&b'[')
        {
            return vec![Span::styled(
                Cow::Borrowed(line),
                Style::default().fg(colors.foreground),
            )];
        }

        let mut spans = Vec::new();
        let mut i = 0; // byte index

        while i < len {
            let b = bytes[i];

            // Inline code: `code` or ``code``
            if b == b'`' {
                let tick_count = count_consecutive(bytes, i, b'`');
                let search_start = i + tick_count;
                if search_start < len {
                    if let Some(close_pos) =
                        find_consecutive(&bytes[search_start..], b'`', tick_count)
                    {
                        let content_end = search_start + close_pos;
                        spans.push(Span::styled(
                            Cow::Borrowed(&line[search_start..content_end]),
                            Style::default().fg(Color::Rgb(180, 210, 170)),
                        ));
                        i = content_end + tick_count;
                        continue;
                    }
                }
                spans.push(Span::styled(
                    Cow::Borrowed(&line[i..i + 1]),
                    Style::default().fg(colors.foreground),
                ));
                i += 1;
                continue;
            }

            // Bold (**text**)
            if b == b'*' && i + 1 < len && bytes[i + 1] == b'*' {
                if let Some(end) = find_byte_pair(&bytes[i + 2..], b'*') {
                    spans.push(Span::styled(
                        Cow::Borrowed(&line[i + 2..i + 2 + end]),
                        Style::default()
                            .fg(colors.foreground)
                            .add_modifier(Modifier::BOLD),
                    ));
                    i = i + 2 + end + 2;
                    continue;
                }
            }

            // Italic (*text*)
            if b == b'*' && (i + 1 >= len || bytes[i + 1] != b'*') {
                if let Some(end) = find_byte(&bytes[i + 1..], b'*') {
                    spans.push(Span::styled(
                        Cow::Borrowed(&line[i + 1..i + 1 + end]),
                        Style::default()
                            .fg(colors.foreground)
                            .add_modifier(Modifier::ITALIC),
                    ));
                    i = i + 1 + end + 1;
                    continue;
                }
            }

            // Strikethrough (~~text~~)
            if b == b'~' && i + 1 < len && bytes[i + 1] == b'~' {
                if let Some(end) = find_byte_pair(&bytes[i + 2..], b'~') {
                    spans.push(Span::styled(
                        Cow::Borrowed(&line[i + 2..i + 2 + end]),
                        Style::default()
                            .fg(colors.muted)
                            .add_modifier(Modifier::CROSSED_OUT),
                    ));
                    i = i + 2 + end + 2;
                    continue;
                }
            }

            // Markdown links [text](url)
            if b == b'[' {
                if let Some(bracket_end) = find_byte(&bytes[i + 1..], b']') {
                    let url_start = i + 1 + bracket_end + 1;
                    if url_start < len && bytes[url_start] == b'(' {
                        if let Some(url_end) = find_byte(&bytes[url_start + 1..], b')') {
                            let link_text = &line[i + 1..i + 1 + bracket_end];
                            let url_text = &line[url_start + 1..url_start + 1 + url_end];
                            if !link_text.is_empty() {
                                spans.push(Span::styled(
                                    Cow::Borrowed(link_text),
                                    Style::default()
                                        .fg(colors.secondary)
                                        .add_modifier(Modifier::UNDERLINED),
                                ));
                            }
                            if !url_text.is_empty() && url_text.len() < 80 {
                                spans.push(Span::styled(
                                    format!("({})", url_text),
                                    Style::default()
                                        .fg(colors.muted)
                                        .add_modifier(Modifier::DIM),
                                ));
                            }
                            i = url_start + 1 + url_end + 1;
                            continue;
                        }
                    }
                }
            }

            // Plain text — accumulate until markdown character
            let start = i;
            i += 1;
            while i < len {
                let c = bytes[i];
                if c == b'`' || c == b'*' || c == b'~' || c == b'[' {
                    break;
                }
                i += 1;
            }
            spans.push(Span::styled(
                Cow::Borrowed(&line[start..i]),
                Style::default().fg(colors.foreground),
            ));
        }

        spans
    }

    /// Estimate message height for scrolling
    pub fn estimate_message_height(&self, message: &Message, width: usize) -> usize {
        // System messages: compact for single-line, multi-line block for diffs/stats
        if message.role == MessageRole::System {
            let content = message.content.trim();
            if content.is_empty() {
                return 0;
            }
            // Diff content: one line per diff line
            if content.starts_with("diff --git") {
                let line_count = content.lines().count();
                return line_count.min(50) + if line_count > 50 { 1 } else { 0 };
            }
            // Multi-line content: header + content lines
            let line_count = content.lines().count();
            if line_count > 1 {
                let capped = line_count.min(50);
                return 1 + capped + if line_count > 50 { 1 } else { 0 }; // header + lines + overflow indicator
            }
            // Single-line notice
            return 1;
        }

        let role_height = 1;

        // Collapsed messages: role header + first line + "N more" indicator + tools + separator
        if message.collapsed {
            let content_line_count = message.content.lines().count();
            let mut height = role_height; // role header
            if content_line_count > 0 {
                height += 1; // first line preview
                if content_line_count > 1 {
                    height += 1; // "N more lines" indicator
                }
            }
            // Tool summary line
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    height += 1; // tool summary
                }
            }
            height += 1; // separator
            return height;
        }

        // Calculate content height accounting for code block line limits
        // and markdown table rendering.
        let content_lines = if message.content.is_empty() {
            0
        } else {
            let content_lines_vec: Vec<&str> = message.content.lines().collect();
            let mut in_code = false;
            let mut code_lines: usize = 0;
            let mut in_table = false;
            let mut total: usize = 0;
            let mut line_idx = 0;

            while line_idx < content_lines_vec.len() {
                let line = content_lines_vec[line_idx];
                let trimmed = line.trim();

                // Code block fences
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    in_table = false;
                    if in_code {
                        let hidden = code_lines.saturating_sub(MAX_CODE_BLOCK_LINES);
                        if hidden > 0 {
                            total += 1;
                        } // "... N more lines"
                        in_code = false;
                        code_lines = 0;
                        total += 1; // closing fence
                    } else {
                        in_code = true;
                        code_lines = 0;
                        total += 1; // opening fence
                    }
                    line_idx += 1;
                    continue;
                }

                if in_code {
                    code_lines += 1;
                    if code_lines <= MAX_CODE_BLOCK_LINES {
                        total += 1;
                    }
                    line_idx += 1;
                    continue;
                }

                // Markdown table detection: match the renderer's table logic
                // Tables render as: header row + skip separator + data rows + border
                if trimmed.starts_with('|')
                    && trimmed.ends_with('|')
                    && trimmed.contains('|')
                    && !in_table
                {
                    let is_separator = trimmed.trim_matches('|').split('|').all(|cell| {
                        let t = cell.trim();
                        !t.is_empty() && t.chars().all(|c| c == '-' || c == ':' || c == ' ')
                    });

                    if !is_separator && line_idx + 1 < content_lines_vec.len() {
                        let next_trimmed = content_lines_vec[line_idx + 1].trim();
                        let next_is_sep = next_trimmed.starts_with('|')
                            && next_trimmed.ends_with('|')
                            && next_trimmed.trim_matches('|').split('|').all(|cell| {
                                let t = cell.trim();
                                !t.is_empty() && t.chars().all(|c| c == '-' || c == ':' || c == ' ')
                            });

                        if next_is_sep {
                            // Start of table: header row (1) + skip separator
                            total += 1; // header row
                            line_idx += 2; // skip header + separator

                            // Count data rows
                            while line_idx < content_lines_vec.len() {
                                let row_line = content_lines_vec[line_idx].trim();
                                if !row_line.starts_with('|') || !row_line.ends_with('|') {
                                    break;
                                }
                                total += 1; // data row
                                line_idx += 1;
                            }
                            total += 1; // border line after table
                            in_table = false;
                            continue;
                        }
                    }

                    // Standalone separator row (part of a table not caught above)
                    if is_separator {
                        line_idx += 1;
                        continue; // separators are not rendered
                    }
                }

                // Regular content line
                in_table = false;
                let line_width = line.width();
                total += if line_width == 0 {
                    1
                } else {
                    line_width.div_ceil(width.max(1))
                };
                line_idx += 1;
            }

            if in_code {
                let hidden = code_lines.saturating_sub(MAX_CODE_BLOCK_LINES);
                if hidden > 0 {
                    total += 1;
                } // "... N more lines"
                total += 1; // "╰ (unclosed)" close indicator
            }
            total
        };

        let mut height = role_height + content_lines;

        // Tool-only messages get "(running tools)" indicator
        if message.content.trim().is_empty() {
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    height += 1;
                }
            }
        }

        // Add tools
        if let Some(tools) = &message.tool_executions {
            if tools.len() > 1 {
                height += 1; // Summary line (╶ N tools: X passed ... ╴)
            }
            height += tools.len(); // One line per tool
                                   // Error preview lines for failed tools
            let failed_with_output = tools
                .iter()
                .filter(|t| {
                    t.status == ToolStatus::Failed
                        && t.detailed_output
                            .as_ref()
                            .is_some_and(|o| !o.trim().is_empty())
                        || !t.result_summary.is_empty()
                })
                .count();
            height += failed_with_output;

            // Output preview lines for running tools (Goose pattern)
            let running_with_output = tools
                .iter()
                .filter(|t| {
                    t.status == ToolStatus::Running
                        && (t
                            .detailed_output
                            .as_ref()
                            .is_some_and(|o| !o.trim().is_empty())
                            || !t.result_summary.is_empty())
                })
                .count();
            height += running_with_output;

            if message.tools_expansion == ExpansionLevel::Expanded {
                for tool in tools {
                    // Add header lines for input/output
                    if tool.input_json.is_some() {
                        height += 1; // input header
                        height += 15; // JSON content (max)
                    }
                    if let Some(output) = &tool.detailed_output {
                        height += 1; // output header
                        let out_lines = output.lines().count();
                        if out_lines <= 10 {
                            height += out_lines;
                        } else {
                            // Head/tail truncation: head + hidden + tail = max_lines + 1
                            height += 11; // 10 lines + 1 hidden indicator
                        }
                    }
                }
            }
        }

        // Add thinking (capped at 20 display lines, single-pass)
        if let Some(thinking) = &message.thinking {
            if !thinking.is_empty() && message.thinking_expansion == ExpansionLevel::Expanded {
                let think_lines = thinking.lines().take(21).count().min(20);
                height += 1 + think_lines;
            }
        }

        // Turn summary footer (Goose pattern)
        if message.role == MessageRole::Assistant {
            let has_tools = message
                .tool_executions
                .as_ref()
                .is_some_and(|t| !t.is_empty());
            if has_tools {
                height += 1; // Tool summary line
            } else if message.content.lines().count() > 3 {
                height += 1; // Text-only summary line (word count + lines)
            }
        }

        // Add separator
        height += 1;

        height
    }
}
