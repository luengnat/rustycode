// BrutalistRenderer construction helper — single source of truth for all parameters.
//
// Use create_brutalist_renderer() instead of BrutalistRenderer::new() directly —
// it ensures all fields are consistently populated from live TUI state.

use crate::app::brutalist_renderer::{BrutalistRenderer, BrutalistRendererBuilder};
use crate::app::context_usage::ContextUsage;

impl super::TUI {
    /// Create a BrutalistRenderer populated with current session data.
    ///
    /// `input_text` must be passed in because the renderer borrows it.
    /// Get it via `self.input_handler.state.all_text()` before calling.
    pub(crate) fn create_brutalist_renderer<'a>(
        &'a self,
        input_text: &'a str,
    ) -> BrutalistRenderer<'a> {
        let mut context_usage = ContextUsage::new();
        context_usage.update(self.session_input_tokens, self.session_output_tokens);
        context_usage.set_limit(self.context_monitor.max_tokens);

        let agent_status = if self.is_streaming {
            "thinking"
        } else if !self.active_tools.is_empty() {
            "tools"
        } else {
            "ready"
        };
        let auto_memory_status = if self.auto_memory.is_some() {
            "on"
        } else {
            "off"
        };

        let active_tool_count = self.active_tools.len();
        // Show all running tool names (not just the first one)
        let active_tool_names: String = self
            .active_tools
            .keys()
            .take(3) // Cap at 3 to avoid overflowing status bar
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let remaining = active_tool_count.saturating_sub(3);
        let active_tool_display = if remaining > 0 {
            format!("{}, +{} more", active_tool_names, remaining)
        } else {
            active_tool_names
        };
        let input_line_count = input_text.lines().count().max(1);

        // Compute stream elapsed time for live timing display (Goose pattern)
        let stream_elapsed = self.stream_start_time.map(|t| t.elapsed());

        // History/reverse search state for input bar display
        let (reverse_query, reverse_match, reverse_total) =
            self.input_handler.reverse_search_info();
        let (hist_pos, hist_total) = self.input_handler.history_position();

        BrutalistRendererBuilder::new(&self.messages, input_text)
            .stream_content(&self.current_stream_content)
            .cwd(self.services.cwd().clone())
            .is_streaming(self.is_streaming)
            .scroll(self.scroll_offset_line, self.user_scrolled)
            .selection(self.selected_message, self.viewport_height)
            .theme(self.theme_colors.clone())
            .statuses(agent_status, auto_memory_status)
            .input_mode(self.input_mode)
            .rate_limit(self.rate_limit.until)
            .streaming_state(
                self.chunks_received,
                self.animator.current_frame().progress_frame,
            )
            .context_usage(context_usage)
            .tool_status(active_tool_count, active_tool_display)
            .session_info(
                self.session_cost_usd,
                self.session_input_tokens,
                self.session_output_tokens,
                &self.current_model,
            )
            .warnings(self.api_key_warning.clone())
            .collapsed(self.status_bar_collapsed, self.footer_collapsed)
            .input_state(
                input_line_count,
                self.queued_message.is_some(),
                self.queued_message.as_deref().unwrap_or("").to_string(),
            )
            .timing(self.last_response_duration, stream_elapsed)
            .git_branch(self.git_branch.as_deref().unwrap_or(""))
            .reverse_search(reverse_query, reverse_match, reverse_total)
            .history_browsing(hist_pos, hist_total)
            .search(
                self.search_state.query.clone(),
                self.search_state.matches.clone(),
                self.search_state.current_match_index,
            )
            .session_start(Some(self.start_time))
            .cursor_position(
                self.input_handler.state.cursor_col,
                self.input_handler.state.cursor_row,
            )
            .build()
    }
}
