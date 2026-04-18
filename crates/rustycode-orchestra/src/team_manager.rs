//! Team Management & Lifecycle
//!
//! Provides a high-level manager to coordinate team agent lifecycles,
//! including starting, stopping, sending commands, and monitoring status.

use crate::team_engine::{AgentHandle, TeamExecutionEngine};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;

pub struct TeamManager {
    engine: Arc<dyn TeamExecutionEngine>,
    agents: HashMap<String, AgentHandle>,
}

impl TeamManager {
    pub fn new(engine: Arc<dyn TeamExecutionEngine>) -> Self {
        Self {
            engine,
            agents: HashMap::new(),
        }
    }

    /// Launch a new team member
    pub async fn start_agent(&mut self, id: &str, command: &str) -> Result<()> {
        if self.agents.contains_key(id) {
            return Err(anyhow!("Agent with ID {} already exists", id));
        }
        let handle = self.engine.launch_agent(id, command).await?;
        self.agents.insert(id.to_string(), handle);
        Ok(())
    }

    /// Kill a specific team member
    pub async fn stop_agent(&mut self, id: &str) -> Result<()> {
        let handle = self.agents.remove(id)
            .ok_or_else(|| anyhow!("Agent {} not found", id))?;
        handle.stop(self.engine.as_ref()).await
    }

    /// Shut down the entire team
    pub async fn shutdown_team(&mut self) -> Result<()> {
        for handle in self.agents.values() {
            handle.stop(self.engine.as_ref()).await?;
        }
        self.agents.clear();
        self.engine.shutdown().await
    }
}
