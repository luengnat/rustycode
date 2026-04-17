//! Dataset discovery and loading.
//!
//! Provides utilities for finding benchmark task datasets on the local
//! filesystem, including Harbor's cache directory.

use std::path::{Path, PathBuf};

/// Default Harbor cache directory.
const HARBOR_CACHE_DIR: &str = ".cache/harbor/tasks";

/// Registry for discovering benchmark datasets.
///
/// A dataset is a directory containing task subdirectories, each with
/// a `task.toml`, `instruction.md`, and `environment/Dockerfile`.
pub struct DatasetRegistry {
    /// Search paths for datasets.
    search_paths: Vec<PathBuf>,
}

impl DatasetRegistry {
    /// Create a new registry with default search paths.
    ///
    /// Searches in:
    /// - `~/.cache/harbor/tasks/` (Harbor cache)
    /// - Current directory
    pub fn new() -> Self {
        let mut search_paths = Vec::new();

        // Harbor cache directory
        if let Ok(home) = std::env::var("HOME") {
            let harbor_cache = PathBuf::from(home).join(HARBOR_CACHE_DIR);
            if harbor_cache.exists() {
                search_paths.push(harbor_cache);
            }
        }

        Self { search_paths }
    }

    /// Create a registry with custom search paths.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_paths(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths }
    }

    /// List all available datasets across search paths.
    ///
    /// A dataset is a directory that contains task subdirectories
    /// (directories with `task.toml` files).
    pub fn list_datasets(&self) -> Vec<DatasetInfo> {
        let mut datasets = Vec::new();

        for search_path in &self.search_paths {
            if !search_path.exists() {
                continue;
            }

            let Ok(entries) = std::fs::read_dir(search_path) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                // Check if this contains task directories
                let task_count = count_tasks(&path);
                if task_count > 0 {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    datasets.push(DatasetInfo {
                        name,
                        path,
                        task_count,
                    });
                }
            }
        }

        datasets.sort_by(|a, b| a.name.cmp(&b.name));
        datasets
    }

    /// Find a dataset by name across all search paths.
    pub fn find_dataset(&self, name: &str) -> Option<PathBuf> {
        for search_path in &self.search_paths {
            let candidate = search_path.join(name);
            if candidate.exists() && count_tasks(&candidate) > 0 {
                return Some(candidate);
            }
        }
        None
    }

    /// Resolve a dataset reference to a path.
    ///
    /// Accepts:
    /// - A direct path (if it exists)
    /// - A dataset name (searched in registry paths)
    /// - `name@version` format (searched by name prefix)
    pub fn resolve(&self, reference: &str) -> anyhow::Result<PathBuf> {
        // Try as direct path first
        let direct = PathBuf::from(reference);
        if direct.exists() {
            return Ok(direct);
        }

        // Try as dataset name
        if let Some(path) = self.find_dataset(reference) {
            return Ok(path);
        }

        // Try name@version format — strip version and search
        let base_name = reference.split('@').next().unwrap_or(reference);
        if let Some(path) = self.find_dataset(base_name) {
            return Ok(path);
        }

        // Search inside Harbor cache for nested dataset dirs
        if let Ok(home) = std::env::var("HOME") {
            let harbor_cache = PathBuf::from(home).join(HARBOR_CACHE_DIR);
            if harbor_cache.exists() {
                let entries = std::fs::read_dir(&harbor_cache)?;
                for entry in entries.flatten() {
                    let session_dir = entry.path();
                    if !session_dir.is_dir() {
                        continue;
                    }

                    // Check if this session dir contains tasks
                    let task_count = count_tasks(&session_dir);
                    if task_count > 0 {
                        return Ok(session_dir);
                    }
                }
            }
        }

        anyhow::bail!("Dataset not found: {reference}")
    }
}

impl Default for DatasetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a discovered dataset.
#[derive(Debug, Clone)]
pub struct DatasetInfo {
    /// Dataset name (directory name).
    pub name: String,
    /// Path to the dataset directory.
    pub path: PathBuf,
    /// Number of tasks in the dataset.
    pub task_count: usize,
}

/// Count the number of task directories in a given directory.
fn count_tasks(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    entries
        .flatten()
        .filter(|e| e.path().is_dir() && e.path().join("task.toml").exists())
        .count()
}
