//! Orchestra Commands Config — Tool API key management
//!
//! Contains: TOOL_KEYS, load_tool_api_keys, get_config_auth_storage, handle_config

use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};

/// Tool API key configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolKeyConfig {
    pub id: String,
    pub env: String,
    pub label: String,
    pub hint: String,
}

/// Tool API key configurations
///
/// This is the source of truth for tool credentials - used by both the config wizard
/// and session startup to load keys from auth.json into environment variables.
pub fn get_tool_keys() -> Vec<ToolKeyConfig> {
    vec![
        ToolKeyConfig {
            id: "tavily".to_string(),
            env: "TAVILY_API_KEY".to_string(),
            label: "Tavily Search".to_string(),
            hint: "tavily.com/app/api-keys".to_string(),
        },
        ToolKeyConfig {
            id: "brave".to_string(),
            env: "BRAVE_API_KEY".to_string(),
            label: "Brave Search".to_string(),
            hint: "brave.com/search/api".to_string(),
        },
        ToolKeyConfig {
            id: "context7".to_string(),
            env: "CONTEXT7_API_KEY".to_string(),
            label: "Context7 Docs".to_string(),
            hint: "context7.com/dashboard".to_string(),
        },
        ToolKeyConfig {
            id: "jina".to_string(),
            env: "JINA_API_KEY".to_string(),
            label: "Jina Page Extract".to_string(),
            hint: "jina.ai/api".to_string(),
        },
        ToolKeyConfig {
            id: "groq".to_string(),
            env: "GROQ_API_KEY".to_string(),
            label: "Groq Voice".to_string(),
            hint: "console.groq.com".to_string(),
        },
    ]
}

/// Auth storage entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub key: Option<String>,
}

/// Auth storage
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthStorage {
    pub credentials: Vec<AuthStorageEntry>,
}

/// Auth storage entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStorageEntry {
    pub id: String,
    pub value: AuthEntry,
}

/// Load tool API keys from auth.json into environment variables
///
/// Called at session startup to ensure tools have access to their credentials.
///
/// # Example
/// ```
/// use rustycode_orchestra::commands_config::*;
///
/// load_tool_api_keys();
/// // Keys are now available in environment variables
/// ```
pub fn load_tool_api_keys() {
    let auth_path = get_auth_path();
    if !auth_path.exists() {
        return;
    }

    let auth = match load_auth_storage(&auth_path) {
        Some(a) => a,
        None => return,
    };

    let tool_keys = get_tool_keys();
    for tool in tool_keys {
        if let Some(cred) = auth.get(&tool.id) {
            if cred.entry_type == "api_key" {
                if let Some(key) = &cred.key {
                    if env::var(&tool.env).is_err() {
                        env::set_var(&tool.env, key);
                    }
                }
            }
        }
    }
}

/// Get config auth storage path
///
/// # Returns
/// Path to auth.json file
pub fn get_auth_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".orchestra")
        .join("agent")
        .join("auth.json")
}

