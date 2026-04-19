//! Main application module
//!
//! Modular TUI architecture with:
//! - Responsive event loop (one-item-per-frame processing)
//! - State machine for request management
//! - Async channel system with backpressure handling
//! - Service integration layer for LLM, tools, and workspace
//! - Clean separation of UI, business logic, and async communication

// New modular components (refactored from event_loop mega-file)
pub mod brutalist_helpers;
pub mod brutalist_renderer;
pub mod clipboard_export;
pub mod clipboard_ops;
pub mod commands;
pub mod context_usage;
pub mod doom_loop;
pub mod event_loop_agents;
pub mod event_loop_commands;
pub mod event_loop_input;
pub mod event_loop_render;
pub mod input;
pub mod keyboard_shortcuts;
pub mod memory_manager;
pub mod memory_ops;
pub mod message_management;
pub mod message_ops;
pub mod rate_limit;
pub mod rate_limit_handler;
pub mod renderer;
pub mod scrolling_ops;
pub mod service_polling;
pub mod session_recovery_integration;
pub mod state_manager;
pub mod team_mode_handler;
pub mod turn_snapshot;
pub mod wizard_handler;
pub mod workspace_manager;

pub mod storage_bridge;
pub mod streaming_render_buffer;
pub mod task_dashboard;
pub mod thinking_messages;
pub mod tool_confirmation_router;
pub mod tool_output_format;

// Legacy modules (to be further refactored)
pub mod async_;
pub mod event_loop;
pub mod handlers;
pub mod service_integration;
pub mod streaming;

// render sub-module — shared helpers + per-section render files
pub mod render {
    pub mod shared;
}

// Tests
#[cfg(test)]
mod event_loop_tests;

// Re-exports
pub use async_::*;
pub use event_loop::TUI;
pub use event_loop_agents::*;
pub use event_loop_commands::*;
pub use event_loop_input::*;
pub use event_loop_render::*;
pub use keyboard_shortcuts::{KeyboardAction, KeyboardShortcutHandler};
pub use memory_manager::MemoryManager;
pub use service_integration::*;
pub use session_recovery_integration::{SessionRecoveryConfig, SessionRecoveryManager};
pub use state_manager::StateManager;

use std::time::Duration;

/// Frame budget for 60 FPS
pub const FRAME_BUDGET_60FPS: Duration = Duration::from_millis(16);

/// Maximum acceptable input latency (50ms = 20 FPS minimum)
pub const MAX_INPUT_LATENCY: Duration = Duration::from_millis(50);

/// Maximum number of undo entries to keep in the undo stack
pub const MAX_UNDO_ENTRIES: usize = 5;
