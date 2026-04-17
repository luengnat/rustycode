//! Task configuration parser for Harbor task.toml format.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Deserialize;

/// Parsed task configuration from Harbor task.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskConfig {
    /// Task metadata (author, difficulty, category).
    #[serde(default)]
    pub metadata: TaskMetadata,
    /// Verifier configuration.
    #[serde(default)]
    pub verifier: VerifierConfig,
    /// Agent configuration.
    #[serde(default)]
    pub agent: AgentConfig,
    /// Environment configuration.
    #[serde(default)]
    pub environment: EnvironmentConfig,
}

/// Task metadata section.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TaskMetadata {
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub author_email: String,
    #[serde(default)]
    pub difficulty: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Verifier configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct VerifierConfig {
    #[serde(default = "default_timeout")]
    pub timeout_sec: f64,
}

impl Default for VerifierConfig {
    fn default() -> Self {
        Self {
            timeout_sec: default_timeout(),
        }
    }
}

/// Agent configuration section.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_timeout")]
    pub timeout_sec: f64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            timeout_sec: default_timeout(),
        }
    }
}

/// Environment configuration section from task.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct EnvironmentConfig {
    #[serde(default = "default_build_timeout")]
    pub build_timeout_sec: f64,
    #[serde(default)]
    pub docker_image: Option<String>,
    #[serde(default = "default_cpus")]
    pub cpus: u32,
    /// Memory limit as a string (e.g. "2G", "512M").
    #[serde(default = "default_memory")]
    pub memory: String,
    #[serde(default)]
    pub storage: Option<String>,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            build_timeout_sec: default_build_timeout(),
            docker_image: None,
            cpus: default_cpus(),
            memory: default_memory(),
            storage: None,
        }
    }
}

const fn default_timeout() -> f64 {
    900.0
}

const fn default_build_timeout() -> f64 {
    600.0
}

const fn default_cpus() -> u32 {
    1
}

fn default_memory() -> String {
    "2G".to_string()
}

/// Resolved task with all paths and configuration.
#[derive(Debug, Clone)]
pub struct ResolvedTask {
    /// Unique task name (e.g. "sparql-university").
    pub name: String,
    /// Root directory of the task.
    pub task_dir: PathBuf,
    /// Parsed task.toml configuration.
    pub config: TaskConfig,
    /// Task instruction text (from instruction.md).
    pub instruction: String,
    /// Path to the environment directory (contains Dockerfile).
    pub environment_dir: PathBuf,
    /// Path to the tests directory.
    pub tests_dir: PathBuf,
    /// Path to the solution directory (oracle).
    pub solution_dir: PathBuf,
}

impl ResolvedTask {
    /// Resolve a task from its root directory.
    ///
    /// Expected structure:
    /// ```text
    /// task-name/
    /// ├── task.toml
    /// ├── instruction.md
    /// ├── environment/
    /// │   └── Dockerfile
    /// ├── tests/
    /// │   └── test.sh
    /// └── solution/
    ///     └── solve.sh
    /// ```
    pub fn from_dir(task_dir: &Path) -> anyhow::Result<Self> {
        let name = task_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Parse task.toml
        let toml_path = task_dir.join("task.toml");
        let toml_content = std::fs::read_to_string(&toml_path)
            .with_context(|| format!("reading task.toml from {}", toml_path.display()))?;
        let config: TaskConfig = toml::from_str(&toml_content)
            .with_context(|| format!("parsing task.toml from {}", toml_path.display()))?;

        // Read instruction.md
        let instruction_path = task_dir.join("instruction.md");
        let instruction = std::fs::read_to_string(&instruction_path).with_context(|| {
            format!("reading instruction.md from {}", instruction_path.display())
        })?;

        let environment_dir = task_dir.join("environment");
        let tests_dir = task_dir.join("tests");
        let solution_dir = task_dir.join("solution");

        // Validate required paths
        if !environment_dir.join("Dockerfile").exists() {
            bail!(
                "Dockerfile not found at {}",
                environment_dir.join("Dockerfile").display()
            );
        }

        Ok(Self {
            name,
            task_dir: task_dir.to_path_buf(),
            config,
            instruction,
            environment_dir,
            tests_dir,
            solution_dir,
        })
    }

    /// Discover all tasks in a dataset directory.
    ///
    /// Scans for subdirectories containing `task.toml`.
    pub fn discover(dataset_dir: &Path) -> anyhow::Result<Vec<Self>> {
        let mut tasks = Vec::new();

        let entries = std::fs::read_dir(dataset_dir)
            .with_context(|| format!("reading dataset dir {}", dataset_dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && path.join("task.toml").exists() {
                match Self::from_dir(&path) {
                    Ok(task) => tasks.push(task),
                    Err(e) => {
                        tracing::warn!("Skipping task at {}: {}", path.display(), e);
                    }
                }
            }
        }

        tasks.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tasks)
    }
}
