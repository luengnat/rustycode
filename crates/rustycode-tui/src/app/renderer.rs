//! Renderer dispatch layer for the TUI.
//!
//! # Architecture
//!
//! ```text
//! RendererMode (enum)          — selects the active backend
//! RendererState (struct)       — unified snapshot of TUI state for a frame
//! PolishedRenderer (struct)    — polished backend implementation
//! FrameRenderer (trait)        — dispatch interface (kept small + Copy-friendly)
//! ```
//!
//! Adding a new backend only requires:
//! 1. Implement `rustycode_ui_core::TuiRenderer` for your new struct
//! 2. Add a `RendererMode::YourName` variant
//! 3. Add a match arm in `FrameRenderer::render` on `RendererMode`
//!
//! No other files need to change.

use crate::app::event_loop::TUI;
use crate::app::render::shared::centered_rect;
use crate::ui::footer::Footer;
use crate::ui::header::{Header, HeaderStatus};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Clear};
use ratatui::Frame;
pub use rustycode_ui_core::{RendererFrame, TuiRenderer};

// ============================================================================
// RENDERER STATE — unified snapshot (replaces the old asymmetric RenderContext)
// ============================================================================

/// Unified snapshot of TUI state for a single render frame.
///
/// Extracted once per frame from `TUI` and passed to renderer backends,
/// avoiding the previous pattern where `PolishedRenderer` used `RenderContext`
/// while `BrutalistRenderer` was built via a 25-field builder on `TUI`.
///
/// **Rule:** Fields that are shared between ≥ 2 backends belong here.
/// Backend-specific fields (theme colours, input cursor position, …) stay
/// inside the concrete renderer struct.
#[derive(Debug, Clone)]
pub struct RendererState {
    // ── Layout ──────────────────────────────────────────────────────────────
    /// Full terminal area for the frame.
    pub area: Rect,

    // ── Context strings ──────────────────────────────────────────────────────
    /// Directory basename used as the project label in chrome.
    pub project_name: String,
    /// Current git branch (cached — not re-queried every frame).
    pub git_branch: Option<String>,
    /// Short model name for display (e.g. `"sonnet-4-5"` not the full path).
    pub current_model: String,

    // ── Status ───────────────────────────────────────────────────────────────
    /// High-level header status driven by streaming / error / idle state.
    pub header_status: HeaderStatus,
    /// Number of user turns (= user messages) in the current session.
    pub turn_count: usize,
    /// Number of active tool executions.
    pub pending_tools: usize,

    // ── Tasks ─────────────────────────────────────────────────────────────────
    /// Total task count in workspace.
    pub task_count: usize,
    /// Compact summary string like `"✓3 ☐2"`.
    pub task_summary: String,

    // ── Session ───────────────────────────────────────────────────────────────
    /// Session wall-clock duration in seconds.
    pub session_secs: u64,
    /// Cumulative cost of this session in USD.
    pub session_cost: f64,

    // ── Chrome visibility ────────────────────────────────────────────────────
    /// Whether the status bar / header chrome is collapsed.
    pub status_bar_collapsed: bool,
    /// Whether the footer chrome is collapsed.
    pub footer_collapsed: bool,
}

impl RendererState {
    /// Extract a `RendererState` from live `TUI` state.
    ///
    /// This is the single construction site — both `PolishedRenderer` and
    /// `BrutalistRenderer` call this before they build their own structs.
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
            git_branch: tui.git_branch.clone(),
            current_model,
            header_status,
            turn_count,
            pending_tools: tui.active_tools.len(),
            task_count: tui.workspace_tasks.tasks.len(),
            task_summary,
            session_secs: tui.start_time.elapsed().as_secs(),
            session_cost: tui.session_cost_usd,
            status_bar_collapsed: tui.status_bar_collapsed,
            footer_collapsed: tui.footer_collapsed,
        }
    }

    /// Convert this snapshot into the renderer-agnostic [`RendererFrame`]
    /// from `rustycode-ui-core`.
    pub fn to_renderer_frame(&self) -> RendererFrame {
        RendererFrame::new(self.area)
            .with_project_name(self.project_name.clone())
            .with_git_branch(self.git_branch.clone())
            .with_active_tool_count(self.pending_tools)
            .with_collapsed(self.status_bar_collapsed, self.footer_collapsed)
            .with_streaming(matches!(
                self.header_status,
                HeaderStatus::Thinking | HeaderStatus::RunningTools
            ))
    }
}

// Keep the old type alias so any code referencing `RenderContext` still compiles.
// Deprecated; prefer `RendererState`.
#[deprecated(note = "use RendererState instead")]
pub type RenderContext = RendererState;

// ============================================================================
// POLISHED RENDERER
// ============================================================================

/// Polished renderer backend — clean chrome + markdown-rendered messages.
pub struct PolishedRenderer {
    state: RendererState,
}

impl PolishedRenderer {
    /// Construct a `PolishedRenderer` from live `TUI` state.
    pub fn from_tui(tui: &mut TUI, area: Rect) -> Self {
        Self {
            state: RendererState::from_tui(tui, area),
        }
    }

