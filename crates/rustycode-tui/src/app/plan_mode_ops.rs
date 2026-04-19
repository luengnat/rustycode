//! Plan mode UI state and helpers.
//!
//! This module keeps the user-facing plan-mode banner state separate from the
//! execution gate itself. The gate lives in `rustycode-orchestra`, while this
//! module manages how the TUI explains planning, stalls, and mode switches.

use crate::app::event_loop::TUI;
use crate::ui::header::HeaderStatus;

/// User-facing plan mode banner shown in the persistent status bar / header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanModeBanner {
    /// Planning is active and the assistant should keep analyzing.
    Planning { action_hint: String },
    /// Planning has stalled on a blocker and needs user action.
    Stalled { action_hint: String },
    /// The assistant believes the plan is complete and is waiting for the
    /// user to switch to implementation mode.
    ReadyToSwitch { summary: String, action_hint: String },
    /// Approval is required for the current plan.
    ApprovalRequired { reason: String, action_hint: String },
}

impl PlanModeBanner {
    /// Banner title shown in the status bar.
    pub(crate) fn title(&self) -> &'static str {
        match self {
            Self::Planning { .. } => "Planning",
            Self::Stalled { .. } => "Plan stalled",
            Self::ReadyToSwitch { .. } => "Ready to switch",
            Self::ApprovalRequired { .. } => "Approval required",
        }
    }

    /// Main descriptive text shown in the status bar.
    pub(crate) fn description(&self) -> String {
        match self {
            Self::Planning { action_hint } => {
                format!("Planning is active. {}", action_hint)
            }
            Self::Stalled { action_hint } => {
                format!("Planning is blocked. {}", action_hint)
            }
            Self::ReadyToSwitch { summary, action_hint } => {
                format!("{} {}", summary, action_hint)
            }
            Self::ApprovalRequired { reason, action_hint } => {
                format!("{} {}", reason, action_hint)
            }
        }
    }

    /// Short user-facing message that can also be surfaced in chat.
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Planning { action_hint } => {
                format!("Plan mode is active. {}", action_hint)
            }
            Self::Stalled { action_hint } => {
                format!("Plan mode is blocked. {}", action_hint)
            }
            Self::ReadyToSwitch { summary, action_hint } => {
                format!("{} {}", summary, action_hint)
            }
            Self::ApprovalRequired { reason, action_hint } => {
                format!("{} {}", reason, action_hint)
            }
        }
    }

    /// Color accent used for the banner.
    pub(crate) fn status_color(&self) -> ratatui::style::Color {
        match self {
            Self::Planning { .. } => ratatui::style::Color::Cyan,
            Self::Stalled { .. } => ratatui::style::Color::Red,
            Self::ReadyToSwitch { .. } => ratatui::style::Color::Yellow,
            Self::ApprovalRequired { .. } => ratatui::style::Color::Magenta,
        }
    }

    /// Header status to use while this banner is active.
    pub(crate) fn header_status(&self) -> HeaderStatus {
        match self {
            Self::Planning { .. } | Self::ReadyToSwitch { .. } => HeaderStatus::Planning,
            Self::Stalled { .. } | Self::ApprovalRequired { .. } => HeaderStatus::Stalled,
        }
    }
}

impl TUI {
    /// Replace the current plan-mode banner.
    pub(crate) fn set_plan_mode_banner(&mut self, banner: Option<PlanModeBanner>) {
        if self.plan_mode_banner == banner {
            return;
        }

        self.plan_mode_banner = banner;
        self.dirty = true;
    }

    /// Clear any active plan-mode banner.
    pub(crate) fn clear_plan_mode_banner(&mut self) {
        self.set_plan_mode_banner(None);
    }

    /// Show that planning mode is active.
    pub(crate) fn show_plan_mode_planning(&mut self) {
        self.set_plan_mode_banner(Some(PlanModeBanner::Planning {
            action_hint: "Use /plan again to switch to implementation mode.".to_string(),
        }));
    }

    /// Show that planning has completed and the user should switch modes.
    pub(crate) fn show_plan_mode_ready_to_switch(&mut self, summary: impl Into<String>) {
        self.set_plan_mode_banner(Some(PlanModeBanner::ReadyToSwitch {
            summary: summary.into(),
            action_hint: "Use /plan to switch to implementation mode.".to_string(),
        }));
    }

    /// Show that planning is stalled and should stop until the user acts.
    pub(crate) fn report_plan_mode_stall(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        tracing::warn!("Plan mode stalled: {}", reason);
        let banner = PlanModeBanner::Stalled {
            action_hint: "Use /plan to switch to implementation mode and continue.".to_string(),
        };
        self.set_plan_mode_banner(Some(banner.clone()));
        let message = banner.message();
        self.add_system_message(message.clone());
        self.toast_manager.warning(message);
    }

    /// Whether a stalled banner is currently active.
    pub(crate) fn is_plan_mode_stalled(&self) -> bool {
        matches!(
            self.plan_mode_banner,
            Some(PlanModeBanner::Stalled { .. })
        )
    }
}
