//! Pi migration — one-time migration of provider credentials from Pi.
//!
//! Migrates provider credentials from Pi (~/.pi/agent/auth.json)
//! into Orchestra's auth storage. Runs when Orchestra has no LLM providers configured,
//! so users with an existing Pi install skip re-authentication.
//!
//! Matches orchestra-2's pi-migration.ts implementation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self};
use std::path::PathBuf;

/// Pi auth storage path (~/.pi/agent/auth.json)
pub fn pi_auth_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pi")
        .join("agent")
        .join("auth.json")
}

/// Pi settings path (~/.pi/agent/settings.json)
pub fn pi_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pi")
        .join("agent")
        .join("settings.json")
}

/// LLM provider IDs that should trigger onboarding skip when migrated
pub const LLM_PROVIDER_IDS: &[&str] = &[
    "anthropic",
    "openai",
    "github-copilot",
    "openai-codex",
    "google-gemini-cli",
    "google-antigravity",
    "google",
    "groq",
    "xai",
    "openrouter",
    "mistral",
];

/// Auth credential structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCredential {
    /// Credential type (e.g., "api_key", "bearer_token")
    #[serde(rename = "type")]
    pub cred_type: String,

    /// The actual credential value (e.g., API key)
    pub key: Option<String>,

    /// Additional metadata
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Result of credential migration
#[derive(Debug, Clone)]
pub struct MigrationResult {
    /// Whether any LLM provider was migrated
    pub migrated_llm: bool,

    /// Number of credentials migrated
    pub count: usize,

    /// Names of migrated providers
    pub providers: Vec<String>,
}

/// Migrate provider credentials from Pi's auth.json into Orchestra's auth storage.
///
/// Only runs when Orchestra has no LLM provider configured and Pi's auth.json exists.
/// Copies any credentials Orchestra doesn't already have. Returns migration result.
///
/// # Arguments
/// * `existing_providers` - Set of provider IDs already configured in Orchestra
/// * `pi_auth_path_override` - Optional override for Pi auth path (for testing)
///
/// # Returns
/// MigrationResult indicating what was migrated (empty if nothing)
///
/// # Examples
/// ```no_run
/// use rustycode_orchestra::pi_migration::migrate_pi_credentials;
/// use std::collections::HashSet;
///
/// let existing = HashSet::new();
/// let result = migrate_pi_credentials(&existing, None);
/// if result.migrated_llm {
///     println!("Migrated {} LLM providers", result.count);
/// }
/// ```
///
/// # Errors
/// Returns empty MigrationResult if:
/// - Orchestra already has LLM providers configured
/// - Pi auth file doesn't exist
/// - File is malformed or can't be read
pub fn migrate_pi_credentials(
    existing_providers: &HashMap<String, AuthCredential>,
    pi_auth_path_override: Option<PathBuf>,
) -> MigrationResult {
    // Check if Orchestra already has LLM providers
    let has_llm = existing_providers
        .keys()
        .any(|id| LLM_PROVIDER_IDS.contains(&id.as_str()));
    if has_llm {
        return MigrationResult {
            migrated_llm: false,
            count: 0,
            providers: Vec::new(),
        };
    }

    // Use override path or default Pi auth path
    let auth_path = pi_auth_path_override.unwrap_or_else(pi_auth_path);

    // Check if Pi auth file exists
    if !auth_path.exists() {
        return MigrationResult {
            migrated_llm: false,
            count: 0,
            providers: Vec::new(),
        };
    }

    // Read and parse Pi auth file
    let raw = match fs::read_to_string(&auth_path) {
        Ok(content) => content,
        Err(_) => {
            return MigrationResult {
                migrated_llm: false,
                count: 0,
                providers: Vec::new(),
            };
        }
    };

    let pi_data: HashMap<String, AuthCredential> = match serde_json::from_str(&raw) {
        Ok(data) => data,
        Err(_) => {
            return MigrationResult {
                migrated_llm: false,
                count: 0,
                providers: Vec::new(),
            };
        }
    };

    let mut migrated_llm = false;
    let mut providers = Vec::new();

    for (provider_id, _credential) in pi_data {
        // Skip if already exists in Orchestra
        if existing_providers.contains_key(&provider_id) {
            continue;
        }

        // Check if this is an LLM provider
        let is_llm = LLM_PROVIDER_IDS.contains(&provider_id.as_str());
        if is_llm {
            migrated_llm = true;
        }

        providers.push(provider_id.clone());
    }

    MigrationResult {
        migrated_llm,
        count: providers.len(),
        providers,
    }
}

