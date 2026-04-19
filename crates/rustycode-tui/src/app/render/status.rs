impl crate::app::renderer::PolishedRenderer {
    pub fn render_status(&self, tui: &mut crate::app::event_loop::TUI, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        use ratatui::style::Color;
        use ratatui::style::Style;
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;
        use crate::app::plan_mode_ops::PlanModeBanner;

        let anim_frame = tui.animator.current_frame();
        let width = area.width as usize;

        // Width tiers for progressive indicator hiding
        let show_agent_mode = width >= 60;
        let show_context_bar = width >= 70;
        let show_cost = width >= 85;
        let show_git_branch = width >= 90;
        let _show_scroll_pos = true; // Always show when user scrolled
        let show_task_counts = width >= 80;

        // Determine current status and create appropriate spinner.
        // Plan-mode banners take priority because they are the most important
        // user-facing state changes in this workflow.
        let status = if let Some(banner) = tui.plan_mode_banner.clone() {
            match banner {
                PlanModeBanner::Planning { .. } | PlanModeBanner::ReadyToSwitch { .. } => {
                    RenderStatus::Planning { banner }
                }
                PlanModeBanner::Stalled { .. } => RenderStatus::Stalled { banner },
                PlanModeBanner::ApprovalRequired { .. } => RenderStatus::Stalled { banner },
            }
        } else if tui.is_streaming {
            RenderStatus::Thinking {
                chunks_received: tui.chunks_received,
            }
        } else if !tui.active_tools.is_empty() {
            // Collect all running tool names from active_tools map
            let tool_names: Vec<String> = tui.active_tools.keys().take(3).cloned().collect();
            let remaining = tui.active_tools.len().saturating_sub(3);

            RenderStatus::RunningTools {
                count: tui.active_tools.len(),
                tool_names,
                remaining,
            }
        } else {
            RenderStatus::Idle
        };

        // Build status line based on current state
        let mut spans = Vec::new();

        match status {
            RenderStatus::Planning { banner } => {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let frame_idx = (anim_frame.progress_frame / 5) % frames.len();
                spans.push(Span::styled(
                    format!("{} {} ", frames[frame_idx], banner.title()),
                    Style::default()
                        .fg(banner.status_color())
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    banner.description(),
                    Style::default().fg(banner.status_color()),
                ));
            }
            RenderStatus::Stalled { banner } => {
                spans.push(Span::styled(
                    format!("⚠ {} ", banner.title()),
                    Style::default()
                        .fg(banner.status_color())
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    banner.description(),
                    Style::default().fg(banner.status_color()),
                ));
            }
            RenderStatus::Thinking { chunks_received } => {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let frame_idx = (anim_frame.progress_frame / 5) % frames.len();
                // Cycle through themed thinking messages every ~3 seconds (60 frames at 20fps)
                let thinking_msg = crate::app::thinking_messages::get_thinking_message(
                    anim_frame.progress_frame / 60
                );
                spans.push(Span::styled(
                    format!("{} {} ", frames[frame_idx], thinking_msg),
                    Style::default().fg(Color::Cyan),
                ));
                if chunks_received > 0 {
                    spans.push(Span::styled(
                        format!("({} chunks)", chunks_received),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
            RenderStatus::RunningTools { count, tool_names, remaining } => {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let frame_idx = (anim_frame.progress_frame / 5) % frames.len();
                spans.push(Span::styled(
                    format!(
                        "{} Running {} tool{}",
                        frames[frame_idx],
                        count,
                        if count > 1 { "s" } else { "" }
                    ),
                    Style::default().fg(Color::Yellow),
                ));
                if !tool_names.is_empty() {
                    let names_display = tool_names.join(", ");
                    spans.push(Span::styled(
                        format!(": {}", names_display),
                        Style::default().fg(Color::DarkGray),
                    ));
                    if remaining > 0 {
                        spans.push(Span::styled(
                            format!(" +{} more", remaining),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }
            }
            RenderStatus::Idle => {
                spans.push(Span::styled("✓ Ready", Style::default().fg(Color::Green)));
                // Show last response duration (Goose pattern: response timing)
                if let Some(dur) = tui.last_response_duration {
                    spans.push(Span::styled(
                        format!(" {}", format_response_duration(dur)),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }

        // Turn counter (goose pattern) — count user+assistant message pairs
        let turn_count = tui.messages.iter()
            .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
            .count();
        if turn_count > 0 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("{} turn{}", turn_count, if turn_count != 1 { "s" } else { "" }),
                Style::default().fg(Color::DarkGray),
            ));
        }

        // Add separator
        spans.push(Span::raw(" | "));

        // Show workspace scan progress if active
        if let Some((scanned, total)) = tui.workspace_scan_progress {
            let pct = if total > 0 {
                (scanned as f64 / total as f64 * 100.0) as u8
            } else {
                0
            };
            spans.push(Span::styled(
                format!("🔍 Scanning... {}% ({}/{})", pct, scanned, total),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
            spans.push(Span::raw(" | "));
        }

        // Show rate limit countdown if active
        if let Some(until) = tui.rate_limit.until {
            let remaining = until.saturating_duration_since(std::time::Instant::now());
            let remaining_secs = remaining.as_secs();
            if remaining_secs > 0 {
                spans.push(Span::styled(
                    format!("⏱️ Rate limit: {}s ", remaining_secs),
                    Style::default().fg(Color::Red),
                ));
                spans.push(Span::raw(" | "));
            }
        }

        // Show input mode
        let mode_text = match tui.input_mode {
            crate::ui::input::InputMode::SingleLine => "📝 Single-line",
            crate::ui::input::InputMode::MultiLine => "📄 Multi-line",
        };
        spans.push(Span::styled(mode_text, Style::default().fg(Color::Blue)));

        // Show agent mode (hide on narrow terminals)
        if show_agent_mode {
            let mode = tui.services.agent_mode();
            spans.push(Span::raw(" | "));
            spans.push(Span::styled(
                format!("🔧 {}", mode.display_name()),
                Style::default().fg(Color::Magenta),
            ));
        }

        // Show task/agent/todo indicators if any are active
        let agents = tui.agent_manager.get_agents();
        let running_agents: Vec<_> = agents
            .iter()
            .filter(|a| matches!(a.status, crate::agents::AgentStatus::Running))
            .collect();

        let in_progress_tasks = tui.workspace_tasks
            .tasks
            .iter()
            .filter(|t| matches!(t.status, crate::tasks::TaskStatus::InProgress))
            .count();

        let pending_todos = tui.workspace_tasks
            .todos
            .iter()
            .filter(|t| !t.done)
            .count();

        if show_task_counts && (!running_agents.is_empty() || in_progress_tasks > 0 || pending_todos > 0) {
            spans.push(Span::raw(" | "));
            let mut activity_spans = Vec::new();

            if !running_agents.is_empty() {
                // Show running agents with elapsed time of longest-running one
                let elapsed =
                    if let Some(longest) = running_agents.iter().max_by_key(|a| a.elapsed_secs) {
                        format!(" ({})", longest.formatted_time())
                    } else {
                        String::new()
                    };
                activity_spans.push(Span::styled(
                    format!("🤖{}{}", running_agents.len(), elapsed),
                    Style::default().fg(Color::Yellow),
                ));
            }

            if in_progress_tasks > 0 {
                if !activity_spans.is_empty() {
                    activity_spans.push(Span::raw(" "));
                }
                activity_spans.push(Span::styled(
                    format!("🔄{}", in_progress_tasks),
                    Style::default().fg(Color::Yellow),
                ));
            }

            if pending_todos > 0 {
                if !activity_spans.is_empty() {
                    activity_spans.push(Span::raw(" "));
                }
                activity_spans.push(Span::styled(
                    format!("☐{}", pending_todos),
                    Style::default().fg(Color::White),
                ));
            }

            spans.push(Span::raw(""));
            spans.extend(activity_spans);
        }

        // Context usage with goose-style progress bar + token counts (hide on narrow terminals)
        if show_context_bar {
            let usage_pct = (tui.context_monitor.usage_percentage() * 100.0) as usize;
            // Always show model name + context bar (even at 0% so user sees what model is active)
            spans.push(Span::raw(" | "));
            let token_color = if usage_pct < 50 {
                Color::Green
            } else if usage_pct < 80 {
                Color::Yellow
            } else {
                Color::Red
            };
            // Visual progress bar (10 segments, each = 10%)
            let bar_width = 10;
            let filled = if usage_pct > 0 {
                usize::div_ceil(usage_pct * bar_width, 100).min(bar_width)
            } else {
                0
            };
            let empty = bar_width - filled;
            let bar = format!("{}{}", "━".repeat(filled), "╌".repeat(empty));
            // Format token counts (Goose pattern: k/M suffixes)
            let fmt_tokens = |n: usize| -> String {
                if n >= 1_000_000 {
                    format!("{:.1}M", n as f64 / 1_000_000.0)
                } else if n >= 1_000 {
                    format!("{:.0}k", n as f64 / 1_000.0)
                } else {
                    n.to_string()
                }
            };
            let current_tokens = tui.context_monitor.current_tokens;
            let max_tokens = tui.context_monitor.max_tokens;
            // Shorten model name for display (take last segment after '/')
            let display_model = tui.current_model
                .rsplit('/')
                .next()
                .map(|s| {
                    if let Some(stripped) = s.strip_prefix("claude-") {
                        stripped
                    } else {
                        s
                    }
                })
                .unwrap_or(&tui.current_model);
            spans.push(Span::styled(bar, Style::default().fg(token_color)));
            spans.push(Span::raw(" "));
            // Show token counts on wide terminals, model name on narrow
            if width >= 100 && max_tokens > 0 {
                spans.push(Span::styled(
                    format!("{}/{}", fmt_tokens(current_tokens), fmt_tokens(max_tokens)),
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                spans.push(Span::styled(
                    display_model.to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }

        // Session cost display (hide on narrow terminals)
        if show_cost && tui.session_cost_usd > 0.0 {
            let cost_str = if tui.session_cost_usd < 0.01 {
                format!("${:.4}", tui.session_cost_usd)
            } else if tui.session_cost_usd < 1.0 {
                format!("${:.3}", tui.session_cost_usd)
            } else {
                format!("${:.2}", tui.session_cost_usd)
            };
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                cost_str,
                Style::default().fg(Color::Yellow),
            ));
        }

        // Git branch indicator (hide on narrow terminals)
        if show_git_branch {
            if let Some(branch) = &tui.git_branch {
                // Truncate long branch names to prevent overflow
                let display_branch = if branch.len() > 25 {
                    format!("{}…", &branch[..branch.floor_char_boundary(24)])
                } else {
                    branch.clone()
                };
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("| {} ", display_branch),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ));
            }
        }

        // Scroll position indicator (shows position when user has scrolled)
        if tui.user_scrolled {
            let total = tui.last_total_lines.get();
            if total > 0 {
                let safe_viewport = tui.viewport_height.max(1);
                let max_scroll = total.saturating_sub(safe_viewport);
                if max_scroll > 0 {
                    // Clamp offset to valid range (may be stale if messages changed)
                    let offset = tui.scroll_offset_line.min(max_scroll);
                    let pos_label = if offset == 0 {
                        "Top".to_string()
                    } else if offset >= max_scroll {
                        "Bot".to_string()
                    } else {
                        // Show fraction of total lines
                        let current = offset + safe_viewport;
                        format!("{}/{}", current.min(total), total)
                    };
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(pos_label, Style::default().fg(Color::DarkGray)));
                }
            }
        }

        // Team mode indicator (shows when orchestrator is running)
        if tui.team_handler.event_rx.is_some() {
            spans.push(Span::raw(" "));
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame_idx = (anim_frame.progress_frame / 5) % frames.len();
            let active_agent = tui.team_panel.active_agent_name();
            if let Some(agent) = active_agent {
                // Truncate long agent names to prevent overflow
                let display_agent = if agent.len() > 15 {
                    format!("{}…", &agent[..agent.floor_char_boundary(14)])
                } else {
                    agent.clone()
                };
                spans.push(Span::styled(
                    format!("{}TEAM {}{}", frames[frame_idx], tui.team_panel.current_turn(), display_agent),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(
                    format!("{}TEAM T:{}/{} Tr:{:.0}%",
                        frames[frame_idx],
                        tui.team_panel.current_turn(),
                        tui.team_panel.max_turns(),
                        tui.team_panel.trust_value() * 100.0,
                    ),
                    Style::default().fg(Color::Cyan),
                ));
            }
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }

}

/// Format response duration for status bar (Goose pattern).
///
/// Shows human-friendly timing: "<1s", "3.2s", "1m05s"
fn format_response_duration(dur: std::time::Duration) -> String {
    let secs = dur.as_secs();
    let ms = dur.as_millis();
    if ms < 1000 {
        format!("{}ms", ms)
    } else if secs < 60 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let mins = secs / 60;
        let remain = secs % 60;
        format!("{}m{:02}s", mins, remain)
    }
}
