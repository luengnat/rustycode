//! # RustyCode CLI Library
//!
//! This library provides the core CLI functionality for RustyCode, including
//! interactive prompt system for user interaction.

pub mod commands;
pub mod prompt;

pub use prompt::{Confirm, Input, MultiSelect, Prompt, PromptConfig, Select};
pub use rustycode_protocol::WorkingMode;
