// rustycode-orchestra/src/convoy.rs
//! Convoy tracking system inspired by Gastown's convoy concept.
//!
//! Groups related tasks into trackable units ("convoys") that provide
//! feature-level progress visibility beyond individual task completion.
//!
//! ## Concept (from Gastown)
//! Gastown uses convoys to bundle multiple "beads" (tasks) into a named unit
//! with shared lifecycle. We adapt this for RustyCode's Autonomous Mode workflow:
//! instead of tracking tasks one-by-one from ROADMAP.md, convoys group related
//! tasks under a feature/theme with aggregated progress.

use crate::error::{OrchestraV2Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Unique convoy identifier (human-readable)
pub type ConvoyId = String;

/// Task identifier within a convoy
pub type TaskId = String;

/// Status of a convoy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConvoyStatus {
    /// All tasks pending, none started
    Pending,
    /// At least one task in progress
    InProgress,
    /// All tasks completed successfully
    Completed,
    /// At least one task failed (with optional retry pending)
    Failed,
    /// User explicitly paused this convoy
    Paused,
}

impl std::fmt::Display for ConvoyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvoyStatus::Pending => write!(f, "pending"),
            ConvoyStatus::InProgress => write!(f, "in_progress"),
            ConvoyStatus::Completed => write!(f, "completed"),
            ConvoyStatus::Failed => write!(f, "failed"),
            ConvoyStatus::Paused => write!(f, "paused"),
        }
    }
}

/// Status of an individual task within a convoy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskStatus {
    /// Not yet started
    Pending,
    /// Currently being worked on
    InProgress,
    /// Completed successfully
    Done,
    /// Failed (will be retried)
    Failed,
    /// Skipped (user or system decision)
    Skipped,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "⏳"),
            TaskStatus::InProgress => write!(f, "🔄"),
            TaskStatus::Done => write!(f, "✅"),
            TaskStatus::Failed => write!(f, "❌"),
            TaskStatus::Skipped => write!(f, "⏭️"),
        }
    }
}

/// A task entry within a convoy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvoyTask {
    /// Task identifier (matches Autonomous Mode task ID from PLAN.md)
    pub id: TaskId,
    /// Human-readable task title
    pub title: String,
    /// Current status
    pub status: TaskStatus,
    /// Which agent identity worked on this (if any)
    pub assigned_agent: Option<String>,
    /// Timestamp when task was started
    pub started_at: Option<DateTime<Utc>>,
    /// Timestamp when task was completed/failed
    pub completed_at: Option<DateTime<Utc>>,
    /// Number of attempts (for retry tracking)
    pub attempts: u32,
}

/// A convoy - a group of related tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Convoy {
    /// Unique convoy identifier
    pub id: ConvoyId,
    /// Human-readable convoy name (e.g., "Authentication Feature")
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Current overall status
    pub status: ConvoyStatus,
    /// Tasks in this convoy
    pub tasks: Vec<ConvoyTask>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Completion timestamp
    pub completed_at: Option<DateTime<Utc>>,
    /// Tags for filtering/categorization
    pub tags: Vec<String>,
}

impl Convoy {
    /// Create a new convoy with the given name and task IDs
    pub fn new(name: impl Into<String>, task_ids: &[TaskId]) -> Self {
        let name = name.into();
        let now = Utc::now();
        let id = Self::generate_id(&name);
        let tasks = task_ids
            .iter()
            .map(|tid| ConvoyTask {
                id: tid.clone(),
                title: tid.clone(),
                status: TaskStatus::Pending,
                assigned_agent: None,
                started_at: None,
                completed_at: None,
                attempts: 0,
            })
            .collect();

        Self {
            id,
            name,
            description: None,
            status: ConvoyStatus::Pending,
            tasks,
            created_at: now,
            updated_at: now,
            completed_at: None,
            tags: Vec::new(),
        }
    }

    /// Generate a slug-style ID from the convoy name
    fn generate_id(name: &str) -> ConvoyId {
        let slug: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let slug = slug
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        // Take first 3 words max for brevity
        let parts: Vec<&str> = slug.split('-').take(3).collect();
        let now = Utc::now();
        let suffix = encode_base62(now.timestamp_millis() as u64);
        format!("{}-{}", parts.join("-"), &suffix[..4.min(suffix.len())])
    }

