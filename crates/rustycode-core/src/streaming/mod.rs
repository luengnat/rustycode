//! Unified streaming infrastructure for SSE responses
//!
//! This module provides shared utilities for processing SSE (Server-Sent Events)
//! responses from LLM providers. It eliminates duplication between the headless
//! runtime and the TUI by providing a canonical event dispatch mechanism.
//!
//! Both consumers implement the `StreamingCallbacks` trait to handle semantic
//! events (text, thinking, tool completion) according to their own needs.

pub mod processor;
pub mod tool_state;

pub use processor::{SseEventProcessor, StreamingCallbacks};
pub use tool_state::ToolAccumulator;
