//! Team Execution Engine Traits
//!
//! Provides an abstraction for running agent teams in different modes:
//! - InProcess: Lightweight tasks running on the same process/event loop.
//! - SplitPane: Isolated execution in dedicated tmux panes.

use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy)]
pub enum AgentInterface {
    /// Agent runs as an ACP-compatible service
    ACP { port: u16 },
    /// Agent runs as a standard subprocess
    Shell,
    /// Agent runs in-memory
    InProcess,
}

#[async_trait]
pub trait TeamExecutionEngine: Send + Sync {
    /// Start a new agent for a task
    async fn launch_agent(&self, task_name: &str, command: &str) -> Result<AgentHandle>;
    /// Stop a specific agent
    async fn stop_agent(&self, handle: &AgentHandle) -> Result<()>;
    /// Shutdown the team engine and all managed agents
    async fn shutdown(&self) -> Result<()>;
}

/// Handle for a running agent
pub struct AgentHandle {
    pub id: String,
    pub pane_index: Option<usize>,
    pub interface: AgentInterface,
}

impl AgentHandle {
    /// Gracefully stop the agent
    pub async fn stop(&self, engine: &dyn TeamExecutionEngine) -> Result<()> {
        engine.stop_agent(self).await
    }
}
