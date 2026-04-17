//! RustyCode TUI — a Ratatui-based terminal interface.

// Service modules (existing, will be refactored to plugin architecture)
// The modules below are already declared with `mod` above line 60
//!
//! Layout:
//! ```text
//! ┌─ RustyCode ──────────────────────────────── branch ─ session ─┐
//! │ Tools        │ Message transcript                              │
//! │ ──────────── │                                                 │
//! │ read_file    │ [you] git_status                                │
//! │ write_file   │ [tool] On branch main...                        │
//! │ list_dir     │                                                 │
//! │ bash         │                                                 │
//! │ grep         │                                                 │
//! │ glob         │                                                 │
//! │ git_status   │                                                 │
//! │ …            │                                                 │
//! ├──────────────┴─────────────────────────────────────────────────┤
//! │ > _                                                            │
//! ├────────────────────────────────────────────────────────────────┤
//! │ i:input  t:tools  q:quit  ↑↓:scroll  Enter:run                │
//! └────────────────────────────────────────────────────────────────┘
//! ```

// Core services (existing, will be refactored to plugin architecture)
pub mod agent_mode;
mod agents;

// New architecture integration modes
pub mod auto_tool_parser;
mod clipboard;
pub mod config;
pub mod conversation_service;
mod error_messages;
mod handlers;

mod logging;
pub mod mcp_mode;
mod memory_auto;
mod memory_command;
mod memory_injection;
mod memory_relevance;
mod providers;
mod session;
pub mod theme;
pub mod tool_helpers;
mod unicode;

// Token autocompaction
mod compaction;

// Slash commands
pub mod slash_commands;
pub mod task_commands;

// Plugin system
pub mod plugin;

// Task management
pub mod extraction_analytics;
mod task_extraction;
pub mod tasks;

// Mistake tracking and recovery
mod mistake_tracker;
pub use mistake_tracker::{Mistake, MistakeTracker, MistakeType, RecoveryStrategy};

// Checkpoint and task resumption
mod checkpoint;
pub use checkpoint::{format_checkpoint, Checkpoint, CheckpointManager, CheckpointMetadata};

// File read deduplication cache
mod file_read_cache;
pub use file_read_cache::{format_repeated_read_warning, FileReadCache, FileReadEntry};

// Tool error handling
mod tool_errors;
pub use tool_errors::{
    format_command_failure, format_file_not_found_error, format_tool_error, ErrorTracker,
    ToolErrorType,
};

// UI components
pub mod ui;

// New modular architecture
pub mod app;

// Workspace context
mod workspace_context;

// Search functionality

// Skills system
pub mod skills;

// Workspace scanning
pub mod workspace_scanner;

// Accessibility features
pub mod accessibility;

// Marketplace system
pub mod marketplace;

// Help system
pub mod help;

// Tool approval system
pub mod tool_approval;

// Observability - dashboard and metrics display
pub mod observability;

// Session recovery and crash detection
pub mod session_recovery;

// Re-exports
pub use agent_mode::AiMode;
pub use ui::input::InputHandler;
pub use ui::{DiffRenderer, MarkdownRenderer, SyntaxHighlighter};

use std::path::PathBuf;

// Core logging and initialization
use crate::logging::{info_log, init, log_level};

// Error handling
use anyhow::Result;

///
/// ✅ **Functional**: Core components and event loop are implemented.
/// The TUI is ready for interactive use.
///
/// Uses the new modular TUI architecture from `app/event_loop.rs`
/// with proper slash command support and service integration.
pub fn run(cwd: PathBuf, reconfigure: bool, resume: bool) -> Result<()> {
    // Short-circuit for headless testing to prevent TUI initialization hang
    if std::env::var("RUSTYCODE_TEST_MODE").is_ok() {
        return Ok(());
    }

    // Initialize logging system first
    if let Err(e) = init() {
        // Use tracing instead of eprintln to avoid screen pollution
        tracing::error!("Failed to initialize logging: {}", e);
    } else {
        info_log!("RustyCode TUI starting");
        info_log!("Working directory: {}", cwd.display());
        debug_log!("Log level: {:?}", log_level());
    }

    // Use the new modular TUI architecture
    use crate::agent_mode::AiMode;
    use crate::app::TUI;

    // Create TUI with default mode
    let mut tui = TUI::new(cwd, AiMode::Ask, reconfigure)?;

    // Initialize background services (graceful fallback — TUI still works without them)
    if let Err(e) = tui.init_services() {
        tracing::warn!(
            "Service initialization failed (TUI will run in degraded mode): {}",
            e
        );
        // Don't return early — the TUI can still display help and accept text input
    }

    // Resume most recent session if requested
    if resume {
        tui.resume_most_recent_session();
    }

    // Run the main event loop
    tui.run()
}
