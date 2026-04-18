//! Layered Memory System
//!
//! Provides a tiered memory architecture:
//! - Shared: Global team project state.
//! - Private: Agent-specific scratchpads.
//! - Background persistence: Auto-syncing memory state to disk.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub content: String,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    pub access_count: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub trait MemoryStore: Send + Sync {
    fn read(&self, key: &str) -> Option<MemoryEntry>;
    fn write(&self, key: &str, value: MemoryEntry);
    fn delete(&self, key: &str);
    fn get_all(&self) -> HashMap<String, MemoryEntry>;
    fn compact(&self, threshold: f64);
    fn persist(&self) -> Result<()>;
}

/// A tiered memory controller that manages automatic persistence
pub struct LayeredMemory {
    project: Arc<dyn MemoryStore>,
    shared: Arc<dyn MemoryStore>,
    project_root: PathBuf,
    private: Arc<RwLock<HashMap<String, Arc<dyn MemoryStore>>>>,
}

impl LayeredMemory {
    pub fn new(project_root: PathBuf) -> Self {
        let project_path = project_root.join(".rustycode/memory/project.json");
        let shared_path = project_root.join(".rustycode/memory/shared.json");
        Self {
            project: Arc::new(FileBackedStore::new(project_path)),
            shared: Arc::new(FileBackedStore::new(shared_path)),
            project_root,
            private: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_project(&self) -> Arc<dyn MemoryStore> {
        self.project.clone()
    }

    pub fn get_shared(&self) -> Arc<dyn MemoryStore> {
        self.shared.clone()
    }

    pub async fn get_private(&self, agent_id: &str) -> Arc<dyn MemoryStore> {
        let mut private = self.private.write().await;
        if let Some(store) = private.get(agent_id) {
            store.clone()
        } else {
            let path = self.project_root.join(format!(".rustycode/memory/agents/{}.json", agent_id));
            let store = Arc::new(FileBackedStore::new(path));
            private.insert(agent_id.to_string(), store.clone());
            store
        }
    }

    pub async fn resolve_conflicts(&self) {}
}

pub struct FileBackedStore {
    path: PathBuf,
    data: Arc<RwLock<HashMap<String, MemoryEntry>>>,
}

impl FileBackedStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl MemoryStore for FileBackedStore {
    fn read(&self, key: &str) -> Option<MemoryEntry> {
        let mut data = self.data.try_write().ok()?;
        if let Some(entry) = data.get_mut(key) {
            entry.access_count += 1;
            entry.last_accessed = chrono::Utc::now();
            return Some(entry.clone());
        }
        None
    }

    fn write(&self, key: &str, value: MemoryEntry) {
        if let Ok(mut data) = self.data.try_write() {
            data.insert(key.to_string(), value);
        }
    }

    fn delete(&self, key: &str) {
        if let Ok(mut data) = self.data.try_write() {
            data.remove(key);
        }
    }

    fn get_all(&self) -> HashMap<String, MemoryEntry> {
        self.data.try_read().map(|d| d.clone()).unwrap_or_default()
    }

    fn compact(&self, threshold: f64) {
        if let Ok(mut data) = self.data.try_write() {
            let now = chrono::Utc::now();
            data.retain(|_, entry| {
                let age = (now - entry.created_at).num_hours() as f64;
                let score = (entry.access_count as f64 * 10.0) / (age + 1.0).powf(1.5);
                score > threshold
            });
        }
    }

    fn persist(&self) -> Result<()> {
        let data = self.data.try_read().map_err(|_| anyhow!("Busy"))?;
        let content = serde_json::to_string_pretty(&*data)?;
        std::fs::create_dir_all(self.path.parent().unwrap_or(Path::new(".")))?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }
}

pub async fn start_memory_autosave(
    memory: Arc<LayeredMemory>, 
    interval: std::time::Duration
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let _ = memory.get_project().persist();
        let _ = memory.get_shared().persist();
        let private = memory.private.read().await;
        for store in private.values() {
            let _ = store.persist();
        }
    }
}
