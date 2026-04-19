// Re-use helpers from the shared render module so we have a single
// source of truth instead of duplicated private functions.
// Note: this file is include!()-ed inside event_loop.rs so "super" doesn't
// exist — use the full crate path.
use crate::app::render::shared::{
    estimate_line_count, format_duration_ms, safe_truncate, shorten_path, tool_kind_icon,
};

impl TUI {
    /// Render messages area with line-based auto-scrolling
    pub fn render_messages(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        // Branch to brutalist renderer if enabled
        if self.renderer_mode.is_brutalist() {
            self.render_messages_brutalist(frame, area);
            return;
        }

        use ratatui::layout::Alignment;
        use ratatui::text::Line;
        use ratatui::widgets::Paragraph;
        use rustycode_ui_core::MessageTheme;

        // Clear previous message areas for click detection
        self.clear_message_areas();

        // Calculate how many lines fit in viewport
        let viewport_height = area.height as usize;

        // If no user/assistant conversation yet, show helpful empty state with context
        // (System messages like "Workspace loaded" don't count as conversation)
        let has_conversation = self.messages.iter().any(|m| {
            matches!(
                m.role,
                crate::ui::message::MessageRole::User | crate::ui::message::MessageRole::Assistant
            )
        });
        if !has_conversation {
            let center_y = area.height / 2;
            let mut lines = Vec::new();

            // Add top padding for centering
            for _ in 0..center_y.saturating_sub(5) {
                lines.push(Line::raw(""));
            }

            // ASCII art logo (compact 1-line)
            lines.push(Line::from(vec![
                ratatui::text::Span::styled(
                    "rustycode",
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
                ratatui::text::Span::styled(
                    " v0.1",
                    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                ),
            ]));
            lines.push(Line::raw(""));

            // Context info (claw-code pattern: show model, project, branch)
            let project_name = self
                .services
                .cwd()
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let branch_info = self.git_branch.as_deref().unwrap_or("detached");
            // Stable per-session index derived from project name
            let greeting_idx = project_name.bytes().fold(0u8, |a, b| a.wrapping_add(b)) as usize;

            lines.push(Line::from(vec![
                ratatui::text::Span::styled(
                    "  Model  ",
                    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                ),
                ratatui::text::Span::styled(
                    self.current_model.clone(),
                    ratatui::style::Style::default().fg(ratatui::style::Color::Gray),
                ),
            ]));
            lines.push(Line::from(vec![
                ratatui::text::Span::styled(
                    "  Project ",
                    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                ),
                ratatui::text::Span::styled(
                    project_name.to_string(),
                    ratatui::style::Style::default().fg(ratatui::style::Color::Gray),
                ),
            ]));
            lines.push(Line::from(vec![
                ratatui::text::Span::styled(
                    "  Branch ",
                    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                ),
                ratatui::text::Span::styled(
                    branch_info.to_string(),
                    ratatui::style::Style::default().fg(ratatui::style::Color::Gray),
                ),
            ]));
            lines.push(Line::raw(""));
            {
                // Rotating greeting messages (goose pattern)
                const GREETINGS: &[&str] = &[
                    "What would you like to build?",
                    "Ready to code something amazing?",
                    "What shall we create today?",
                    "Let's write some Rust!",
                    "What's on your mind?",
                    "How can I help you today?",
                    "Ready to ship some features?",
                    "What should we work on?",
                    "Let's get productive!",
                    "Your codebase awaits...",
                ];
                // Stable per session: hash project name for deterministic greeting
                let greeting = GREETINGS[greeting_idx % GREETINGS.len()];
                lines.push(Line::from(vec![ratatui::text::Span::styled(
                    greeting.to_string(),
                    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                )]));
            }
            lines.push(Line::from(vec![ratatui::text::Span::styled(
                {
                    // Rotate through different tips for discoverability
                    // Changes every ~5 seconds based on animation frame
                    const TIPS: &[&str] = &[
                        "? help  ·  / commands  ·  ! bash  ·  Ctrl+K palette",
                        "Ctrl+X editor  ·  Ctrl+S stash  ·  Ctrl+R search history",
                        "Shift+Up/Down = turn jump  ·  Alt+E/W expand/collapse all",
                        "Tab = toggle tools  ·  Ctrl+P tool panel  ·  Ctrl+B sessions",
                        "Ctrl+D to quit  ·  Ctrl+C to cancel  ·  Esc to stop",
                    ];
                    let tip_idx = (greeting_idx
                        + self.animator.current_frame().progress_frame / 20)
                        % TIPS.len();
                    TIPS[tip_idx]
                },
                ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
            )]));

            // Check if API key is missing and show a warning (cached in TUI struct)
            if !self.api_key_warning.is_empty() {
                lines.push(Line::raw(""));
                lines.push(Line::from(vec![ratatui::text::Span::styled(
                    format!("  {}", self.api_key_warning),
                    ratatui::style::Style::default().fg(ratatui::style::Color::Rgb(255, 200, 80)),
                )]));
            }

            let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
            return;
        }

        // Use default theme for rendering
        let theme = MessageTheme::default();

        // Render all messages with vertical border (using MessageRenderer)
        let mut render_chunks: Vec<(usize, ratatui::style::Color, Vec<ratatui::text::Line>)> =
            Vec::new();

        // Pre-estimate total lines to determine which messages are visible.
        // This avoids expensive markdown rendering for messages entirely off-screen.
        let safe_viewport_height = viewport_height.max(1);
        let estimated_auto_scroll_start = if !self.user_scrolled {
            // When auto-scrolled, we only need to render the last N lines that fit
            // in the viewport. Estimate total lines cheaply.
            let mut est_total: usize = 0;
            for msg in &self.messages {
                let msg_lines = estimate_line_count(&msg.content)
                    + msg
                        .thinking
                        .as_ref()
                        .map(|t| estimate_line_count(t) + 4)
                        .unwrap_or(0)
                    + msg
                        .tool_executions
                        .as_ref()
                        .map(|t| t.len() + 2)
                        .unwrap_or(0)
                    + 1; // separator
                est_total += msg_lines.max(1);
            }
            est_total.saturating_sub(safe_viewport_height)
        } else {
            self.scroll_offset_line
        };

        // Track cumulative estimated lines to skip messages above viewport
        let mut est_cumulative: usize = 0;
        let mut all_above_viewport = true;
        // Estimate viewport end to skip messages below it
        let estimated_viewport_end = estimated_auto_scroll_start + safe_viewport_height + 10; // +10 buffer

        for (msg_idx, msg) in self.messages.iter().enumerate() {
            // Get vertical bar style (determines border color)
            let (pipe_char, pipe_color) = msg.pipe_style();

            // Fast skip: estimate this message's line count and skip if entirely
            // above the viewport. This avoids expensive markdown rendering for
            // messages the user can't see (especially important in long conversations).
            let est_msg_lines = if msg.collapsed {
                1
            } else {
                estimate_line_count(&msg.content).max(1)
                    + msg
                        .thinking
                        .as_ref()
                        .map(|t| estimate_line_count(t) + 4)
                        .unwrap_or(0)
                    + msg
                        .tool_executions
                        .as_ref()
                        .map(|t| t.len() + 2)
                        .unwrap_or(0)
            };
            let separator = if msg_idx > 0 { 1 } else { 0 };
            est_cumulative += separator + est_msg_lines;

            if all_above_viewport && est_cumulative < estimated_auto_scroll_start {
                // This message is entirely above the viewport — skip expensive rendering
                continue;
            }
            if est_cumulative >= estimated_auto_scroll_start {
                all_above_viewport = false;
            }

            // Skip messages well below the viewport to avoid expensive markdown rendering
            if est_cumulative > estimated_viewport_end {
                break;
            }

            // Check if message is collapsed
            if msg.collapsed {
                let first_line = msg.content.lines().next().unwrap_or("");
                // For empty content with tool executions, show tool count instead
                let preview = if first_line.is_empty() {
                    if let Some(tools) = &msg.tool_executions {
                        if !tools.is_empty() {
                            format!(
                                "{} tool{}",
                                tools.len(),
                                if tools.len() > 1 { "s" } else { "" }
                            )
                        } else {
                            // Empty content, no tools — show role-based placeholder
                            match msg.role {
                                crate::ui::message::MessageRole::User => {
                                    "(empty message)".to_string()
                                }
                                crate::ui::message::MessageRole::Assistant => {
                                    "(no content)".to_string()
                                }
                                crate::ui::message::MessageRole::System => "(system)".to_string(),
                            }
                        }
                    } else {
                        // Empty content, no tools — show role-based placeholder
                        match msg.role {
                            crate::ui::message::MessageRole::User => "(empty message)".to_string(),
                            crate::ui::message::MessageRole::Assistant => {
                                "(no content)".to_string()
                            }
                            crate::ui::message::MessageRole::System => "(system)".to_string(),
                        }
                    }
                } else if first_line.len() > 60 {
                    // floor_char_boundary ensures we don't slice mid-UTF-8
                    let end = first_line.floor_char_boundary(57);
                    format!("{}...", &first_line[..end])
                } else {
                    first_line.to_string()
                };

                let line = Line::from(vec![
                    ratatui::text::Span::styled(
                        format!("{} ", pipe_char),
                        ratatui::style::Style::default().fg(pipe_color),
                    ),
                    ratatui::text::Span::styled(
                        preview,
                        ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                    ),
                    ratatui::text::Span::styled(
                        " (collapsed)",
                        ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                    ),
                ]);
                render_chunks.push((msg_idx, pipe_color, vec![line]));
            } else {
                // Render markdown content
                let content_lines = self
                    .message_renderer
                    .render_markdown_content(&msg.content, &theme);

                // Collect spans for this message with highlighting
                let mut lines: Vec<ratatui::text::Line> = content_lines
                    .into_iter()
                    .map(|l| ratatui::text::Line::from(l.spans))
                    .collect();

                // Apply search highlighting if search is active
                if !self.search_state.query.is_empty() {
                    lines = self.apply_search_highlighting(&lines, msg_idx);
                }

                // Append thinking block (collapsed header or expanded content)
                // Skip if thinking is empty or whitespace-only
                if let Some(thinking) = &msg.thinking {
                    if !thinking.trim().is_empty() {
                        lines.push(ratatui::text::Line::from(""));
                        lines.extend(render_thinking_block(
                            thinking,
                            msg.thinking_expansion,
                            pipe_char,
                            pipe_color,
                        ));
                    }
                }

                // Append tool execution summary lines
                if let Some(tools) = &msg.tool_executions {
                    if !tools.is_empty() {
                        lines.push(ratatui::text::Line::from(""));
                        if msg.content.trim().is_empty() {
                            lines.push(ratatui::text::Line::from(vec![
                                ratatui::text::Span::styled(
                                    format!(
                                        "  🔧 {} tool{} executed",
                                        tools.len(),
                                        if tools.len() > 1 { "s" } else { "" }
                                    ),
                                    ratatui::style::Style::default()
                                        .fg(ratatui::style::Color::Gray),
                                ),
                            ]));
                        }
                        lines.extend(render_tool_summary(tools));
                    }
                }

                render_chunks.push((msg_idx, pipe_color, lines));
            }
        }

        // Use line-based scroll offset
        // Account for blank separator lines between messages (N-1 messages get a separator)
        // Calculate actual rendered height including wrapped lines
        let available_width = (area.width - 1).max(1) as usize; // Account for left border
        let mut total_lines: usize = 0;

        for (_, _, lines) in &render_chunks {
            for line in lines {
                // Calculate how many terminal rows this line occupies when wrapped
                let line_width = line.width();
                let wrapped_rows = line_width.div_ceil(available_width).max(1);
                total_lines += wrapped_rows;
            }
        }

        // Add separator lines between messages
        total_lines += render_chunks.len().saturating_sub(1);

        // Save total lines for scroll initialization
        self.last_total_lines.set(total_lines);

        // Ensure viewport_height is at least 1 to avoid division issues
        let safe_viewport_height = viewport_height.max(1);

        // Clamp scroll offset to valid range
        let max_scroll = total_lines.saturating_sub(safe_viewport_height);
        let clamped_scroll = self.scroll_offset_line.min(max_scroll);

        let start_line = if self.user_scrolled {
            clamped_scroll
        } else {
            total_lines.saturating_sub(safe_viewport_height)
        };

        // Render with per-message colored prefix (no borders for clean selection)
        let mut current_line = 0;
        let mut y_offset = 0u16;
        let mut rendered_any = false;

        // Record per-message line offsets for accurate search/turn scrolling.
        // Key: indexed by msg_idx (message index), not chunk position,
        // and accounts for line wrapping (div_ceil) to match scroll coordinates.
        {
            let mut offsets = self.message_line_offsets.borrow_mut();
            offsets.clear();
            offsets.resize(self.messages.len(), 0);
            let mut acc = 0usize;
            for (chunk_idx, (msg_idx, _, lines)) in render_chunks.iter().enumerate() {
                let separator = if chunk_idx > 0 { 1 } else { 0 };
                acc += separator;
                offsets[*msg_idx] = acc;
                // Account for line wrapping — each rendered line may occupy
                // multiple terminal rows, matching how total_lines is computed.
                for line in lines {
                    let wrapped_rows = line.width().div_ceil(available_width).max(1);
                    acc += wrapped_rows;
                }
            }
        }

        for (chunk_idx, (msg_idx, border_color, lines)) in render_chunks.iter().enumerate() {
            // Early exit: if we've filled the viewport, stop rendering
            if y_offset >= area.height {
                break;
            }

            let msg_height = lines.len();
            // Separator line before this message (except the first)
            let separator = if chunk_idx > 0 { 1 } else { 0 };

            // Skip lines above scroll offset (including separator)
            if current_line + separator + msg_height <= start_line {
                current_line += separator + msg_height;
                continue;
            }

            // Add spacing between messages (blank line) — only if we already
            // rendered a previous message's visible lines
            if rendered_any && y_offset < area.height {
                y_offset = y_offset.saturating_add(1);
            }

            let visible_start = start_line.saturating_sub(current_line + separator);

            let visible_lines: Vec<_> = lines.iter().skip(visible_start).cloned().collect();
            let visible_count = visible_lines.len();

            if visible_count > 0 {
                // Calculate available height for this message
                let remaining = area.height.saturating_sub(y_offset);
                if remaining == 0 {
                    current_line += separator + msg_height;
                    continue;
                }

                // Add space between border and content
                let spaced_lines: Vec<_> = visible_lines
                    .iter()
                    .map(|line| {
                        let mut styled_spans = vec![ratatui::text::Span::raw(" ")];
                        styled_spans.extend(line.spans.iter().cloned());
                        ratatui::text::Line::from(styled_spans)
                    })
                    .collect();

                // Use LEFT border with color for visual indicator
                let block = ratatui::widgets::Block::default()
                    .borders(ratatui::widgets::Borders::LEFT)
                    .border_style(ratatui::style::Style::default().fg(*border_color));

                // Clamp height to what we actually have
                let render_height = remaining.min(visible_count as u16);

                // Create message area with remaining height
                let msg_area = ratatui::layout::Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: render_height,
                };

                let paragraph = Paragraph::new(spaced_lines)
                    .alignment(Alignment::Left)
                    .block(block)
                    .wrap(ratatui::widgets::Wrap { trim: false });

                // Clear the message area first to prevent text overlap from previous renders
                frame.render_widget(ratatui::widgets::Clear, msg_area);
                frame.render_widget(paragraph, msg_area);

                // Register click area for this message
                self.register_message_area(*msg_idx, msg_area);

                y_offset = y_offset.saturating_add(render_height);
                rendered_any = true;
            }

            current_line += separator + msg_height;
        }

        // Show queued message indicator at bottom when auto-scrolled
        // (goose pattern: dimmed preview of queued message)
        if !self.user_scrolled {
            if let Some(queued) = &self.queued_message {
                if y_offset < area.height.saturating_sub(2) {
                    let preview: String = queued.chars().take(80).collect();
                    let ellipsis = if queued.chars().count() > 80 {
                        "..."
                    } else {
                        ""
                    };
                    let queued_line = Line::from(vec![
                        ratatui::text::Span::styled(
                            " ⏳ ",
                            ratatui::style::Style::default()
                                .fg(ratatui::style::Color::Rgb(180, 180, 255)),
                        ),
                        ratatui::text::Span::styled(
                            format!("Queued: {}{}", preview, ellipsis),
                            ratatui::style::Style::default()
                                .fg(ratatui::style::Color::DarkGray)
                                .add_modifier(ratatui::style::Modifier::DIM),
                        ),
                    ]);
                    let queued_area = ratatui::layout::Rect {
                        x: area.x,
                        y: area.y + y_offset.saturating_add(1),
                        width: area.width,
                        height: 1,
                    };
                    frame.render_widget(
                        Paragraph::new(vec![queued_line]).alignment(Alignment::Left),
                        queued_area,
                    );
                }
            }
        }

        // Goose-inspired viewport overflow indicators
        let overflows = total_lines > safe_viewport_height;
        if overflows && self.user_scrolled && area.height > 2 {
            let above = start_line;
            let below = total_lines.saturating_sub(start_line + safe_viewport_height);

            // Top indicator
            if above > 0 {
                let indicator = format!(" ▲ {} more (↑)", above);
                let top_line = Line::from(vec![ratatui::text::Span::styled(
                    indicator,
                    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                )]);
                let top_area = ratatui::layout::Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: 1,
                };
                frame.render_widget(ratatui::widgets::Clear, top_area);
                frame.render_widget(
                    Paragraph::new(vec![top_line]).alignment(Alignment::Left),
                    top_area,
                );
            }

            // Bottom indicator — more prominent, clickable
            if below > 0 && (y_offset as usize) < area.height as usize {
                let anim_frame = self.animator.current_frame();
                let is_streaming = self.is_streaming;
                // Use a brighter color when streaming to attract attention
                let indicator_color = if is_streaming {
                    let pulse = (anim_frame.progress_frame / 10).is_multiple_of(2);
                    if pulse {
                        ratatui::style::Color::Cyan
                    } else {
                        ratatui::style::Color::DarkGray
                    }
                } else {
                    ratatui::style::Color::DarkGray
                };
                let indicator = if below < 100 {
                    format!(" ▼ {} lines below · End to jump", below)
                } else {
                    format!(
                        " ▼ ~{} lines below · End to jump",
                        (below as f64 / 10.0).round() as usize * 10
                    )
                };
                let bottom_line = Line::from(vec![ratatui::text::Span::styled(
                    indicator,
                    ratatui::style::Style::default().fg(indicator_color),
                )]);
                let bottom_area = ratatui::layout::Rect {
                    x: area.x,
                    y: area.y + area.height.saturating_sub(1),
                    width: area.width,
                    height: 1,
                };
                frame.render_widget(ratatui::widgets::Clear, bottom_area);
                frame.render_widget(
                    Paragraph::new(vec![bottom_line]).alignment(Alignment::Left),
                    bottom_area,
                );
            }
        }

