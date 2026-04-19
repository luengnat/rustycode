impl TUI {
    /// Render input area with label and keyboard hints
    pub fn render_input(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        // Branch to brutalist renderer if enabled
        if self.renderer_mode.is_brutalist() {
            self.render_input_brutalist(frame, area);
            return;
        }

        use ratatui::style::Color;
        use ratatui::style::Style;
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;

        let is_multiline =
            self.input_handler.state.mode == crate::ui::input::InputMode::MultiLine;

        // Top label row - shows context/mode
        let label_area = ratatui::layout::Rect::new(area.x, area.y, area.width, 1);

        // Input content row(s)
        let input_area = ratatui::layout::Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(2),
        );

        // Bottom hints row - shows keyboard shortcuts
        let hints_area = ratatui::layout::Rect::new(
            area.x,
            area.y + area.height.saturating_sub(1),
            area.width,
            1,
        );

        // Render label — show streaming indicator when AI is generating
        let label_spans = if self.is_streaming {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let anim_frame = self.animator.current_frame();
            let frame_idx = (anim_frame.progress_frame / 5) % frames.len();

            // Use shared thinking messages module (cycle every ~2s at 4FPS)
            let msg_idx = anim_frame.progress_frame / 8;

            let mut spans = vec![
                Span::styled("│", Style::default().fg(Color::DarkGray)),
                Span::styled(" ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("{} {}", frames[frame_idx], crate::app::thinking_messages::get_thinking_message(msg_idx)),
                    Style::default().fg(Color::Rgb(255, 200, 80)), // Amber
                ),
                Span::styled("  Ctrl+C cancel", Style::default().fg(Color::DarkGray)),
            ];
            // Show queue state hint so users know they can type ahead
            if self.queued_message.is_some() {
                spans.push(Span::styled(
                    "  📝 1 queued",
                    Style::default().fg(Color::Rgb(180, 180, 255)),
                ));
            } else {
                spans.push(Span::styled(
                    "  type to queue",
                    Style::default().fg(Color::Rgb(120, 120, 140)),
                ));
            }
            spans
        } else {
            let mut spans = vec![
                Span::styled("│", Style::default().fg(Color::DarkGray)),
                Span::styled(" ", Style::default().fg(Color::White)),
                Span::styled(
                    if is_multiline { "📄 Multi-line" } else { "📝 Single-line" },
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(" ", Style::default().fg(Color::DarkGray)),
            ];
            // Show image attachment count when images are attached
            let img_count = self.input_handler.state.images.len();
            if img_count > 0 {
                spans.push(Span::styled(
                    format!(" 🖼 {} ", img_count),
                    Style::default().fg(Color::Rgb(255, 200, 80)),
                ));
            }
            // Show reverse search indicator (readline Ctrl+R)
            let (rs_query, _rs_match, rs_total) = self.input_handler.reverse_search_info();
            if !rs_query.is_empty() {
                spans.push(Span::styled(
                    format!(" 🔍 '{}/{} ", rs_query, rs_total),
                    Style::default().fg(Color::Rgb(255, 200, 80)),
                ));
            } else {
                // Show history browsing position
                let (hist_pos, hist_total) = self.input_handler.history_position();
                if hist_pos > 0 {
                    spans.push(Span::styled(
                        format!(" 📜 {}/{} ", hist_pos, hist_total),
                        Style::default().fg(Color::Rgb(120, 160, 200)),
                    ));
                }
            }
            // Character count for long messages (goose pattern: show when > 500 chars)
            let char_count: usize = self.input_handler.state.all_text().chars().count();
            if char_count > 500 {
                let count_color = if char_count > 5000 {
                    Color::Red
                } else if char_count > 2000 {
                    Color::Yellow
                } else {
                    Color::DarkGray
                };
                let fmt_count = if char_count >= 10_000 {
                    format!("{:.1}k", char_count as f64 / 1000.0)
                } else {
                    char_count.to_string()
                };
                spans.push(Span::styled(
                    format!(" {} chars", fmt_count),
                    Style::default().fg(count_color),
                ));
            }
            spans
        };
        let label = Paragraph::new(Line::from(label_spans));
        frame.render_widget(label, label_area);

        // Collect all lines for display
        let state = &self.input_handler.state;
        let lines = &state.lines;
        let cursor_row = state.cursor_row.min(lines.len().saturating_sub(1));
        let cursor_col = state.cursor_col;

        // Build visible lines (show up to area.height lines in multiline mode)
        let max_display_lines = if is_multiline {
            input_area.height as usize
        } else {
            1
        };

        // Calculate which lines to show (scroll if needed)
        let start_row = if is_multiline && cursor_row >= max_display_lines {
            cursor_row - max_display_lines + 1
        } else {
            0
        };

        let mut display_lines = Vec::new();

        for (row_idx, row) in lines.iter().enumerate().skip(start_row).take(max_display_lines) {
            let mut spans = vec![];

            // Add line number in multiline mode
            if is_multiline {
                let line_num = row_idx + 1;
                spans.push(Span::styled(
                    format!("{:>2} ", line_num),
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                // Goose pattern: cranberry ❯ prompt for distinctive visual identity
                spans.push(Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))));
                spans.push(Span::raw(" "));
            }

            if row_idx == cursor_row {
                // Split at cursor for cursor rendering
                let col = cursor_col.min(row.len());
                let (before, after) = row.split_at(col);

                if !before.is_empty() {
                    spans.push(Span::raw(before.to_string()));
                }

                // Blinking cursor
                let cursor_visible = (self.animator.frame_count() / 2).is_multiple_of(2);
                if cursor_visible {
                    spans.push(Span::styled("▏", Style::default().fg(Color::White)));
                } else {
                    spans.push(Span::styled("▏", Style::default().fg(Color::DarkGray)));
                }

                if !after.is_empty() {
                    spans.push(Span::raw(after.to_string()));
                } else if row.is_empty() && !is_multiline && !self.is_streaming {
                    // Show placeholder when cursor line is empty (kilocode pattern)
                    let placeholder = if self.messages.is_empty() {
                        " Ask me anything..."
                    } else {
                        " Message..."
                    };
                    spans.push(Span::styled(
                        placeholder,
                        Style::default().fg(Color::Rgb(80, 80, 100)),
                    ));
                }
            } else {
                spans.push(Span::raw(row.clone()));
            }

            display_lines.push(Line::from(spans));
        }

        // Ensure at least one line is rendered (with context-aware placeholder)
        if display_lines.is_empty() {
            let mut spans = vec![Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))), Span::raw(" ")];
            let cursor_visible = (self.animator.frame_count() / 2).is_multiple_of(2);
            if cursor_visible {
                spans.push(Span::styled("▏", Style::default().fg(Color::White)));
            } else {
                spans.push(Span::styled("▏", Style::default().fg(Color::DarkGray)));
            }
            // Context-aware placeholder (kilocode pattern)
            let placeholder = if self.is_streaming {
                "" // No placeholder during streaming — spinner is in label
            } else if self.messages.is_empty() {
                " Ask me anything..."
            } else {
                " Message..."
            };
            if !placeholder.is_empty() {
                spans.push(Span::styled(
                    placeholder,
                    Style::default().fg(Color::Rgb(80, 80, 100)),
                ));
            }
            display_lines.push(Line::from(spans));
        }

        let paragraph = Paragraph::new(display_lines);
        frame.render_widget(paragraph, input_area);

        // Render keyboard hints (right-aligned, hidden on narrow terminals)
        let send_hint = if self.is_streaming { "⏎ Queue" } else { "⏎ Send" };
        let mode_hint = if is_multiline { "Ctrl+J" } else { "" };

        // Goose pattern: show scroll hint when viewport is scrolled up
        let scroll_hint = if self.user_scrolled {
            "Home/End = top/bottom"
        } else {
            ""
        };

        if area.width > 70 {
            // Show context-appropriate hints
            let hints_text = if self.is_streaming {
                "Ctrl+C cancel · ↑↓ scroll"
            } else {
                "Ctrl+A/E nav · Ctrl+U/D scroll · Ctrl+X edit · Ctrl+R search"
            };
            let spacer_len = area.width.saturating_sub(68) as usize;

            let mut hints = vec![
                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                Span::styled(hints_text, Style::default().fg(Color::DarkGray)),
            ];
            // Show scroll hint if scrolled up (goose pattern)
            if !scroll_hint.is_empty() {
                hints.push(Span::raw(" ".repeat(spacer_len.min(4))));
                hints.push(Span::styled(scroll_hint, Style::default().fg(Color::Cyan)));
                hints.push(Span::raw(" ".repeat(spacer_len.saturating_sub(scroll_hint.len() + 4))));
            } else {
                hints.push(Span::raw(" ".repeat(spacer_len)));
            }
            hints.push(Span::styled(send_hint, Style::default().fg(Color::Green)));
            hints.push(Span::raw("  "));
            hints.push(Span::styled(mode_hint, Style::default().fg(Color::DarkGray)));
            hints.push(Span::styled(" │", Style::default().fg(Color::DarkGray)));
            frame.render_widget(Paragraph::new(Line::from(hints)), hints_area);
        } else {
            // Compact hints on narrow terminals
            let hints = vec![
                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                Span::raw(" ".repeat(area.width.saturating_sub(12) as usize)),
                Span::styled(send_hint, Style::default().fg(Color::Green)),
                Span::styled(" │", Style::default().fg(Color::DarkGray)),
            ];
            frame.render_widget(Paragraph::new(Line::from(hints)), hints_area);
        }
    }

    /// Render input using brutalist renderer
    fn render_input_brutalist(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let input_text = self.input_handler.state.all_text();
        let renderer = self.create_brutalist_renderer(&input_text);
        let colors = self
            .theme_colors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        renderer.render_input(frame, area, &colors);
    }
}
