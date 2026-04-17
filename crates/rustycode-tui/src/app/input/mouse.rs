//! Mouse input handling
//!
//! Handles mouse scroll events and click interactions.
//!
//! # Terminal Text Selection
//!
//! In alternate screen mode, native text selection may be limited depending on the terminal.
//! For best results:
//! - Use keyboard shortcuts for copy: Ctrl+Shift+C (selected), Ctrl+Y (last response), Ctrl+Shift+K (conversation)
//! - Use terminal paste: Ctrl+Shift+V (Linux/Windows), Cmd+V (macOS)
//! - Some terminals (iTerm2, WezTerm, kitty) support native selection with mouse

use crate::app::event_loop::TUI;
use crossterm::event::{MouseEvent, MouseEventKind};

impl TUI {
    /// Handle mouse scroll events
    pub(crate) fn handle_mouse_scroll(&mut self, kind: MouseEventKind) {
        let scroll_speed = self.tui_config.behavior.get_mouse_scroll_speed();
        match kind {
            MouseEventKind::ScrollUp => {
                self.scroll_up_by(scroll_speed as usize);
            }
            MouseEventKind::ScrollDown => {
                self.scroll_down_by(scroll_speed as usize);
            }
            // Ignore other mouse events to allow terminal-native text selection
            // This lets users select text with their mouse in terminals that support it
            _ => {}
        }
    }

    /// Handle mouse click — toggle collapse on clicked message
    pub(crate) fn handle_mouse_click(&mut self, mouse: MouseEvent) {
        let (col, row) = (mouse.column, mouse.row);

        // Check if click is on the scroll-to-bottom indicator
        if self.user_scrolled {
            let msg_area = self.messages_area.get();
            let bottom_row = msg_area.y + msg_area.height.saturating_sub(1);
            if row == bottom_row && col >= msg_area.x && col < msg_area.x + msg_area.width {
                // Click on scroll-to-bottom indicator — jump to bottom
                self.user_scrolled = false;
                self.auto_scroll();
                self.dirty = true;
                return;
            }
        }

        // Find which message was clicked
        let areas = self.message_areas.borrow();
        if let Some(&(msg_idx, _)) = areas
            .iter()
            .find(|(_, rect)| self.point_in_rect((col, row), *rect))
        {
            drop(areas); // Release borrow before mutating messages
            if msg_idx < self.messages.len() {
                // Update selection so keyboard navigation continues from clicked position
                self.selected_message = msg_idx;
                let msg = &mut self.messages[msg_idx];
                // Toggle tool expansion for assistant messages with tools
                // Toggle collapse for all other messages (user and assistant without tools)
                if msg.role == crate::ui::message::MessageRole::Assistant
                    && msg.tool_executions.as_ref().is_some_and(|t| !t.is_empty())
                {
                    msg.tools_expansion = match msg.tools_expansion {
                        crate::ui::message::ExpansionLevel::Collapsed => {
                            crate::ui::message::ExpansionLevel::Expanded
                        }
                        _ => crate::ui::message::ExpansionLevel::Collapsed,
                    };
                } else {
                    msg.collapsed = !msg.collapsed;
                }
                self.dirty = true;
            }
        }
    }
}