/// Get config auth storage
///
/// Creates the auth directory if it doesn't exist and returns the auth path.
///
/// # Returns
/// Path to auth.json file
pub fn get_config_auth_storage_path() -> PathBuf {
    let auth_path = get_auth_path();

    // Create parent directory if it doesn't exist
    if let Some(parent) = auth_path.parent() {
        if !parent.exists() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    auth_path
}

/// Load auth storage from file
///
/// # Arguments
/// * `path` - Path to auth.json file
///
/// # Returns
/// AuthStorage or None if file doesn't exist or can't be read
fn load_auth_storage(path: &Path) -> Option<AuthStorage> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Get auth entry for a tool
///
/// # Arguments
/// * `auth` - Auth storage
/// * `tool` - Tool key configuration
///
/// # Returns
/// Auth entry or None
pub fn get_auth_entry(auth: &AuthStorage, tool: &ToolKeyConfig) -> Option<AuthEntry> {
    auth.credentials
        .iter()
        .find(|entry| entry.id == tool.id)
        .map(|entry| entry.value.clone())
}

impl AuthStorage {
    /// Get auth entry for a tool ID
    ///
    /// # Arguments
    /// * `tool_id` - Tool ID (e.g., "tavily")
    ///
    /// # Returns
    /// Auth entry or None
    pub fn get(&self, tool_id: &str) -> Option<&AuthEntry> {
        self.credentials
            .iter()
            .find(|entry| entry.id == tool_id)
            .map(|entry| &entry.value)
    }

    /// Set auth entry for a tool ID
    ///
    /// # Arguments
    /// * `tool_id` - Tool ID (e.g., "tavily")
    /// * `entry` - Auth entry to set
    pub fn set(&mut self, tool_id: &str, entry: AuthEntry) {
        // Remove existing entry if present
        self.credentials.retain(|e| e.id != tool_id);

        // Add new entry
        self.credentials.push(AuthStorageEntry {
            id: tool_id.to_string(),
            value: entry,
        });
    }

    /// Check if tool has a key configured
    ///
    /// # Arguments
    /// * `tool` - Tool key configuration
    ///
    /// # Returns
    /// true if tool has a key
    pub fn has_key(&self, tool: &ToolKeyConfig) -> bool {
        // Check environment variable first
        if env::var(&tool.env).is_ok() {
            return true;
        }

        // Check auth storage
        if let Some(entry) = self.get(&tool.id) {
            return entry.key.is_some();
        }

        false
    }

    /// Get configuration status for all tools
    ///
    /// # Returns
    /// Vector of (tool, has_key) tuples
    pub fn get_status(&self) -> Vec<(ToolKeyConfig, bool)> {
        get_tool_keys()
            .into_iter()
            .map(|tool| {
                let has_key = self.has_key(&tool);
                (tool, has_key)
            })
            .collect()
    }

    /// Save auth storage to file
    ///
    /// # Arguments
    /// * `path` - Path to save auth.json
    ///
    /// # Returns
    /// Result indicating success or error
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize auth: {}", e))?;

        std::fs::write(path, json).map_err(|e| format!("Failed to write auth: {}", e))
    }
}

/// Create new auth storage
///
/// # Arguments
/// * `path` - Path to auth.json file
///
/// # Returns
/// AuthStorage (loaded from file or new default)
pub fn create_auth_storage(path: &Path) -> AuthStorage {
    load_auth_storage(path).unwrap_or_default()
}

/// Get configuration status text
///
/// # Returns
/// Multi-line string showing status of all tools
pub fn get_config_status_text() -> String {
    let auth_path = get_config_auth_storage_path();
    let auth = create_auth_storage(&auth_path);

    let mut lines = vec!["Orchestra Tool Configuration".to_string()];

    for (tool, has_key) in auth.get_status() {
        if has_key {
            lines.push(format!("  ✓ {}", tool.label));
        } else {
            lines.push(format!("  ✗ {} — get key at {}", tool.label, tool.hint));
        }
    }

    lines.join("\n")
}

/// Check if a specific tool is configured
///
/// # Arguments
/// * `tool_id` - Tool ID (e.g., "tavily")
///
/// # Returns
/// true if tool has a key configured
pub fn is_tool_configured(tool_id: &str) -> bool {
    let tool_keys = get_tool_keys();

    // Check environment variable first
    if let Some(tool) = tool_keys.iter().find(|t| t.id == tool_id) {
        if env::var(&tool.env).is_ok() {
            return true;
        }
    }

    // Check auth storage
    let auth_path = get_auth_path();
    if let Some(auth) = load_auth_storage(&auth_path) {
        if let Some(entry) = auth.get(tool_id) {
            return entry.key.is_some();
        }
    }

    false
}

/// Set API key for a tool
///
/// # Arguments
/// * `tool_id` - Tool ID (e.g., "tavily")
/// * `api_key` - API key to set
///
/// # Returns
/// Result indicating success or error
pub fn set_tool_api_key(tool_id: &str, api_key: &str) -> Result<(), String> {
    let tool_keys = get_tool_keys();
    let tool = tool_keys
        .iter()
        .find(|t| t.id == tool_id)
        .ok_or_else(|| format!("Unknown tool ID: {}", tool_id))?;

    let auth_path = get_config_auth_storage_path();
    let mut auth = create_auth_storage(&auth_path);

    // Set the key in auth storage
    auth.set(
        tool_id,
        AuthEntry {
            entry_type: "api_key".to_string(),
            key: Some(api_key.to_string()),
        },
    );

    // Set environment variable
    env::set_var(&tool.env, api_key);

    // Save to file
    auth.save(&auth_path)
}