    /// Calculate completion percentage
    pub fn completion_percent(&self) -> f32 {
        if self.tasks.is_empty() {
            return 0.0;
        }
        let done = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .count();
        (done as f32 / self.tasks.len() as f32) * 100.0
    }

    /// Get count of tasks by status
    pub fn task_counts(&self) -> TaskCounts {
        let mut counts = TaskCounts::default();
        for task in &self.tasks {
            match task.status {
                TaskStatus::Pending => counts.pending += 1,
                TaskStatus::InProgress => counts.in_progress += 1,
                TaskStatus::Done => counts.done += 1,
                TaskStatus::Failed => counts.failed += 1,
                TaskStatus::Skipped => counts.skipped += 1,
            }
        }
        counts.total = self.tasks.len();
        counts
    }

    /// Update a task's status
    pub fn update_task_status(&mut self, task_id: &TaskId, new_status: TaskStatus) -> Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == *task_id)
            .ok_or_else(|| {
                OrchestraV2Error::InvalidState(format!("Task {} not found in convoy", task_id))
            })?;

        let now = Utc::now();
        match new_status {
            TaskStatus::InProgress => {
                task.started_at = Some(now);
                task.attempts += 1;
            }
            TaskStatus::Done | TaskStatus::Failed => {
                task.completed_at = Some(now);
            }
            TaskStatus::Skipped => {
                task.completed_at = Some(now);
            }
            TaskStatus::Pending => {
                // Reset - used for retries
                task.started_at = None;
                task.completed_at = None;
            }
        }

        task.status = new_status;
        self.updated_at = now;
        self.recalculate_status();
        Ok(())
    }

    /// Assign an agent to a specific task
    pub fn assign_agent(&mut self, task_id: &TaskId, agent_id: &str) -> Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == *task_id)
            .ok_or_else(|| {
                OrchestraV2Error::InvalidState(format!("Task {} not found in convoy", task_id))
            })?;
        task.assigned_agent = Some(agent_id.to_string());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Recalculate overall convoy status from task statuses
    fn recalculate_status(&mut self) {
        let counts = self.task_counts();

        if counts.total == 0 {
            self.status = ConvoyStatus::Pending;
            return;
        }

        if counts.failed > 0 {
            self.status = ConvoyStatus::Failed;
        } else if counts.done == counts.total {
            self.status = ConvoyStatus::Completed;
            self.completed_at = Some(Utc::now());
        } else if counts.in_progress > 0 || counts.done > 0 {
            self.status = ConvoyStatus::InProgress;
        } else {
            self.status = ConvoyStatus::Pending;
        }
    }

    /// Format a one-line progress summary
    pub fn format_progress(&self) -> String {
        let counts = self.task_counts();
        let pct = self.completion_percent();
        let status_icon = match self.status {
            ConvoyStatus::Pending => "⏳",
            ConvoyStatus::InProgress => "🔄",
            ConvoyStatus::Completed => "✅",
            ConvoyStatus::Failed => "❌",
            ConvoyStatus::Paused => "⏸️",
        };
        format!(
            "{} {} [{:.0}%] — {}/{} done, {} active, {} failed",
            status_icon,
            self.name,
            pct,
            counts.done,
            counts.total,
            counts.in_progress,
            counts.failed,
        )
    }
}

