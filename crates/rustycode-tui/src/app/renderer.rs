use crate::app::event_loop::TUI;
use crate::ui::footer::Footer;
use crate::ui::header::{Header, HeaderStatus};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Clear};
use ratatui::Frame;

/// Shared frame snapshot for renderer backends.
///
/// This stays intentionally small and owned so multiple renderers can
/// consume the same top-level data without borrowing the entire `TUI`.
#[derive(Debug, Clone)]
pub struct RenderContext {
    pub area: Rect,
    pub project_name: String,
    pub task_count: usize,
    pub pending_tools: usize,
    pub header_status: HeaderStatus,
    pub turn_count: usize,
    pub git_branch: Option<String>,
    pub current_model: String,
    pub session_secs: u64,
    pub task_summary: String,
    pub session_cost: f64,
    pub status_bar_collapsed: bool,
    pub footer_collapsed: bool,
}

impl RenderContext {
    pub fn from_tui(tui: &mut TUI, area: Rect) -> Self {
        let project_name = tui
            .services
            .cwd()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let header_status = if tui.error_manager.is_showing() {
            HeaderStatus::Error
        } else if tui.is_streaming {
            if tui.active_tools.is_empty() {
                HeaderStatus::Thinking
            } else {
                HeaderStatus::RunningTools
            }
        } else {
            HeaderStatus::Ready
        };

        let turn_count = tui
            .messages
            .iter()
            .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
            .count();

        let done_count = tui
            .workspace_tasks
            .tasks
            .iter()
            .filter(|t| matches!(t.status, crate::tasks::TaskStatus::Completed))
            .count();
        let pending_count = tui
            .workspace_tasks
            .tasks
            .iter()
            .filter(|t| matches!(t.status, crate::tasks::TaskStatus::Pending))
            .count();
        let task_summary = if done_count > 0 || pending_count > 0 {
            format!("✓{} ☐{}", done_count, pending_count)
        } else {
            String::new()
        };

        let current_model = tui
            .current_model
            .rsplit('/')
            .next()
            .map(|s| s.strip_prefix("claude-").unwrap_or(s))
            .unwrap_or(&tui.current_model)
            .to_string();

        Self {
            area,
            project_name,
            task_count: tui.workspace_tasks.tasks.len(),
            pending_tools: tui.active_tools.len(),
            header_status,
            turn_count,
            git_branch: tui.git_branch.clone(),
            current_model,
            session_secs: tui.start_time.elapsed().as_secs(),
            task_summary,
            session_cost: tui.session_cost_usd,
            status_bar_collapsed: tui.status_bar_collapsed,
            footer_collapsed: tui.footer_collapsed,
        }
    }
}

/// Polished renderer backend.
pub struct PolishedRenderer {
    context: RenderContext,
}

impl PolishedRenderer {
    pub fn from_tui(tui: &mut TUI, area: Rect) -> Self {
        Self {
            context: RenderContext::from_tui(tui, area),
        }
    }

    pub fn render(&self, tui: &mut TUI, frame: &mut Frame) {
        let size = self.context.area;

        // Minimum size guard: if terminal is too small, show a message instead
        if size.width < 40 || size.height < 8 {
            frame.render_widget(Clear, size);
            let msg = ratatui::widgets::Paragraph::new("Terminal too small (min 40×8)")
                .style(Style::default().fg(Color::Yellow));
            frame.render_widget(msg, size);
            return;
        }

        // Clear the entire frame first to prevent text overlap from previous renders
        frame.render_widget(Clear, size);

        // Auto-collapse chrome on small terminals to maximize message space.
        if size.height < 12 {
            tui.status_bar_collapsed = true;
            tui.footer_collapsed = true;
        }

        let status_bar_height = if tui.status_bar_collapsed { 0 } else { 1 };
        let footer_height = if tui.footer_collapsed { 0 } else { 1 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(status_bar_height),
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Length(footer_height),
            ])
            .split(size);

        tui.viewport_height = chunks[2].height.max(1) as usize;
        tui.messages_area.set(chunks[2]);

        let header_bg = Block::default().style(Style::default().bg(Color::Rgb(23, 23, 23)));
        frame.render_widget(header_bg, chunks[0]);

        let header = Header::new()
            .with_app_name("rustycode")
            .with_project_name(self.context.project_name.clone())
            .with_git_branch(self.context.git_branch.clone())
            .with_counts(self.context.task_count, self.context.pending_tools)
            .with_turn_count(self.context.turn_count)
            .with_status(self.context.header_status)
            .with_spinner_frame(tui.animator.current_frame().progress_frame / 5);
        header.render(frame, chunks[0]);

        if !tui.status_bar_collapsed {
            tui.render_status_safe(frame, chunks[1]);
        }

        tui.render_messages_safe(frame, chunks[2]);
        tui.render_input_safe(frame, chunks[3]);

