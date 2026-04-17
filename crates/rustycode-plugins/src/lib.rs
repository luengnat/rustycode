//! Plugin system foundation for RustyCode
//!
//! This module provides the core traits and registry for dynamically loading and managing plugins.
//! It supports three types of plugins:
//! - `ToolPlugin`: Tools that can be used by the agent
//! - `AgentPlugin`: Agents that can execute tasks
//! - `LLMProviderPlugin`: LLM providers for model integration
//!
//! # Quick Start
//!
//! ```ignore
//! use rustycode_plugins::{PluginRegistry, ToolPlugin, PluginMetadata};
//!
//! let mut registry = PluginRegistry::new();
//! // Register plugins...
//! ```

pub mod config;
pub mod dependency_resolver;
pub mod error;
pub mod lifecycle;
pub mod manifest;
pub mod metadata;
pub mod registry;
pub mod status;
pub mod traits;

pub use config::{ConfigBuilder, ConfigValue, PluginConfig, SensitiveValue};
pub use dependency_resolver::DependencyResolver;
pub use error::PluginError;
pub use lifecycle::PluginLifecycleManager;
pub use manifest::{DependencySpec, PluginManifest};
pub use metadata::PluginMetadata;
pub use registry::PluginRegistry;
pub use status::PluginStatus;
pub use traits::{AgentPlugin, LLMProviderPlugin, ToolPlugin};

#[cfg(test)]
mod tests;