    pub fn render(&self, tui: &mut TUI, frame: &mut Frame) {
        let size = self.state.area;

        // Minimum size guard
        if size.width < 40 || size.height < 8 {
            frame.render_widget(Clear, size);
            let msg = ratatui::widgets::Paragraph::new("Terminal too small (min 40×8)")
                .style(Style::default().fg(Color::Yellow));
            frame.render_widget(msg, size);
            return;
        }

        frame.render_widget(Clear, size);

        // Auto-collapse chrome on very small terminals
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
            .with_project_name(self.state.project_name.clone())
            .with_git_branch(self.state.git_branch.clone())
            .with_counts(self.state.task_count, self.state.pending_tools)
            .with_turn_count(self.state.turn_count)
            .with_status(self.state.header_status)
            .with_spinner_frame(tui.animator.current_frame().progress_frame / 5);
        header.render(frame, chunks[0]);

        if !tui.status_bar_collapsed {
            self.render_status(tui, frame, chunks[1]);
        }

        self.render_messages(tui, frame, chunks[2]);
        self.render_input(tui, frame, chunks[3]);

        if !tui.footer_collapsed {
            let footer_bg = Block::default().style(Style::default().bg(Color::Rgb(23, 23, 23)));
            frame.render_widget(footer_bg, chunks[4]);

            let footer = Footer::new()
                .with_session_duration(Footer::format_duration(self.state.session_secs))
                .with_task_summary(self.state.task_summary.clone())
                .with_model(self.state.current_model.clone())
                .with_session_cost(self.state.session_cost);
            footer.render(frame, chunks[4]);
        }

        // ── Overlays (rendered last so they appear on top) ──────────────────
        self.render_overlays(tui, frame, size, &chunks);
    }

    /// Render all overlay widgets (search, panels, dialogs, …).
    fn render_overlays(&self, tui: &mut TUI, frame: &mut Frame, size: Rect, chunks: &[Rect]) {
        // Overlay: search box (over message area - chunks[2])
        if tui.search_state.visible {
            crate::app::renderer::render_search_box(tui, frame, chunks[2]);
        }

        if tui.showing_tool_panel {
            crate::app::renderer::render_tool_panel(tui, frame, chunks[2]);
        }

        // Worker status panel overlay (Ctrl+W) - right side overlay
        if tui.worker_panel.visible {
            crate::app::renderer::render_worker_panel(tui, frame, chunks[2]);
        }

        if tui.team_panel.visible {
            frame.render_widget(ratatui::widgets::Clear, chunks[2]);
            frame.render_widget(tui.team_panel.clone(), chunks[2]);
        }

        if tui.awaiting_clarification && tui.clarification_panel.visible {
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let panel_width = (size.width * 3 / 4).min(60);
            let panel_area = centered_rect(panel_width, panel_height, size);
            frame.render_widget(Clear, panel_area);
            frame.render_widget(tui.clarification_panel.clone(), panel_area);
        }

        // Overlay: provider selector
        if tui.showing_provider_selector {
            crate::app::renderer::render_provider_selector(frame);
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
                let panel_area = centered_rect(panel_width, panel_height, size);
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

        // Overlay: compaction preview (while pending)
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

// ============================================================================
// RENDERER MODE — selector enum
// ============================================================================

/// Available frame-renderer backends for the TUI.
///
/// The enum is `Copy` so it can be captured inside closures and passed through
/// channels without fighting borrow-checker friction.
///
/// # Adding a new backend
///
/// 1. Add a variant here (e.g. `Minimal`).
/// 2. Add a `match` arm in `FrameRenderer for RendererMode`.
/// 3. Implement [`TuiRenderer`] for your new renderer struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererMode {
    Polished,
    Brutalist,
}

impl RendererMode {
    /// Select a mode based on a boolean flag (`true` → Brutalist).
    pub fn from_brutalist(enabled: bool) -> Self {
        if enabled {
            Self::Brutalist
        } else {
            Self::Polished
        }
    }

    /// Returns `true` if the active backend is `Brutalist`.
    pub fn is_brutalist(self) -> bool {
        matches!(self, Self::Brutalist)
    }

    /// Short human-readable label used in the command palette and status bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::Polished => "polished",
            Self::Brutalist => "brutalist",
        }
    }

    /// Toggle between the two built-in backends.
    pub fn toggled(self) -> Self {
        match self {
            Self::Polished => Self::Brutalist,
            Self::Brutalist => Self::Polished,
        }
    }
}

// ============================================================================
// FRAME RENDERER TRAIT — dispatch interface
// ============================================================================

/// Common frame-rendering dispatch interface.
///
/// The enum implementation keeps the active backend inside `TUI` without
/// borrow-checker friction (no `Box<dyn …>` required for the built-in
/// variants). External renderers that implement [`TuiRenderer`] from
/// `rustycode-ui-core` are the extension point for the plugin-style API.
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

// ============================================================================
// TESTS
// ============================================================================

// Bring in the modular render implementations for PolishedRenderer
include!("tui_render_impl.rs");

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

    #[test]
    fn from_brutalist_flag() {
        assert_eq!(RendererMode::from_brutalist(true), RendererMode::Brutalist);
        assert_eq!(RendererMode::from_brutalist(false), RendererMode::Polished);
    }
}