        // Turn indicator when viewing a past turn (goose pattern)
        // Shows "turn X/Y" when user navigated to a historical message
        if self.user_scrolled {
            let total_turns = self
                .messages
                .iter()
                .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
                .count();
            if total_turns > 1 {
                // Find which turn the selected message belongs to
                let current_turn = self.messages[..=self
                    .selected_message
                    .min(self.messages.len().saturating_sub(1))]
                    .iter()
                    .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
                    .count();
                let is_latest = self.selected_message >= self.messages.len().saturating_sub(1);
                if !is_latest && current_turn > 0 {
                    let turn_text =
                        format!(" ◈ turn {}/{} — shift+↓ return ", current_turn, total_turns);
                    let turn_line = Line::from(vec![ratatui::text::Span::styled(
                        turn_text,
                        ratatui::style::Style::default()
                            .fg(ratatui::style::Color::Rgb(255, 200, 80))
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )]);
                    let turn_area = ratatui::layout::Rect {
                        x: area.x,
                        y: area.y + area.height.saturating_sub(2),
                        width: area.width,
                        height: 1,
                    };
                    frame.render_widget(ratatui::widgets::Clear, turn_area);
                    frame.render_widget(
                        Paragraph::new(vec![turn_line]).alignment(Alignment::Center),
                        turn_area,
                    );
                }
            }
        }
    }
    fn render_messages_brutalist(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let input_text = self.input_handler.state.all_text();

        // Compute total lines for scroll system FIRST (needed for auto-scroll calc)
        let width = area.width as usize;
        let safe_viewport = (area.height as usize).max(1);

        // Create renderer for height estimation (offset doesn't affect heights)
        let mut renderer = self.create_brutalist_renderer(&input_text);

        // Compute layout once — reuse for offsets and scroll
        let (total_lines, heights) = renderer.compute_message_layout(width);
        let mut offsets = self.message_line_offsets.borrow_mut();
        offsets.clear();
        offsets.resize(self.messages.len(), 0);
        let mut acc = 0usize;
        for (msg_idx, &h) in heights.iter().enumerate() {
            offsets[msg_idx] = acc;
            acc += h;
        }
        drop(offsets); // Release borrow before render
        self.last_total_lines.set(total_lines);

        // Compute effective scroll offset (auto-scroll to bottom when not user-scrolled)
        let max_scroll = total_lines.saturating_sub(safe_viewport);
        let effective_offset = if self.user_scrolled {
            self.scroll_offset_line.min(max_scroll)
        } else {
            max_scroll // Auto-scroll to bottom
        };

        // Override scroll offset with computed value
        renderer.scroll_offset_line = effective_offset;

        // Use precomputed heights to avoid redundant estimation
        renderer.render_messages_with_heights(frame, area, &heights);

        // Register message areas for click detection using computed offsets and heights
        self.clear_message_areas();
        for (msg_idx, &start_line) in self.message_line_offsets.borrow().iter().enumerate() {
            let msg_height = heights.get(msg_idx).copied().unwrap_or(1);
            let end_line = start_line + msg_height;

            // Skip messages entirely above viewport
            if end_line <= effective_offset {
                continue;
            }
            // Skip messages entirely below viewport
            if start_line >= effective_offset + safe_viewport {
                break;
            }

            // Calculate visible area within the viewport
            let visible_start = start_line.saturating_sub(effective_offset);
            let visible_end = (end_line.saturating_sub(effective_offset)).min(safe_viewport);
            let visible_height = (visible_end.saturating_sub(visible_start)) as u16;

            if visible_height > 0 {
                let msg_area = ratatui::layout::Rect {
                    x: area.x,
                    y: area.y + visible_start as u16,
                    width: area.width,
                    height: visible_height,
                };
                self.register_message_area(msg_idx, msg_area);
            }
        }
    }
}

