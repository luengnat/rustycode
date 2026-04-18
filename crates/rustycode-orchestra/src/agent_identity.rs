// rustycode-orchestra/src/agent_identity.rs
//! Agent identity system inspired by Gastown's "Polecat" concept.
//!
//! Gastown assigns named identities ("polecats") to each agent so that
//! tasks carry attribution — you can see WHO worked on WHAT. We adapt
//! this for RustyCode: each Autonomous Mode session gets a stable agent identity
//! that persists across task executions within that session.

use crate::error::{OrchestraV2Error, Result};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Unique agent identifier (e.g., "worker-a7f3")
pub type AgentId = String;

/// What role this agent is performing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentRole {
    /// Autonomous task executor (default for Autonomous Mode)
    Worker,
    /// Plan-only agent (analysis, no code changes)
    Planner,
    /// Verification-only agent
    Reviewer,
    /// Research agent (read-only exploration)
    Researcher,
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentRole::Worker => write!(f, "worker"),
            AgentRole::Planner => write!(f, "planner"),
            AgentRole::Reviewer => write!(f, "reviewer"),
            AgentRole::Researcher => write!(f, "researcher"),
        }
    }
}

/// Persistent identity for a Autonomous Mode agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    /// Unique agent identifier
    pub id: AgentId,
    /// Human-readable label (e.g., "worker-a7f3")
    pub label: String,
    /// Current role
    pub role: AgentRole,
    /// When this identity was created
    pub created_at: DateTime<Utc>,
    /// Session this identity belongs to
    pub session_id: String,
    /// Tasks completed by this agent
    pub tasks_completed: u32,
    /// Tasks failed by this agent
    pub tasks_failed: u32,
    /// Last activity timestamp
    pub last_active: DateTime<Utc>,
}

impl AgentIdentity {
    /// Create a new agent identity for a session
    pub fn new(session_id: &str, role: AgentRole) -> Self {
        let id = generate_agent_id(role);
        let label = id.clone();
        let now = Utc::now();
        Self {
            id,
            label,
            role,
            created_at: now,
            session_id: session_id.to_string(),
            tasks_completed: 0,
            tasks_failed: 0,
            last_active: now,
        }
    }

    /// Record a completed task
    pub fn record_success(&mut self) {
        self.tasks_completed += 1;
        self.last_active = Utc::now();
    }

    /// Record a failed task
    pub fn record_failure(&mut self) {
        self.tasks_failed += 1;
        self.last_active = Utc::now();
    }

    /// Total tasks attempted
    pub fn total_tasks(&self) -> u32 {
        self.tasks_completed + self.tasks_failed
    }

    /// Success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f32 {
        if self.total_tasks() == 0 {
            return 1.0;
        }
        self.tasks_completed as f32 / self.total_tasks() as f32
    }

    /// Format a one-line summary
    pub fn format_summary(&self) -> String {
        let role_icon = match self.role {
            AgentRole::Worker => "🔧",
            AgentRole::Planner => "📋",
            AgentRole::Reviewer => "🔍",
            AgentRole::Researcher => "🔬",
        };
        format!(
            "{} {} — {} done, {} failed ({:.0}% success)",
            role_icon,
            self.label,
            self.tasks_completed,
            self.tasks_failed,
            self.success_rate() * 100.0,
        )
    }
}

/// Manage agent identities on disk
pub struct AgentIdentityManager {
    identities_dir: PathBuf,
}

impl AgentIdentityManager {
    /// Create a new manager rooted at the project's .orchestra directory
    pub fn new(project_root: &Path) -> Result<Self> {
        let identities_dir = project_root.join(".orchestra").join("agents");
        if !identities_dir.exists() {
            fs::create_dir_all(&identities_dir).map_err(OrchestraV2Error::Io)?;
        }
        Ok(Self { identities_dir })
    }

    /// Register a new agent identity
    pub fn register(&self, identity: &AgentIdentity) -> Result<()> {
        let path = self.identities_dir.join(format!("{}.json", identity.id));
        let content = serde_json::to_string_pretty(identity).map_err(|e| {
            OrchestraV2Error::Parse(format!("Failed to serialize agent identity: {}", e))
        })?;
        fs::write(&path, content).map_err(OrchestraV2Error::Io)
    }

    /// Load an agent identity by ID
    pub fn load(&self, id: &AgentId) -> Result<AgentIdentity> {
        let path = self.identities_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(&path).map_err(OrchestraV2Error::Io)?;
        serde_json::from_str(&content)
            .map_err(|e| OrchestraV2Error::Parse(format!("Invalid agent identity JSON: {}", e)))
    }

    /// Update an existing agent identity
    pub fn update(&self, identity: &AgentIdentity) -> Result<()> {
        self.register(identity)
    }