/// Get Pi's default model and provider from settings.json.
///
/// # Arguments
/// * `pi_settings_path_override` - Optional override for Pi settings path (for testing)
///
/// # Returns
/// Some((provider, model)) if found, None otherwise
///
/// # Examples
/// ```no_run
/// use rustycode_orchestra::pi_migration::get_pi_default_model_and_provider;
///
/// if let Some((provider, model)) = get_pi_default_model_and_provider(None) {
///     println!("Pi default: {} / {}", provider, model);
/// }
/// ```
///
/// # Errors
/// Returns None if:
/// - Pi settings file doesn't exist
/// - File is malformed or can't be read
/// - Required fields are missing or wrong type
pub fn get_pi_default_model_and_provider(
    pi_settings_path_override: Option<PathBuf>,
) -> Option<(String, String)> {
    // Use override path or default Pi settings path
    let settings_path = pi_settings_path_override.unwrap_or_else(pi_settings_path);

    if !settings_path.exists() {
        return None;
    }

    let raw = fs::read_to_string(&settings_path).ok()?;

    let data: Value = serde_json::from_str(&raw).ok()?;

    let default_provider = data.get("defaultProvider")?.as_str()?;
    let default_model = data.get("defaultModel")?.as_str()?;

    Some((default_provider.to_string(), default_model.to_string()))
}

