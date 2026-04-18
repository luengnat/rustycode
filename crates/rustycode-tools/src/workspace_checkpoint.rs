//! Git-based workspace checkpoints for safe experimentation
//!
//! This module provides git-based snapshots that allow:
//! - Auto-saving workspace state before destructive operations
//! - Comparing and restoring to previous checkpoints
//! - Linear checkpoint history with LRU eviction

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use uuid::Uuid;

/// Unique identifier for a checkpoint
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CheckpointId(pub String);

impl CheckpointId {
    pub fn new() -> Self {
        let id = Uuid::new_v4().simple().to_string();
        Self(id[..8].to_string())
    }
}

impl Default for CheckpointId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CheckpointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A workspace checkpoint containing git commit info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCheckpoint {
    pub id: CheckpointId,
    pub commit_hash: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub files_changed: usize,
    pub reason: String,
}

/// Restore mode for checkpoint restoration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestoreMode {
    /// Restore only files, keep current conversation
    FilesOnly,
    /// Full restore (files + conversation)
    Full,
}

/// Configuration for checkpointing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Enable automatic checkpoints
    pub enabled: bool,
    /// Maximum number of checkpoints to keep
    pub max_checkpoints: usize,
    /// Tools that trigger auto-checkpoint
    #[serde(default = "default_auto_trigger_tools")]
    pub auto_trigger_tools: Vec<String>,
    /// Checkpoints directory (separate from workspace)
    pub checkpoints_dir: Option<PathBuf>,
}

fn default_auto_trigger_tools() -> Vec<String> {
    vec!["edit".to_string(), "write".to_string(), "bash".to_string()]
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_checkpoints: 20,
            auto_trigger_tools: default_auto_trigger_tools(),
            checkpoints_dir: None,
        }
    }
}

/// Persistence backend for checkpoints.
///
/// Implementors store checkpoint records to a durable medium (e.g., SQLite).
/// The trait is optional — `CheckpointManager` works without a store (in-memory only).
pub trait CheckpointStore: Send + Sync {
    /// Persist a checkpoint record.
    fn save_checkpoint(&self, session_id: &str, checkpoint: &WorkspaceCheckpoint) -> Result<()>;
    /// List checkpoints for a session, newest first.
    fn list_checkpoints(&self, session_id: &str) -> Result<Vec<WorkspaceCheckpoint>>;
    /// Delete a checkpoint by ID.
    fn delete_checkpoint(&self, id: &str) -> Result<()>;
}

/// Checkpoint store implementation backed by rustycode-storage
///
/// Persists checkpoints to a SQLite database using the Storage API.
pub struct StorageBasedCheckpointStore {
    storage: std::sync::Arc<rustycode_storage::Storage>,
}

impl StorageBasedCheckpointStore {
    /// Create a new storage-based checkpoint store
    pub fn new(storage: std::sync::Arc<rustycode_storage::Storage>) -> Self {
        Self { storage }
    }
}

impl CheckpointStore for StorageBasedCheckpointStore {
    fn save_checkpoint(&self, session_id: &str, checkpoint: &WorkspaceCheckpoint) -> Result<()> {
        let record = rustycode_storage::CheckpointRecord {
            id: checkpoint.id.0.clone(),
            session_id: session_id.to_string(),
            label: checkpoint.reason.clone(),
            commit_sha: if checkpoint.commit_hash.is_empty() {
                None
            } else {
                Some(checkpoint.commit_hash.clone())
            },
            files_json: serde_json::to_string(&checkpoint.files_changed)
                .context("failed to serialize files_changed")?,
            created_at: checkpoint.created_at.to_rfc3339(),
        };

        match self.storage.save_checkpoint(&record) {
            Ok(_) => Ok(()),
            Err(e) => Err(e).context("failed to save checkpoint to storage"),
        }
    }

    fn list_checkpoints(&self, session_id: &str) -> Result<Vec<WorkspaceCheckpoint>> {
        let records = self
            .storage
            .list_checkpoints(session_id)
            .context("failed to list checkpoints from storage")?;

        records
            .into_iter()
            .map(|rec| {
                let files_changed: usize = if rec.files_json.is_empty() {
                    0
                } else {
                    serde_json::from_str(&rec.files_json).unwrap_or(0)
                };

                Ok(WorkspaceCheckpoint {
                    id: CheckpointId(rec.id),
                    commit_hash: rec.commit_sha.unwrap_or_else(String::new),
                    message: rec.label.clone(),
                    created_at: chrono::DateTime::parse_from_rfc3339(&rec.created_at)
                        .context("failed to parse checkpoint created_at timestamp")?
                        .with_timezone(&Utc),
                    files_changed,
                    reason: rec.label,
                })
            })
            .collect()
    }