/// Task counts within a convoy
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskCounts {
    pub total: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Manages convoys on disk
pub struct ConvoyManager {
    /// Path to the convoys directory (e.g., .orchestra/convoys/)
    convoys_dir: PathBuf,
}

impl ConvoyManager {
    /// Create a new convoy manager
    pub fn new(project_root: &Path) -> Result<Self> {
        let convoys_dir = project_root.join(".orchestra").join("convoys");
        if !convoys_dir.exists() {
            fs::create_dir_all(&convoys_dir).map_err(OrchestraV2Error::Io)?;
        }
        Ok(Self { convoys_dir })
    }

    /// Create a new convoy
    pub fn create(&self, name: &str, task_ids: &[TaskId]) -> Result<Convoy> {
        let convoy = Convoy::new(name, task_ids);
        self.save(&convoy)?;
        Ok(convoy)
    }

    /// Load a convoy by ID
    pub fn load(&self, id: &ConvoyId) -> Result<Convoy> {
        let path = self.convoys_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(&path).map_err(OrchestraV2Error::Io)?;
        serde_json::from_str(&content)
            .map_err(|e| OrchestraV2Error::Parse(format!("Invalid convoy JSON: {}", e)))
    }

    /// Save a convoy
    pub fn save(&self, convoy: &Convoy) -> Result<()> {
        let path = self.convoys_dir.join(format!("{}.json", convoy.id));
        let content = serde_json::to_string_pretty(convoy)
            .map_err(|e| OrchestraV2Error::Parse(format!("Failed to serialize convoy: {}", e)))?;
        fs::write(&path, content).map_err(OrchestraV2Error::Io)
    }

    /// List all convoys
    pub fn list(&self) -> Result<Vec<Convoy>> {
        let mut convoys = Vec::new();
        if !self.convoys_dir.exists() {
            return Ok(convoys);
        }

        for entry in fs::read_dir(&self.convoys_dir).map_err(OrchestraV2Error::Io)? {
            let entry = entry.map_err(OrchestraV2Error::Io)?;
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(entry.path()).map_err(OrchestraV2Error::Io)?;
                if let Ok(convoy) = serde_json::from_str::<Convoy>(&content) {
                    convoys.push(convoy);
                }
            }
        }

        // Sort by creation time (newest first)
        convoys.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(convoys)
    }

    /// Update a task status within a convoy
    pub fn update_task(
        &self,
        convoy_id: &ConvoyId,
        task_id: &TaskId,
        status: TaskStatus,
    ) -> Result<()> {
        let mut convoy = self.load(convoy_id)?;
        convoy.update_task_status(task_id, status)?;
        self.save(&convoy)
    }

    /// Add tasks to an existing convoy
    pub fn add_tasks(&self, convoy_id: &ConvoyId, task_ids: &[TaskId]) -> Result<()> {
        let mut convoy = self.load(convoy_id)?;
        for tid in task_ids {
            if !convoy.tasks.iter().any(|t| t.id == *tid) {
                convoy.tasks.push(ConvoyTask {
                    id: tid.clone(),
                    title: tid.clone(),
                    status: TaskStatus::Pending,
                    assigned_agent: None,
                    started_at: None,
                    completed_at: None,
                    attempts: 0,
                });
            }
        }
        convoy.updated_at = Utc::now();
        self.save(&convoy)
    }

    /// Remove (archive) a completed convoy
    pub fn archive(&self, convoy_id: &ConvoyId) -> Result<()> {
        let path = self.convoys_dir.join(format!("{}.json", convoy_id));
        if path.exists() {
            fs::remove_file(path).map_err(OrchestraV2Error::Io)?;
        }
        Ok(())
    }

    /// Find convoy containing a specific task
    pub fn find_by_task(&self, task_id: &TaskId) -> Result<Option<Convoy>> {
        let convoys = self.list()?;
        Ok(convoys
            .into_iter()
            .find(|c| c.tasks.iter().any(|t| t.id == *task_id)))
    }
}

