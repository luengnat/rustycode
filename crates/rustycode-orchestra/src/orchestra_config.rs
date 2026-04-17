//!
//! Per-project Orchestra configuration, loaded from `.orchestra/config.json`.
//!
//! Allows task-specific model selection (e.g., "testing": "claude-3-haiku").
//!

use anyhow::{Context, Result};
use rustycode_llm::TaskType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Project-level Orchestra configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrchestraProjectConfig {
    /// Task type → model ID mapping (e.g., "testing" → "claude-3-haiku")
    #[serde(default)]
    pub task_models: HashMap<String, String>,
}

impl OrchestraProjectConfig {
    /// Load from `.orchestra/config.json` in the project root.
    /// Returns `Ok(None)` if the file doesn't exist.
    /// Returns `Err` if the file exists but is invalid JSON.
    pub fn load(project_root: &Path) -> Result<Option<Self>> {
        let config_path = project_root.join(".orchestra/config.json");

        if !config_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&config_path)
            .context("Failed to read .orchestra/config.json")?;

        let config: OrchestraProjectConfig = serde_json::from_str(&content)
            .context("Failed to parse .orchestra/config.json as JSON")?;

        Ok(Some(config))
    }

    /// Get the model override for a specific task type, if configured.
    pub fn model_for_task(&self, task_type: TaskType) -> Option<&str> {
        let key = match task_type {
            TaskType::Planning => "planning",
            TaskType::Testing => "testing",
            TaskType::CodeGeneration => "code_generation",
            TaskType::Research => "research",
            _ => "general",
        };

        self.task_models.get(key).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_missing_config_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create .orchestra directory but no config.json
        fs::create_dir_all(root.join(".orchestra")).unwrap();

        let result = OrchestraProjectConfig::load(root).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_load_valid_config() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let orchestra_dir = root.join(".orchestra");
        fs::create_dir_all(&orchestra_dir).unwrap();

        let config_json = r#"{
  "task_models": {
    "testing": "claude-3-haiku-20240307",
    "planning": "claude-3-opus-20240229"
  }
}"#;
        fs::write(orchestra_dir.join("config.json"), config_json).unwrap();

        let config = OrchestraProjectConfig::load(root).unwrap().unwrap();
        assert_eq!(
            config.model_for_task(TaskType::Testing),
            Some("claude-3-haiku-20240307")
        );
        assert_eq!(
            config.model_for_task(TaskType::Planning),
            Some("claude-3-opus-20240229")
        );
    }

    #[test]
    fn test_load_invalid_json_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let orchestra_dir = root.join(".orchestra");
        fs::create_dir_all(&orchestra_dir).unwrap();

        fs::write(orchestra_dir.join("config.json"), "{ invalid json }").unwrap();

        let result = OrchestraProjectConfig::load(root);
        assert!(result.is_err());
    }

    #[test]
    fn test_model_for_task_returns_none_when_not_configured() {
        let config = OrchestraProjectConfig::default();
        assert_eq!(config.model_for_task(TaskType::Testing), None);
        assert_eq!(config.model_for_task(TaskType::Planning), None);
    }
}