/// Check if a provider ID is an LLM provider
///
/// # Arguments
/// * `provider_id` - Provider ID to check
///
/// # Returns
/// true if the provider is an LLM provider
///
/// # Examples
/// ```
/// use rustycode_orchestra::pi_migration::is_llm_provider;
///
/// assert!(is_llm_provider("anthropic"));
/// assert!(is_llm_provider("openai"));
/// assert!(!is_llm_provider("github"));  // Not an LLM provider
/// ```
pub fn is_llm_provider(provider_id: &str) -> bool {
    LLM_PROVIDER_IDS.contains(&provider_id)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_pi_auth_path() {
        let path = pi_auth_path();
        assert!(path.ends_with(".pi/agent/auth.json") || path.ends_with(".pi\\agent\\auth.json"));
    }

    #[test]
    fn test_pi_settings_path() {
        let path = pi_settings_path();
        assert!(
            path.ends_with(".pi/agent/settings.json")
                || path.ends_with(".pi\\agent\\settings.json")
        );
    }

    #[test]
    fn test_is_llm_provider_known_providers() {
        assert!(is_llm_provider("anthropic"));
        assert!(is_llm_provider("openai"));
        assert!(is_llm_provider("github-copilot"));
        assert!(is_llm_provider("google-gemini-cli"));
        assert!(is_llm_provider("groq"));
        assert!(is_llm_provider("xai"));
    }

    #[test]
    fn test_is_llm_provider_false_for_others() {
        assert!(!is_llm_provider("github"));
        assert!(!is_llm_provider("gitlab"));
        assert!(!is_llm_provider("unknown"));
    }

    #[test]
    fn test_migrate_pi_credentials_no_existing_providers() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let auth_path = temp_dir.path().join("auth.json");

        // Create Pi auth file
        let mut file = File::create(&auth_path).expect("Failed to create auth file");
        let auth_content = r#"{
            "anthropic": {"type": "api_key", "key": "sk-test-placeholder-1"},
            "openai": {"type": "api_key", "key": "sk-test-placeholder-2"}
        }"#;
        file.write_all(auth_content.as_bytes())
            .expect("Failed to write");

        let existing = HashMap::new();
        let result = migrate_pi_credentials(&existing, Some(auth_path));

        assert!(result.migrated_llm);
        assert_eq!(result.count, 2);
        assert_eq!(result.providers.len(), 2);
    }

    #[test]
    fn test_migrate_pi_credentials_with_existing_llm() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let auth_path = temp_dir.path().join("auth.json");

        // Create Pi auth file
        let mut file = File::create(&auth_path).expect("Failed to create auth file");
        let auth_content = r#"{
            "anthropic": {"type": "api_key", "key": "sk-test-placeholder-1"}
        }"#;
        file.write_all(auth_content.as_bytes())
            .expect("Failed to write");

        // Orchestra already has an LLM provider
        let mut existing = HashMap::new();
        existing.insert(
            "openai".to_string(),
            AuthCredential {
                cred_type: "api_key".to_string(),
                key: Some("sk-existing".to_string()),
                extra: HashMap::new(),
            },
        );

        let result = migrate_pi_credentials(&existing, Some(auth_path));

        // Should not migrate if Orchestra already has LLM
        assert!(!result.migrated_llm);
        assert_eq!(result.count, 0);
    }

    #[test]
    fn test_migrate_pi_credentials_skips_existing() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let auth_path = temp_dir.path().join("auth.json");

        // Create Pi auth file
        let mut file = File::create(&auth_path).expect("Failed to create auth file");
        let auth_content = r#"{
            "anthropic": {"type": "api_key", "key": "sk-test-placeholder-1"},
            "openai": {"type": "api_key", "key": "sk-test-placeholder-2"}
        }"#;
        file.write_all(auth_content.as_bytes())
            .expect("Failed to write");

        // Orchestra already has anthropic (an LLM provider)
        let mut existing = HashMap::new();
        existing.insert(
            "anthropic".to_string(),
            AuthCredential {
                cred_type: "api_key".to_string(),
                key: Some("sk-existing".to_string()),
                extra: HashMap::new(),
            },
        );

        let result = migrate_pi_credentials(&existing, Some(auth_path));

        // Should not migrate anything if Orchestra already has an LLM provider
        assert!(!result.migrated_llm);
        assert_eq!(result.count, 0);
        assert_eq!(result.providers.len(), 0);
    }

    #[test]
    fn test_migrate_pi_credentials_nonexistent_file() {
        let existing = HashMap::new();
        let result =
            migrate_pi_credentials(&existing, Some(PathBuf::from("/nonexistent/auth.json")));

        assert!(!result.migrated_llm);
        assert_eq!(result.count, 0);
    }

    #[test]
    fn test_migrate_pi_credentials_malformed_json() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let auth_path = temp_dir.path().join("auth.json");

        // Create malformed auth file
        let mut file = File::create(&auth_path).expect("Failed to create auth file");
        file.write_all(br"{invalid json").expect("Failed to write");

        let existing = HashMap::new();
        let result = migrate_pi_credentials(&existing, Some(auth_path));

        assert!(!result.migrated_llm);
        assert_eq!(result.count, 0);
    }

    #[test]
    fn test_migrate_pi_credentials_empty_auth_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let auth_path = temp_dir.path().join("auth.json");

        // Create empty auth file
        let mut file = File::create(&auth_path).expect("Failed to create auth file");
        file.write_all(b"{}").expect("Failed to write");

        let existing = HashMap::new();
        let result = migrate_pi_credentials(&existing, Some(auth_path));

        assert!(!result.migrated_llm);
        assert_eq!(result.count, 0);
    }

    #[test]
    fn test_get_pi_default_model_and_provider_success() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let settings_path = temp_dir.path().join("settings.json");

        // Create settings file
        let mut file = File::create(&settings_path).expect("Failed to create settings file");
        let settings_content = r#"{
            "defaultProvider": "anthropic",
            "defaultModel": "claude-sonnet-4-20250514"
        }"#;
        file.write_all(settings_content.as_bytes())
            .expect("Failed to write");

        let result = get_pi_default_model_and_provider(Some(settings_path));

        assert!(result.is_some());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4-6");
    }

    #[test]
    fn test_get_pi_default_model_and_provider_nonexistent_file() {
        let result =
            get_pi_default_model_and_provider(Some(PathBuf::from("/nonexistent/settings.json")));

        assert!(result.is_none());
    }

    #[test]
    fn test_get_pi_default_model_and_provider_malformed_json() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let settings_path = temp_dir.path().join("settings.json");

        // Create malformed settings file
        let mut file = File::create(&settings_path).expect("Failed to create settings file");
        file.write_all(br"{invalid json").expect("Failed to write");

        let result = get_pi_default_model_and_provider(Some(settings_path));

        assert!(result.is_none());
    }

    #[test]
    fn test_get_pi_default_model_and_provider_missing_fields() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let settings_path = temp_dir.path().join("settings.json");

        // Create settings file without required fields
        let mut file = File::create(&settings_path).expect("Failed to create settings file");
        file.write_all(b"{}").expect("Failed to write");

        let result = get_pi_default_model_and_provider(Some(settings_path));

        assert!(result.is_none());
    }

    #[test]
    fn test_get_pi_default_model_and_provider_wrong_types() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let settings_path = temp_dir.path().join("settings.json");

        // Create settings file with wrong types
        let mut file = File::create(&settings_path).expect("Failed to create settings file");
        file.write_all(br#"{"defaultProvider": 123, "defaultModel": true}"#)
            .expect("Failed to write");

        let result = get_pi_default_model_and_provider(Some(settings_path));

        assert!(result.is_none());
    }

    #[test]
    fn test_llm_provider_ids_constant() {
        assert!(LLM_PROVIDER_IDS.contains(&"anthropic"));
        assert!(LLM_PROVIDER_IDS.contains(&"openai"));
        assert!(LLM_PROVIDER_IDS.contains(&"groq"));
        assert_eq!(LLM_PROVIDER_IDS.len(), 11);
    }

    #[test]
    fn test_migration_result_structure() {
        let result = MigrationResult {
            migrated_llm: true,
            count: 2,
            providers: vec!["anthropic".to_string(), "openai".to_string()],
        };

        assert!(result.migrated_llm);
        assert_eq!(result.count, 2);
        assert_eq!(result.providers.len(), 2);
    }

    #[test]
    fn test_auth_credential_deserialization() {
        let json = r#"{"type": "api_key", "key": "sk-test-placeholder"}"#;
        let credential: AuthCredential = serde_json::from_str(json).unwrap();

        assert_eq!(credential.cred_type, "api_key");
        assert_eq!(credential.key, Some("sk-test-placeholder".to_string()));
    }

    #[test]
    fn test_auth_credential_serialization() {
        let credential = AuthCredential {
            cred_type: "api_key".to_string(),
            key: Some("sk-test-placeholder".to_string()),
            extra: HashMap::new(),
        };

        let json = serde_json::to_string(&credential).unwrap();
        assert!(json.contains("\"type\":\"api_key\""));
        assert!(json.contains("\"key\":\"sk-test-placeholder\""));
    }

    #[test]
    fn test_auth_credential_with_extra_fields() {
        let json = r#"{"type": "api_key", "key": "sk-test-placeholder", "expires": "2024-01-01"}"#;
        let credential: AuthCredential = serde_json::from_str(json).unwrap();

        assert_eq!(credential.cred_type, "api_key");
        assert_eq!(credential.key, Some("sk-test-placeholder".to_string()));
        assert!(credential.extra.contains_key("expires"));
    }

    #[test]
    fn test_migrate_pi_credentials_mixed_providers() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let auth_path = temp_dir.path().join("auth.json");

        // Create Pi auth file with mixed LLM and non-LLM providers
        let mut file = File::create(&auth_path).expect("Failed to create auth file");
        let auth_content = r#"{
            "anthropic": {"type": "api_key", "key": "sk-test-placeholder-1"},
            "github": {"type": "token", "key": "ghp-test-placeholder"},
            "openai": {"type": "api_key", "key": "sk-test-placeholder-2"}
        }"#;
        file.write_all(auth_content.as_bytes())
            .expect("Failed to write");

        let existing = HashMap::new();
        let result = migrate_pi_credentials(&existing, Some(auth_path));

        // Should migrate all 3 providers
        assert!(result.migrated_llm);
        assert_eq!(result.count, 3);
        assert_eq!(result.providers.len(), 3);
    }
}