/// Render a compact tool execution summary for a message.
///
/// Claw-code inspired: shows context-aware tool info with file paths,
/// line counts, and semantic formatting per tool type.
fn render_tool_summary(
    tools: &[crate::ui::message::ToolExecution],
) -> Vec<ratatui::text::Line<'static>> {
    use crate::ui::message::ToolStatus;
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    let mut lines = Vec::new();
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

    // Summary line: ╭─ 3 tools · 2 ok · 1 fail · 450ms ╮
    // Goose pattern: border color reflects overall status (red for failures, gold for running)
    let border_color = if failed > 0 {
        Color::Rgb(255, 80, 80) // Red border when any tool failed
    } else if running > 0 {
        Color::Rgb(255, 200, 80) // Gold border while tools are running
    } else {
        Color::DarkGray
    };
    let mut summary = vec![Span::styled("  ╭─ ", Style::default().fg(border_color))];
    if running > 0 {
        summary.push(Span::styled(
            "◐ ",
            Style::default().fg(Color::Rgb(255, 200, 80)),
        ));
    }
    summary.push(Span::styled(
        format!("{} tool{}", total, if total != 1 { "s" } else { "" }),
        Style::default().fg(Color::Gray),
    ));
    if passed > 0 {
        summary.push(Span::styled(
            format!(" · {} ok", passed),
            Style::default().fg(Color::Rgb(80, 200, 120)),
        ));
    }
    if failed > 0 {
        summary.push(Span::styled(
            format!(" · {} fail", failed),
            Style::default().fg(Color::Rgb(255, 80, 80)),
        ));
    }
    if running > 0 {
        summary.push(Span::styled(
            format!(" · {} running", running),
            Style::default().fg(Color::Rgb(255, 200, 80)),
        ));
    }
    // Show total duration when all tools are complete
    if running == 0 {
        let total_ms: u64 = tools.iter().filter_map(|t| t.duration_ms).sum();
        if total_ms > 0 {
            summary.push(Span::styled(
                format!(" · {}", format_duration(total_ms)),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    lines.push(Line::from(summary));

    // Individual tool lines with context-aware formatting
    for (i, tool) in tools.iter().enumerate() {
        let is_last = i == tools.len() - 1;
        let connector = if is_last { "  ╰─ " } else { "  │ " };

        // Goose pattern: connector color matches tool status
        let connector_color = match tool.status {
            ToolStatus::Failed => Color::Rgb(255, 80, 80),
            ToolStatus::Running => Color::Rgb(255, 200, 80),
            _ => Color::DarkGray,
        };

        let (icon, color) = match tool.status {
            ToolStatus::Running => ("◐", Color::Rgb(255, 200, 80)),
            ToolStatus::Complete => ("●", Color::Rgb(80, 200, 120)),
            ToolStatus::Failed => ("✗", Color::Rgb(255, 80, 80)),
            ToolStatus::Cancelled => ("⚠", Color::Rgb(200, 150, 50)),
        };

        let kind = tool_kind_icon(&tool.name);

        // Extract context-aware detail for this tool (file path, line count, etc.)
        let tool_detail = extract_tool_detail(tool);

        let mut tool_line = vec![
            Span::styled(connector, Style::default().fg(connector_color)),
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled(
                format!("[{}] ", kind),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        // Show tool name with context detail
        if let Some(detail) = &tool_detail {
            tool_line.push(Span::styled(
                tool.name.clone(),
                Style::default().fg(Color::Gray),
            ));
            tool_line.push(Span::styled(
                format!(" {}", detail),
                Style::default().fg(Color::Rgb(180, 180, 180)),
            ));
        } else {
            tool_line.push(Span::styled(
                tool.name.clone(),
                Style::default().fg(Color::Gray),
            ));
        }

        // Duration badge
        if let Some(dur_ms) = tool.duration_ms {
            tool_line.push(Span::styled(
                format!(" {}", format_duration(dur_ms)),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ));
        }

        lines.push(Line::from(tool_line));
    }

    lines
}

/// Extract context-aware detail from a tool execution.
///
/// Claw-code pattern: for file operations, show the file path;
/// for bash, show the command; for search, show match count.
fn extract_tool_detail(tool: &crate::ui::message::ToolExecution) -> Option<String> {
    let lower = tool.name.to_lowercase();
    let summary = &tool.result_summary;

    // File operations: try to extract file path from summary or output
    if lower.contains("read") || lower.contains("cat") || lower.contains("view") {
        return Some(extract_file_path(summary).unwrap_or_else(|| summary.clone()));
    }
    if lower.contains("write") || lower.contains("create") {
        let path = extract_file_path(summary);
        let line_count = tool
            .detailed_output
            .as_ref()
            .map(|o| estimate_line_count(o))
            .unwrap_or(0);
        if let Some(p) = path {
            return Some(if line_count > 0 {
                format!("{} ({} lines)", p, line_count)
            } else {
                p
            });
        }
        return Some(summary.clone());
    }
    if lower.contains("edit") || lower.contains("patch") || lower.contains("replace") {
        return Some(extract_file_path(summary).unwrap_or_else(|| summary.clone()));
    }
    // Bash/shell: show command preview
    if lower.contains("bash") || lower.contains("exec") || lower.contains("shell") {
        // Try to extract the actual command from detailed_output or summary
        if let Some(output) = &tool.detailed_output {
            let first_line = output.lines().next().unwrap_or("");
            if !first_line.is_empty() && first_line.len() < 80 {
                return Some(first_line.to_string());
            }
        }
        return Some(safe_truncate(summary, 60));
    }
    // Search/grep: show match count
    if lower.contains("grep") || lower.contains("search") {
        return Some(safe_truncate(summary, 80));
    }
    // Glob/find: show file count
    if lower.contains("glob") || lower.contains("find") || lower.contains("list") {
        if let Some(output) = &tool.detailed_output {
            let count = estimate_line_count(output);
            return Some(format!("{} files", count));
        }
    }

    None
}

/// Try to extract a file path from a tool result summary string.
fn extract_file_path(s: &str) -> Option<String> {
    // Look for common path patterns in tool summaries:
    // "read_file: src/main.rs (145b)" → "src/main.rs"
    // "write_file: src/tree.rs" → "src/tree.rs"
    // "src/main.rs" → "src/main.rs"

    // Try to extract path after colon separator
    if let Some(colon_pos) = s.find(": ") {
        let after_colon = &s[colon_pos + 2..];
        // Take up to first space that looks like metadata (e.g., "(145b)")
        let path_end = after_colon
            .find(" (")
            .or_else(|| after_colon.find(" ["))
            .unwrap_or(after_colon.len());
        let path = &after_colon[..path_end];
        if !path.is_empty() && (path.contains('/') || path.contains('.') || path.contains('\\')) {
            return Some(shorten_path(path));
        }
    }

    // Check if the whole string looks like a path
    if (s.contains('/') || s.contains('\\')) && !s.contains('\n') && s.len() < 200 {
        return Some(shorten_path(s));
    }

    None
}

/// Goose-inspired smart path shortening for compact tool display.
///
/// Converts home directory paths to `~`, then shortens middle path components
/// to their first character while preserving the filename:
///
/// `/Users/nat/dev/rustycode/crates/main.rs` → `~/d/r/c/main.rs`
/// `src/rustycode_tui/app/render/mod.rs` → `s/r/a/r/mod.rs`
///
/// Paths with ≤ 3 components are left unchanged.
// shorten_path is imported from super::shared at the top of this file.

// tool_kind_icon is imported from super::shared at the top of this file.

/// Format duration for display — thin wrapper over the shared helper.
#[inline]
fn format_duration(ms: u64) -> String {
    format_duration_ms(ms)
}

/// Render a thinking block (collapsed header or expanded content).
///
/// Shows a compact header line with size indicator when collapsed,
/// or a bordered content section when expanded.
fn render_thinking_block(
    thinking: &str,
    expansion: crate::ui::message_types::ExpansionLevel,
    pipe_char: char,
    pipe_color: ratatui::style::Color,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};

    let mut lines = Vec::new();

    let size = thinking.len();
    let size_str = if size == 0 {
        "empty".to_string()
    } else if size < 1024 {
        format!("{}b", size)
    } else {
        format!("{:.1}kb", size as f64 / 1024.0)
    };

    match expansion {
        crate::ui::message_types::ExpansionLevel::Collapsed => {
            // Just header: 💭 [thinking] Nkb [▾ show]
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                Span::styled(
                    format!("💭 [thinking] {} [▾ show]", size_str),
                    Style::default().fg(Color::Rgb(180, 160, 220)),
                ),
            ]));
        }
        crate::ui::message_types::ExpansionLevel::Expanded
        | crate::ui::message_types::ExpansionLevel::Deep => {
            // Header with collapse hint
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                Span::styled(
                    format!("💭 [thinking] {} [▴ hide]", size_str),
                    Style::default().fg(Color::Rgb(180, 160, 220)),
                ),
            ]));

            // Top border
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                Span::styled(
                    format!("┌{}", "─".repeat(30)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));

            // Content lines (max 8, with wrapping).
            // Single-pass iteration: collect up to max+1 lines to detect
            // overflow without iterating the full thinking string (which
            // can be 100KB+ for extended thinking models).
            let max_content_lines = 8;
            let collected: Vec<&str> = thinking.lines().take(max_content_lines + 1).collect();
            let has_more = collected.len() > max_content_lines;

            for content_line in collected.iter().take(max_content_lines) {
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                    Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        content_line.to_string(),
                        Style::default().fg(Color::Rgb(160, 150, 200)),
                    ),
                ]));
            }

            if has_more {
                // Estimate remaining lines from byte ratio to avoid full scan
                let shown_bytes: usize = collected
                    .iter()
                    .take(max_content_lines)
                    .map(|l| l.len())
                    .sum();
                let avg_line_bytes = (shown_bytes / max_content_lines.max(1)).max(1);
                let remaining_bytes = thinking.len().saturating_sub(shown_bytes);
                let estimated_remaining = remaining_bytes / avg_line_bytes;
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                    Span::styled(
                        format!("│ ... ~{} more lines", estimated_remaining),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }

            // Bottom border
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                Span::styled(
                    format!("└{}", "─".repeat(30)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    lines
}

// safe_truncate is imported from super::shared at the top of this file.
