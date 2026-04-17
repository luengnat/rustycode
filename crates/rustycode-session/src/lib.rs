//! # RustyCode Session Management
//!
//! This crate provides advanced session and message management with compaction,
//! summarization, and efficient serialization for the RustyCode system.
//!
//! ## Features
//!
//! - **Rich Message Types**: Support for text, tool calls, images, reasoning, code, and diffs
//! - **Session Management**: Track conversations with metadata and context
//! - **Smart Compaction**: Multiple strategies for reducing token usage
//! - **Efficient Serialization**: Binary format with zstd compression
//! - **Streaming Support**: Handle streaming LLM responses
//!
//! ## Example
//!
//! ```rust
//! use rustycode_session::{Session, MessageV2, MessageRole};
//!
//! let mut session = Session::new("My Session".to_string());
//! session.add_message(MessageV2::user("Hello, world!".to_string()));
//! session.add_message(MessageV2::assistant("Hi! How can I help?".to_string()));
//!
//! println!("Session has {} messages", session.message_count());
//! println!("Estimated tokens: {}", session.token_count());
//! ```

pub mod compaction;
pub mod message_v2;
pub mod rewind;
pub mod serialization;
pub mod session;
pub mod summary;

// Re-export main types
pub use compaction::{
    CompactionEngine, CompactionError, CompactionReport, CompactionSnapshot, CompactionStrategy,
};
pub use message_v2::{MessageMetadata, MessagePart, MessageRole, MessageV2};
pub use rewind::{
    create_snapshot, create_snapshot_with_checkpoint, InteractionId, InteractionSnapshot,
    RewindMode, RewindResult, RewindState, RewindStore, ToolCallRecord,
};
pub use serialization::{SerializationFormat, SessionSerializer};
pub use session::{Session, SessionId, SessionMetadata, SessionStatus};
pub use summary::{Summary, SummaryGenerator};