    fn delete_checkpoint(&self, id: &str) -> Result<()> {
        // Storage layer doesn't expose a delete method, but we could add it
        // For now, this is a no-op that succeeds silently
        tracing::debug!(
            "checkpoint deletion not yet implemented in storage layer: {}",
            id
        );
        Ok(())
    }
}

/// Manager for workspace checkpoints
pub struct CheckpointManager {
    config: CheckpointConfig,
    /// Shadow directory for checkpoints
    shadow_dir: PathBuf,
    /// Cache of recent checkpoints (in-memory + persisted to git state)
    checkpoints: Arc<RwLock<HashMap<String, WorkspaceCheckpoint>>>,
    /// Current workspace path (used for path resolution in checkpoints)
    _workspace_path: PathBuf,
    /// Optional persistence backend
    store: Option<Arc<dyn CheckpointStore>>,
    /// Session ID for persistence scoping
    session_id: String,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(workspace_path: PathBuf, config: CheckpointConfig) -> Result<Self> {
        Self::with_store(workspace_path, config, None, "default".to_string())
    }

    /// Create a checkpoint manager with optional persistence backend.
    pub fn with_store(
        workspace_path: PathBuf,
        config: CheckpointConfig,
        store: Option<Arc<dyn CheckpointStore>>,
        session_id: String,
    ) -> Result<Self> {
        let shadow_dir = config.checkpoints_dir.clone().unwrap_or_else(|| {
            dirs::data_local_dir()
                .map(|d| d.join("rustycode").join("checkpoints"))
                .unwrap_or_else(|| PathBuf::from(".rustycode/checkpoints"))
        });

        // Ensure shadow directory exists and is a git repo
        Self::init_shadow_repo(&shadow_dir, &workspace_path)?;

        // Load existing checkpoints from store if available
        let checkpoints: HashMap<String, WorkspaceCheckpoint> = if let Some(ref s) = store {
            s.list_checkpoints(&session_id)
                .unwrap_or_default()
                .into_iter()
                .map(|c| (c.id.0.clone(), c))
                .collect()
        } else {
            HashMap::new()
        };

        Ok(Self {
            config,
            shadow_dir,
            checkpoints: Arc::new(RwLock::new(checkpoints)),
            _workspace_path: workspace_path,
            store,
            session_id,
        })
    }