/// Encode a u64 value to Base62 string (simplified for convoy IDs)
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
    fn test_convoy_new() {
        let convoy = Convoy::new(
            "Auth Feature",
            &["task-1".into(), "task-2".into(), "task-3".into()],
        );
        assert_eq!(convoy.tasks.len(), 3);
        assert_eq!(convoy.status, ConvoyStatus::Pending);
        assert!(convoy.id.starts_with("auth-feature-"));
    }

    #[test]
    fn test_convoy_status_progression() {
        let mut convoy = Convoy::new("Test", &["t1".into(), "t2".into()]);
        assert_eq!(convoy.status, ConvoyStatus::Pending);

        convoy
            .update_task_status(&"t1".to_string(), TaskStatus::InProgress)
            .unwrap();
        assert_eq!(convoy.status, ConvoyStatus::InProgress);

        convoy
            .update_task_status(&"t1".to_string(), TaskStatus::Done)
            .unwrap();
        assert_eq!(convoy.status, ConvoyStatus::InProgress);

        convoy
            .update_task_status(&"t2".to_string(), TaskStatus::Done)
            .unwrap();
        assert_eq!(convoy.status, ConvoyStatus::Completed);
        assert!(convoy.completed_at.is_some());
    }

    #[test]
    fn test_convoy_completion_percent() {
        let mut convoy = Convoy::new(
            "Test",
            &["t1".into(), "t2".into(), "t3".into(), "t4".into()],
        );
        assert_eq!(convoy.completion_percent(), 0.0);

        convoy
            .update_task_status(&"t1".to_string(), TaskStatus::Done)
            .unwrap();
        assert_eq!(convoy.completion_percent(), 25.0);

        convoy
            .update_task_status(&"t2".to_string(), TaskStatus::Done)
            .unwrap();
        assert_eq!(convoy.completion_percent(), 50.0);
    }

    #[test]
    fn test_convoy_failure() {
        let mut convoy = Convoy::new("Test", &["t1".into(), "t2".into()]);
        convoy
            .update_task_status(&"t1".to_string(), TaskStatus::Failed)
            .unwrap();
        assert_eq!(convoy.status, ConvoyStatus::Failed);
    }

    #[test]
    fn test_convoy_agent_assignment() {
        let mut convoy = Convoy::new("Test", &["t1".into()]);
        convoy
            .assign_agent(&"t1".to_string(), "worker-7a3f")
            .unwrap();
        assert_eq!(
            convoy.tasks[0].assigned_agent,
            Some("worker-7a3f".to_string())
        );
    }

    #[test]
    fn test_task_counts() {
        let mut convoy = Convoy::new("Test", &["t1".into(), "t2".into(), "t3".into()]);
        convoy
            .update_task_status(&"t1".to_string(), TaskStatus::Done)
            .unwrap();
        convoy
            .update_task_status(&"t2".to_string(), TaskStatus::InProgress)
            .unwrap();

        let counts = convoy.task_counts();
        assert_eq!(counts.total, 3);
        assert_eq!(counts.done, 1);
        assert_eq!(counts.in_progress, 1);
        assert_eq!(counts.pending, 1);
    }

    #[test]
    fn test_format_progress() {
        let mut convoy = Convoy::new("Auth Feature", &["t1".into(), "t2".into()]);
        convoy
            .update_task_status(&"t1".to_string(), TaskStatus::Done)
            .unwrap();
        let progress = convoy.format_progress();
        assert!(progress.contains("Auth Feature"));
        assert!(progress.contains("50%"));
    }

    #[test]
    fn test_convoy_manager_crud() {
        let temp = tempfile::tempdir().unwrap();
        let manager = ConvoyManager::new(temp.path()).unwrap();

        // Create
        let convoy = manager
            .create("Test Convoy", &["t1".into(), "t2".into()])
            .unwrap();
        assert!(!convoy.id.is_empty());

        // Load
        let loaded = manager.load(&convoy.id).unwrap();
        assert_eq!(loaded.name, "Test Convoy");

        // List
        let all = manager.list().unwrap();
        assert_eq!(all.len(), 1);

        // Update task
        manager
            .update_task(&convoy.id, &"t1".to_string(), TaskStatus::Done)
            .unwrap();
        let updated = manager.load(&convoy.id).unwrap();
        assert_eq!(updated.tasks[0].status, TaskStatus::Done);

        // Archive
        manager.archive(&convoy.id).unwrap();
        assert!(manager.list().unwrap().is_empty());
    }

    #[test]
    fn test_find_by_task() {
        let temp = tempfile::tempdir().unwrap();
        let manager = ConvoyManager::new(temp.path()).unwrap();

        let _convoy = manager
            .create("Feature A", &["t1".into(), "t2".into()])
            .unwrap();
        let _ = manager.create("Feature B", &["t3".into()]).unwrap();

        let found = manager.find_by_task(&"t1".to_string()).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Feature A");

        let not_found = manager.find_by_task(&"t99".to_string()).unwrap();
        assert!(not_found.is_none());
    }
}
