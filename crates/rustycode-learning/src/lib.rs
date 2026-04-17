//! # RustyCode Continuous Learning System (Instincts v2)
//!
//! This crate provides pattern extraction, storage, and automatic application
//! for learned behaviors in the RustyCode system.
//!
//! ## Features
//!
//! - **Pattern Extraction**: Extract reusable patterns from sessions
//! - **Pattern Storage**: Persistent storage for learned patterns
//! - **Auto-Application**: Automatically apply learned patterns
//! - **Learning Loop**: Continuous improvement through feedback
//! - **Built-in Patterns**: Pre-configured patterns for common workflows
//!
//! ## Example
//!
//! ```rust,no_run
//! use rustycode_learning::{PatternStorage, InstinctExtractor, LearningLoop};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let path = std::path::Path::new("/tmp/test");
//! let storage = PatternStorage::new(&path)?;
//! let extractor = InstinctExtractor::new();
//! let _learning_loop = LearningLoop::new(extractor, storage);
//! # Ok(())
//! # }
//! ```

pub mod actions;
pub mod builtin;
pub mod error;
pub mod extractor;
pub mod learning_loop;
pub mod patterns;
pub mod storage;
pub mod triggers;

// Re-export main types
pub use actions::{ActionResult, Change, ChangeType};
pub use builtin::BuiltinPatterns;
pub use error::{ExtractionError, LearningError, StorageError};
pub use extractor::InstinctExtractor;
pub use learning_loop::{Feedback, LearningLoop, LearningReport, UpdateReport};
pub use patterns::{
    Instinct, Pattern, PatternCategory, SuggestedAction, TriggerCondition, TriggerType,
};
pub use storage::PatternStorage;
pub use triggers::{Context, TriggerMatcher};