        if !tui.footer_collapsed {
            let footer_bg = Block::default().style(Style::default().bg(Color::Rgb(23, 23, 23)));
            frame.render_widget(footer_bg, chunks[4]);

            let footer = Footer::new()
                .with_session_duration(Footer::format_duration(self.context.session_secs))
                .with_task_summary(self.context.task_summary.clone())
                .with_model(self.context.current_model.clone())
                .with_session_cost(self.context.session_cost);
            footer.render(frame, chunks[4]);
        }

        // Overlay: search box (over message area - chunks[2])
        if tui.search_state.visible {
            tui.render_search_box(frame, chunks[2]);
        }

        if tui.showing_tool_panel {
            tui.render_tool_panel(frame, chunks[2]);
        }

        if tui.worker_panel.visible {
            tui.render_worker_panel(frame, chunks[2]);
        }

        if tui.team_panel.visible {
            frame.render_widget(ratatui::widgets::Clear, chunks[2]);
            frame.render_widget(tui.team_panel.clone(), chunks[2]);
        }

        if tui.awaiting_clarification && tui.clarification_panel.visible {
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let panel_width = (size.width * 3 / 4).min(60);
            let x = (size.width.saturating_sub(panel_width)) / 2;
            let y = (size.height.saturating_sub(panel_height)) / 2;
            let panel_area = Rect::new(x, y, panel_width, panel_height);
            frame.render_widget(Clear, panel_area);
            frame.render_widget(tui.clarification_panel.clone(), panel_area);
        }

        if tui.showing_provider_selector {
            tui.render_provider_selector(frame);
        }

        if tui.file_finder.is_visible() {
            tui.file_finder.render(frame, size);
        }

        if tui.model_selector.is_visible() {
            tui.model_selector.render(frame, size);
        }

        if tui.file_selector.is_visible() {
            tui.file_selector.render(frame, size);
        }

        if tui.skill_palette.is_visible() {
            tui.skill_palette.render(frame, size);
        }

        if tui.theme_preview.is_visible() {
            tui.theme_preview.render(frame, size);
        }

        if tui.command_palette.is_visible() {
            tui.command_palette.render(frame, size);
        }

        if tui.help_state.visible {
            crate::help::render_help(frame, size, &tui.help_state);
        }

        if tui.awaiting_approval {
            if let Some(ref req) = tui.pending_approval_request {
                let panel_height = 12u16.min(size.height.saturating_sub(4));
                let panel_width = 70u16.min(size.width.saturating_sub(4));
                let x = (size.width.saturating_sub(panel_width)) / 2;
                let y = (size.height.saturating_sub(panel_height)) / 2;
                let panel_area = Rect::new(x, y, panel_width, panel_height);
                crate::tool_approval::render_approval_prompt(frame, panel_area, req);
            }
        }

        if tui.error_manager.is_showing() {
            frame.render_widget(Clear, size);
            tui.error_manager.render(frame, size);
        }

        if tui.session_sidebar.is_visible() {
            tui.session_sidebar.render(frame, size);
        }

        if tui.showing_compaction_preview {
            tui.render_compaction_preview(frame, size);
        }

        if tui.wizard.showing_wizard {
            if let Some(ref mut wizard) = tui.wizard.wizard {
                frame.render_widget(Clear, size);
                wizard.render(frame, size);
            }
        }

        tui.toast_manager.render(frame, size);
    }
}

/// Available frame renderers for the TUI.
///
/// Keep this enum small and copyable so the app can select a backend
/// without fighting Rust's borrowing rules when the renderer needs to
/// inspect application state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererMode {
    Polished,
    Brutalist,
}

impl RendererMode {
    pub fn from_brutalist(enabled: bool) -> Self {
        if enabled {
            Self::Brutalist
        } else {
            Self::Polished
        }
    }

    pub fn is_brutalist(self) -> bool {
        matches!(self, Self::Brutalist)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Polished => "polished",
            Self::Brutalist => "brutalist",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Polished => Self::Brutalist,
            Self::Brutalist => Self::Polished,
        }
    }
}

/// Common frame-rendering interface.
///
/// The implementation uses a small enum rather than trait objects so the
/// active renderer can live inside `TUI` without borrow-checker friction.
pub trait FrameRenderer {
    fn render(self, tui: &mut TUI, frame: &mut Frame);
}

impl FrameRenderer for RendererMode {
    fn render(self, tui: &mut TUI, frame: &mut Frame) {
        match self {
            RendererMode::Polished => tui.render_polished(frame),
            RendererMode::Brutalist => tui.render_brutalist(frame),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RendererMode;

    #[test]
    fn toggles_between_backends() {
        assert_eq!(RendererMode::Polished.toggled(), RendererMode::Brutalist);
        assert_eq!(RendererMode::Brutalist.toggled(), RendererMode::Polished);
    }

    #[test]
    fn preserves_mode_labels() {
        assert_eq!(RendererMode::Polished.label(), "polished");
        assert_eq!(RendererMode::Brutalist.label(), "brutalist");
    }
}
