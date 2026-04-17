// Render implementation for TUI
//
// This module contains the render methods for the TUI struct,
// split into sub-files for maintainability.
//
// Note: This file is included directly in event_loop.rs, so we use
// fully qualified paths to avoid import conflicts.

/// Status for the status bar (local to render implementation)
enum RenderStatus {
    Thinking {
        chunks_received: usize,
    },
    RunningTools {
        count: usize,
        tool_names: Vec<String>,
        remaining: usize,
    },
    Idle,
}

// Each sub-file contains its own `impl TUI { ... }` block with related methods.
// This allows splitting without needing include!() inside an impl block.

// Chat message rendering with auto-scroll and search highlighting
include!("render/messages.rs");

// Tool panel and result detail overlay
include!("render/tools.rs");

// Input area rendering
include!("render/input.rs");

// Status bar rendering
include!("render/status.rs");

// Model/provider selector overlays
include!("render/selectors.rs");

// Search box rendering
include!("render/search.rs");

// BrutalistRenderer helper (single construction site)
include!("render/brutalist.rs");