/// Remove API key for a tool
///
/// # Arguments
/// * `tool_id` - Tool ID (e.g., "tavily")
///
/// # Returns
/// Result indicating success or error
pub fn remove_tool_api_key(tool_id: &str) -> Result<(), String> {
    let tool_keys = get_tool_keys();
    let tool = tool_keys
        .iter()
        .find(|t| t.id == tool_id)
        .ok_or_else(|| format!("Unknown tool ID: {}", tool_id))?;

    let auth_path = get_config_auth_storage_path();
    let mut auth = create_auth_storage(&auth_path);

    // Remove from auth storage
    auth.credentials.retain(|e| e.id != tool_id);

    // Remove from environment
    env::remove_var(&tool.env);

    // Save to file
    auth.save(&auth_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_tool_keys_constants() {
        let keys = get_tool_keys();
        assert_eq!(keys.len(), 5);
        assert_eq!(keys[0].id, "tavily");
        assert_eq!(keys[0].env, "TAVILY_API_KEY");
    }

    #[test]
    fn test_auth_storage_default() {
        let auth = AuthStorage::default();
        assert_eq!(auth.credentials.len(), 0);
    }

    #[test]
    fn test_auth_storage_set_get() {
        let mut auth = AuthStorage::default();
        let keys = get_tool_keys();
        let tool = &keys[0];

        auth.set(
            &tool.id,
            AuthEntry {
                entry_type: "api_key".to_string(),
                key: Some("test-key".to_string()),
            },
        );

        assert_eq!(auth.credentials.len(), 1);

        let entry = auth.get(&tool.id);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().key.as_ref().unwrap(), "test-key");
    }

    #[test]
    fn test_auth_storage_has_key_env() {
        let auth = AuthStorage::default();
        let keys = get_tool_keys();
        let tool = &keys[0];

        // No key in env or storage
        assert!(!auth.has_key(tool));

        // Set in environment
        env::set_var(&tool.env, "env-key");
        assert!(auth.has_key(tool));

        // Clean up
        env::remove_var(&tool.env);
    }

    #[test]
    fn test_auth_storage_has_key_storage() {
        let mut auth = AuthStorage::default();
        let keys = get_tool_keys();
        let tool = &keys[0];

        // Add key to storage
        auth.set(
            &tool.id,
            AuthEntry {
                entry_type: "api_key".to_string(),
                key: Some("stored-key".to_string()),
            },
        );

        assert!(auth.has_key(tool));
    }

    #[test]
    fn test_get_config_status_text() {
        let text = get_config_status_text();
        assert!(text.contains("Orchestra Tool Configuration"));
        assert!(text.contains("Tavily Search"));
    }

    #[test]
    fn test_set_and_remove_tool_api_key() {
        // Use a unique temp directory to avoid conflicts
        let tool_id = "tavily";
        let original_home = env::var("HOME");
        let original_key = env::var("TAVILY_API_KEY");

        let temp_dir = TempDir::new().unwrap();

        // Override auth path for this test
        env::set_var("HOME", temp_dir.path().to_string_lossy().to_string());

        // Set the key
        let result = set_tool_api_key(tool_id, "test-key-123");
        assert!(result.is_ok());

        // Verify key is set in environment
        assert_eq!(env::var("TAVILY_API_KEY").unwrap(), "test-key-123");

        // Remove the key
        let result = remove_tool_api_key(tool_id);
        assert!(result.is_ok());

        // Verify key is removed
        assert!(env::var("TAVILY_API_KEY").is_err());

        // Clean up - restore original state
        if let Ok(home) = original_home {
            env::set_var("HOME", home)
        }

        match original_key {
            Ok(key) => env::set_var("TAVILY_API_KEY", key),
            Err(_) => env::remove_var("TAVILY_API_KEY"),
        }
    }

    #[test]
    fn test_unknown_tool_id() {
        let result = set_tool_api_key("unknown-tool", "key");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown tool ID"));
    }
}
