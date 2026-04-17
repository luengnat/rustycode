//! Benchmark runner for agent evaluation.
//!
//! Provides a Harbor-compatible pipeline for running benchmark tasks:
//! environment (container) → agent → verifier → result.
//!
//! # Architecture
//!
//! - [`BenchEnvironment`] — container lifecycle (start/stop/exec)
//! - [`BenchAgent`] — task execution (oracle, code agent, etc.)
//! - [`Verifier`] — test result parsing
//! - [`Trial`] — orchestrates a single task run
//! - [`Job`] — manages N concurrent trials
//! - [`TaskConfig`] — Harbor task.toml parser
//! - [`DatasetRegistry`] — discover and load task datasets
//! - [`BenchMcpBridge`] — MCP-compatible bridge for container operations

pub mod agent;
pub mod dataset;
pub mod environment;
pub mod job;
pub mod mcp_bridge;
pub mod task;
pub mod trial;
pub mod verifier;

pub use agent::{BenchAgent, CodeAgent, CodeAgentConfig, NopAgent, OracleAgent};
pub use dataset::{DatasetInfo, DatasetRegistry};
pub use environment::{BenchEnvironment, ExecResult};
pub use job::{BenchmarkResults, Job, JobConfig};
pub use mcp_bridge::{BenchMcpBridge, ToolResult as BenchToolResult};
pub use task::{ResolvedTask, TaskConfig};
pub use trial::{Trial, TrialResult};
pub use verifier::{ScriptVerifier, Verifier};
