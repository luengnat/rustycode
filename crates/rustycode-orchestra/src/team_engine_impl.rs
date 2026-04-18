//! Team Execution Engines
//!
//! Implementations for In-Process and Split-Pane agent execution.

use crate::team_engine::{AgentHandle, AgentInterface, TeamExecutionEngine};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rustycode_connector::{TerminalConnector};
use std::sync::{Arc, Mutex};

/// In-Process engine: Runs agents as async tasks within the current process/TUI event loop.
pub struct InProcessEngine {
}

#[async_trait]
impl TeamExecutionEngine for InProcessEngine {
    async fn launch_agent(&self, task_name: &str, _command: &str) -> Result<AgentHandle> {
        Ok(AgentHandle {
            id: format!("in-proc-{}", task_name),
            pane_index: None,
            interface: AgentInterface::InProcess,
        })
    }

    async fn stop_agent(&self, _handle: &AgentHandle) -> Result<()> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

/// Split-Pane engine: Provisions dedicated tmux panes for each agent.
pub struct SplitPaneEngine {
    connector: Arc<Mutex<Box<dyn TerminalConnector + Send + Sync>>>,
}

impl SplitPaneEngine {
    pub fn new(connector: Arc<Mutex<Box<dyn TerminalConnector + Send + Sync>>>) -> Self {
        Self { connector }
    }
}

#[async_trait]
impl TeamExecutionEngine for SplitPaneEngine {
    async fn launch_agent(&self, task_name: &str, command: &str) -> Result<AgentHandle> {
        let mut conn = self.connector.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        
        let sessions = conn.list_sessions()?;
        let session = sessions.first()
            .map(|s| s.id.clone())
            .ok_or_else(|| anyhow!("No active session found for split"))?;

        let pane_index = conn.split_pane(&session, 0, rustycode_connector::SplitDirection::Vertical)?;
        
        conn.send_keys(&session, pane_index, command)?;
        conn.set_pane_title(&session, pane_index, task_name)?;

        Ok(AgentHandle {
            id: format!("pane-{}", pane_index),
            pane_index: Some(pane_index),
            interface: AgentInterface::Shell, 
        })
    }

    async fn stop_agent(&self, handle: &AgentHandle) -> Result<()> {
        if let Some(pane_index) = handle.pane_index {
            let mut conn = self.connector.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
            let sessions = conn.list_sessions()?;
            if let Some(session) = sessions.first() {
                conn.kill_pane(&session.id, pane_index)?;
            }
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}