    /// List all known agent identities
    pub fn list(&self) -> Result<Vec<AgentIdentity>> {
        let mut identities = Vec::new();
        if !self.identities_dir.exists() {
            return Ok(identities);
        }

        for entry in fs::read_dir(&self.identities_dir).map_err(OrchestraV2Error::Io)? {
            let entry = entry.map_err(OrchestraV2Error::Io)?;
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(entry.path()).map_err(OrchestraV2Error::Io)?;
                if let Ok(identity) = serde_json::from_str::<AgentIdentity>(&content) {
                    identities.push(identity);
                }
            }
        }

        identities.sort_by_key(|a| std::cmp::Reverse(a.last_active));
        Ok(identities)
    }

    /// Find the identity for a given session
    pub fn find_by_session(&self, session_id: &str) -> Result<Option<AgentIdentity>> {
        let identities = self.list()?;
        Ok(identities.into_iter().find(|i| i.session_id == session_id))
    }

    /// Remove an agent identity
    pub fn remove(&self, id: &AgentId) -> Result<()> {
        let path = self.identities_dir.join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(path).map_err(OrchestraV2Error::Io)?;
        }
        Ok(())
    }
}

fn generate_agent_id(role: AgentRole) -> AgentId {
    let prefix = match role {
        AgentRole::Worker => "wkr",
        AgentRole::Planner => "pln",
        AgentRole::Reviewer => "rev",
        AgentRole::Researcher => "res",
    };
    let now = Utc::now();
    let ts = encode_base62(now.timestamp_millis() as u64);
    let rand_part = encode_base62(rand::thread_rng().gen_range(0u64..u64::MAX));
    format!(
        "{}-{}-{}",
        prefix,
        &ts[..6.min(ts.len())],
        &rand_part[..4.min(rand_part.len())]
    )
}

/// Encode a u64 value to Base62 string
fn encode_base62(mut value: u64) -> String {
    const BASE62: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut chars = vec![];
    while value > 0 {
        chars.push(BASE62[(value % 62) as usize]);
        value /= 62;
    }
    chars.reverse();
    String::from_utf8(chars).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_identity_new() {
        let agent = AgentIdentity::new("sess_abc123", AgentRole::Worker);
        assert!(agent.id.starts_with("wkr-"));
        assert_eq!(agent.role, AgentRole::Worker);
        assert_eq!(agent.tasks_completed, 0);
    }

    #[test]
    fn test_agent_role_display() {
        assert_eq!(AgentRole::Worker.to_string(), "worker");
        assert_eq!(AgentRole::Planner.to_string(), "planner");
        assert_eq!(AgentRole::Reviewer.to_string(), "reviewer");
        assert_eq!(AgentRole::Researcher.to_string(), "researcher");
    }

    #[test]
    fn test_agent_success_tracking() {
        let mut agent = AgentIdentity::new("sess_test", AgentRole::Worker);
        assert_eq!(agent.success_rate(), 1.0);

        agent.record_success();
        agent.record_success();
        agent.record_failure();

        assert_eq!(agent.tasks_completed, 2);
        assert_eq!(agent.tasks_failed, 1);
        assert_eq!(agent.total_tasks(), 3);
        assert!((agent.success_rate() - 0.6667).abs() < 0.01);
    }

    #[test]
    fn test_agent_format_summary() {
        let mut agent = AgentIdentity::new("sess_test", AgentRole::Worker);
        agent.record_success();
        let summary = agent.format_summary();
        assert!(summary.contains("wkr-"));
        assert!(summary.contains("1 done"));
    }

    #[test]
    fn test_agent_id_uniqueness() {
        let id1 = generate_agent_id(AgentRole::Worker);
        let id2 = generate_agent_id(AgentRole::Worker);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_identity_manager_crud() {
        let temp = tempfile::tempdir().unwrap();
        let manager = AgentIdentityManager::new(temp.path()).unwrap();

        let identity = AgentIdentity::new("sess_test123", AgentRole::Planner);
        manager.register(&identity).unwrap();

        let loaded = manager.load(&identity.id).unwrap();
        assert_eq!(loaded.session_id, "sess_test123");
        assert_eq!(loaded.role, AgentRole::Planner);

        let all = manager.list().unwrap();
        assert_eq!(all.len(), 1);

        let found = manager.find_by_session("sess_test123").unwrap();
        assert!(found.is_some());

        let not_found = manager.find_by_session("nonexistent").unwrap();
        assert!(not_found.is_none());

        manager.remove(&identity.id).unwrap();
        assert!(manager.list().unwrap().is_empty());
    }

    #[test]
    fn test_identity_update() {
        let temp = tempfile::tempdir().unwrap();
        let manager = AgentIdentityManager::new(temp.path()).unwrap();

        let mut identity = AgentIdentity::new("sess_update", AgentRole::Worker);
        manager.register(&identity).unwrap();

        identity.record_success();
        identity.record_success();
        manager.update(&identity).unwrap();

        let loaded = manager.load(&identity.id).unwrap();
        assert_eq!(loaded.tasks_completed, 2);
    }
}