    /// Initialize shadow repository for checkpoints
    fn init_shadow_repo(shadow_dir: &Path, workspace_path: &Path) -> Result<()> {
        if !shadow_dir.exists() {
            std::fs::create_dir_all(shadow_dir).with_context(|| {
                format!("failed to create checkpoints dir: {}", shadow_dir.display())
            })?;
        }

        // Check if already a git repo
        let git_dir = shadow_dir.join(".git");
        if !git_dir.exists() {
            // Initialize new git repo
            let output = Command::new("git")
                .args(["init"])
                .current_dir(shadow_dir)
                .output()
                .context("failed to init checkpoint repo")?;

            if !output.status.success() {
                anyhow::bail!(
                    "git init failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            // Add .gitignore to avoid staging everything
            let gitignore = shadow_dir.join(".gitignore");
            std::fs::write(&gitignore, ".*\n!*")?;
        }

        // Create working directory checkout
        let work_dir = shadow_dir.join("workspace");
        if !work_dir.exists() {
            // Clone from workspace if available
            if workspace_path.join(".git").exists() {
                let output = Command::new("git")
                    .args([
                        "clone",
                        &workspace_path.to_string_lossy(),
                        &work_dir.to_string_lossy(),
                    ])
                    .output()
                    .context("failed to clone workspace")?;

                if !output.status.success() {
                    // Fallback: create empty dir
                    std::fs::create_dir_all(&work_dir)?;
                }
            } else {
                std::fs::create_dir_all(&work_dir)?;
            }
        }

        Ok(())
    }

    /// Create a checkpoint before a potentially destructive operation
    pub fn create_checkpoint(&self, reason: &str) -> Result<WorkspaceCheckpoint> {
        let work_dir = self.shadow_dir.join("workspace");

        // Stage all changes
        let _output = Command::new("git")
            .args(["-C", &work_dir.to_string_lossy(), "add", "-A"])
            .output()
            .context("failed to stage changes")?;

        // Check if there are changes to commit
        let diff_output = Command::new("git")
            .args([
                "-C",
                &work_dir.to_string_lossy(),
                "diff",
                "--cached",
                "--stat",
            ])
            .output()
            .context("failed to check staged changes")?;

        let has_changes = !String::from_utf8_lossy(&diff_output.stdout)
            .trim()
            .is_empty();

        let checkpoint_id = CheckpointId::new();
        let timestamp = Local::now().format("%Y-%m-%d %H:%M").to_string();
        let commit_message = if has_changes {
            format!("checkpoint: {} - {}", timestamp, reason)
        } else {
            format!("checkpoint: {} - {} (no changes)", timestamp, reason)
        };

        let files_changed = if has_changes {
            // Commit changes
            let output = Command::new("git")
                .args([
                    "-C",
                    &work_dir.to_string_lossy(),
                    "commit",
                    "-m",
                    &commit_message,
                ])
                .output()
                .context("failed to commit checkpoint")?;

            if !output.status.success() {
                tracing::warn!(
                    "checkpoint commit failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            // Get the commit hash
            let hash_output = Command::new("git")
                .args(["-C", &work_dir.to_string_lossy(), "rev-parse", "HEAD"])
                .output()
                .context("failed to get commit hash")?;

            let _commit_hash = String::from_utf8_lossy(&hash_output.stdout)
                .trim()
                .to_string();

            // Count files changed
            let files_changed = if let Ok(stat_output) = Command::new("git")
                .args([
                    "-C",
                    &work_dir.to_string_lossy(),
                    "diff",
                    "--cached",
                    "--numstat",
                ])
                .output()
            {
                String::from_utf8_lossy(&stat_output.stdout).lines().count()
            } else {
                0
            };

            files_changed
        } else {
            0
        };

        let checkpoint = WorkspaceCheckpoint {
            id: checkpoint_id.clone(),
            commit_hash: if has_changes {
                String::from_utf8_lossy(
                    &Command::new("git")
                        .args(["-C", &work_dir.to_string_lossy(), "rev-parse", "HEAD"])
                        .output()
                        .map(|o| o.stdout)
                        .unwrap_or_else(|_| Vec::new()),
                )
                .trim()
                .to_string()
            } else {
                String::new()
            },
            message: commit_message,
            created_at: Utc::now(),
            files_changed,
            reason: reason.to_string(),
        };

        // Cache the checkpoint
        {
            let mut cache = self.checkpoints.write();
            cache.insert(checkpoint_id.0.clone(), checkpoint.clone());

            // Evict old checkpoints if over limit
            while cache.len() > self.config.max_checkpoints {
                // Find the oldest checkpoint by created_at timestamp
                let oldest_key = cache
                    .iter()
                    .min_by_key(|(_, cp)| cp.created_at)
                    .map(|(k, _)| k.clone());
                if let Some(oldest) = oldest_key {
                    if let Some(cp) = cache.remove(&oldest) {
                        // Delete from store
                        if let Some(ref store) = self.store {
                            let _ = store.delete_checkpoint(&cp.id.0);
                        }
                    }
                } else {
                    break;
                }
            }
        }

        // Persist to storage backend
        if let Some(ref store) = self.store {
            if let Err(e) = store.save_checkpoint(&self.session_id, &checkpoint) {
                tracing::warn!("failed to persist checkpoint: {}", e);
            }
        }

        tracing::info!("Created checkpoint {} for: {}", checkpoint.id, reason);

        Ok(checkpoint)
    }

    /// List available checkpoints
    pub fn list_checkpoints(&self) -> Vec<WorkspaceCheckpoint> {
        // If we have a store, load from there (authoritative source)
        if let Some(ref store) = self.store {
            if let Ok(loaded) = store.list_checkpoints(&self.session_id) {
                return loaded;
            }
        }
        // Fallback to in-memory cache
        let cache = self.checkpoints.read();
        let mut checkpoints: Vec<_> = cache.values().cloned().collect();
        checkpoints.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        checkpoints
    }

    /// Get a specific checkpoint by ID
    pub fn get_checkpoint(&self, id: &str) -> Option<WorkspaceCheckpoint> {
        let cache = self.checkpoints.read();
        cache.get(id).cloned()
    }

    /// Restore workspace to a specific checkpoint
    pub fn restore(&self, checkpoint_id: &str, mode: RestoreMode) -> Result<()> {
        let checkpoint = self
            .get_checkpoint(checkpoint_id)
            .context("checkpoint not found")?;

        if checkpoint.commit_hash.is_empty() {
            tracing::warn!("checkpoint has no commit hash, nothing to restore");
            return Ok(());
        }

        let work_dir = self.shadow_dir.join("workspace");

        match mode {
            RestoreMode::FilesOnly => {
                // Checkout specific files from the commit
                let output = Command::new("git")
                    .args([
                        "-C",
                        &work_dir.to_string_lossy(),
                        "checkout",
                        &checkpoint.commit_hash,
                        "--",
                        ".",
                    ])
                    .output()
                    .context("failed to checkout files")?;

                if !output.status.success() {
                    anyhow::bail!(
                        "restore failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            RestoreMode::Full => {
                // Hard reset to commit
                let output = Command::new("git")
                    .args([
                        "-C",
                        &work_dir.to_string_lossy(),
                        "reset",
                        "--hard",
                        &checkpoint.commit_hash,
                    ])
                    .output()
                    .context("failed to reset")?;

                if !output.status.success() {
                    anyhow::bail!(
                        "restore failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }

        tracing::info!("Restored workspace to checkpoint {}", checkpoint_id);

        Ok(())
    }

    /// Get diff between two checkpoints
    pub fn diff(&self, from_id: &str, to_id: &str) -> Result<String> {
        let from = self
            .get_checkpoint(from_id)
            .context("from checkpoint not found")?;
        let to = self
            .get_checkpoint(to_id)
            .context("to checkpoint not found")?;

        if from.commit_hash.is_empty() || to.commit_hash.is_empty() {
            return Ok(String::new());
        }

        let work_dir = self.shadow_dir.join("workspace");
        let output = Command::new("git")
            .args([
                "-C",
                &work_dir.to_string_lossy(),
                "diff",
                &from.commit_hash,
                &to.commit_hash,
            ])
            .output()
            .context("failed to diff checkpoints")?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// Check if a tool should trigger auto-checkpoint
pub fn should_auto_checkpoint(tool_name: &str, config: &CheckpointConfig) -> bool {
    if !config.enabled {
        return false;
    }
    config.auto_trigger_tools.iter().any(|t| t == tool_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[allow(dead_code)]
    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-checkpoint-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[allow(dead_code)]
    fn temp_workspace() -> (PathBuf, PathBuf) {
        let ws = temp_dir();
        let ckpt = temp_dir();
        (ws, ckpt)
    }

    #[test]
    fn checkpoint_id_display() {
        let id = CheckpointId::new();
        assert!(!id.0.is_empty());
        assert_eq!(id.to_string(), id.0);
    }

    #[test]
    fn checkpoint_config_defaults() {
        let config = CheckpointConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_checkpoints, 20);
        assert!(config.auto_trigger_tools.contains(&"edit".to_string()));
    }

    #[test]
    fn should_auto_checkpoint_enabled() {
        let config = CheckpointConfig::default();
        assert!(should_auto_checkpoint("edit", &config));
        assert!(should_auto_checkpoint("write", &config));
        assert!(should_auto_checkpoint("bash", &config));
    }

    #[test]
    fn should_auto_checkpoint_disabled() {
        let config = CheckpointConfig {
            enabled: false,
            ..CheckpointConfig::default()
        };
        assert!(!should_auto_checkpoint("edit", &config));
    }

    #[test]
    fn restore_mode_serialize() {
        let json = serde_json::to_string(&RestoreMode::FilesOnly).unwrap();
        assert!(json.contains("files-only"));

        let json = serde_json::to_string(&RestoreMode::Full).unwrap();
        assert!(json.contains("full"));
    }

    #[test]
    fn checkpoint_serialize() {
        let checkpoint = WorkspaceCheckpoint {
            id: CheckpointId::new(),
            commit_hash: "abc123".to_string(),
            message: "test checkpoint".to_string(),
            created_at: Utc::now(),
            files_changed: 5,
            reason: "testing".to_string(),
        };

        let json = serde_json::to_string(&checkpoint).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("test checkpoint"));
    }
}
