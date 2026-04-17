//! Orchestra Wizard — Load stored API keys into environment.
//!
//! Hydrates environment variables from stored auth.json credentials for optional
//! tool keys. Runs on every launch so extensions see Brave/Context7/Jina keys
//! stored via the wizard on prior launches.
//!
//! Matches orchestra-2's wizard.ts implementation.

use serde_json::Value;
use std::collections::HashMap;
use std::env;

/// Provider to environment variable mapping
///
/// Maps provider IDs to their corresponding environment variable names
const PROVIDER_ENV_MAPPING: &[(&str, &str)] = &[
    ("brave", "BRAVE_API_KEY"),
    ("brave_answers", "BRAVE_ANSWERS_KEY"),
    ("context7", "CONTEXT7_API_KEY"),
    ("jina", "JINA_API_KEY"),
    ("tavily", "TAVILY_API_KEY"),
    ("slack_bot", "SLACK_BOT_TOKEN"),
    ("discord_bot", "DISCORD_BOT_TOKEN"),
    ("telegram_bot", "TELEGRAM_BOT_TOKEN"),
    ("groq", "GROQ_API_KEY"),
    ("ollama-cloud", "OLLAMA_API_KEY"),
    ("custom-openai", "CUSTOM_OPENAI_API_KEY"),
    ("kimi-cn", "KIMI_CN_API_KEY"),
    ("kimi-global", "KIMI_GLOBAL_API_KEY"),
    ("alibaba-cn", "ALIBABA_CN_API_KEY"),
    ("alibaba-global", "ALIBABA_GLOBAL_API_KEY"),
    ("vertex", "VERTEX_ACCESS_TOKEN"),
    ("vertex-sa", "VERTEX_SERVICE_ACCOUNT_KEY"),
];

/// Auth storage entry representing a stored credential
#[derive(Debug, Clone)]
pub struct AuthEntry {
    /// Type of credential (e.g., "api_key", "oauth_token")
    pub cred_type: String,
    /// The actual key/token value
    pub key: Option<String>,
}

/// Load stored API keys from auth storage into environment variables
///
/// This function hydrates `process.env` from stored auth.json credentials
/// for optional tool keys. It runs on every launch so extensions see
/// Brave/Context7/Jina keys stored via the wizard on prior launches.
///
/// # Arguments
/// * `auth_storage` - HashMap of provider ID to AuthEntry
///
/// # Behavior
/// - Only sets env vars that aren't already set (doesn't override existing)
/// - Only sets env vars for credentials with type "api_key"
/// - Skips credentials with missing or empty keys
///
/// # Examples
/// ```
/// use rustycode_orchestra::wizard::{load_stored_env_keys, AuthEntry};
/// use std::collections::HashMap;
///
/// let mut auth_storage = HashMap::new();
/// auth_storage.insert("brave".to_string(), AuthEntry {
///     cred_type: "api_key".to_string(),
///     key: Some("sk-test-placeholder".to_string()),
/// });
///
/// load_stored_env_keys(&auth_storage);
///
/// assert_eq!(std::env::var("BRAVE_API_KEY"), Ok("sk-test-placeholder".to_string()));
/// ```
pub fn load_stored_env_keys(auth_storage: &HashMap<String, AuthEntry>) {
    for (provider, env_var) in PROVIDER_ENV_MAPPING {
        // Skip if env var is already set (don't override)
        if env::var(env_var).is_ok() {
            continue;
        }

        // Try to get credential from storage
        if let Some(entry) = auth_storage.get(*provider) {
            // Only set for api_key type with a non-empty key
            if entry.cred_type == "api_key" {
                if let Some(ref key) = entry.key {
                    if !key.is_empty() {
                        env::set_var(env_var, key);
                    }
                }
            }
        }
    }
}

/// Load stored API keys from JSON auth storage
///
/// Convenience function that parses JSON auth storage and loads keys.
///
/// # Arguments
/// * `auth_json` - JSON value representing auth storage
///
/// # Examples
/// ```
/// use rustycode_orchestra::wizard::load_stored_env_keys_from_json;
/// use serde_json::json;
///
/// let auth_json = json!({
///     "brave": {
///         "type": "api_key",
///         "key": "sk-test-placeholder"
///     }
/// });
///
/// load_stored_env_keys_from_json(&auth_json);
/// ```
pub fn load_stored_env_keys_from_json(auth_json: &Value) {
    let mut auth_storage = HashMap::new();

    if let Some(obj) = auth_json.as_object() {
        for (provider, value) in obj {
            if let Some(entry_obj) = value.as_object() {
                let cred_type = entry_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");

                let key = entry_obj
                    .get("key")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                auth_storage.insert(
                    provider.clone(),
                    AuthEntry {
                        cred_type: cred_type.to_string(),
                        key,
                    },
                );
            }
        }
    }

    load_stored_env_keys(&auth_storage);
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_load_stored_env_keys_basic() {
        let _guard = test_lock().lock().unwrap();
        // Clear all relevant env vars first
        for (_, env_var) in PROVIDER_ENV_MAPPING {
            env::remove_var(env_var);
        }

        let mut auth_storage = HashMap::new();
        auth_storage.insert(
            "brave".to_string(),
            AuthEntry {
                cred_type: "api_key".to_string(),
                key: Some("sk-test-brave-placeholder".to_string()),
            },
        );

        load_stored_env_keys(&auth_storage);

        assert_eq!(
            env::var("BRAVE_API_KEY"),
            Ok("sk-test-brave-placeholder".to_string())
        );

        // Cleanup
        env::remove_var("BRAVE_API_KEY");
    }

    #[test]
    fn test_load_stored_env_keys_doesnt_override() {
        let _guard = test_lock().lock().unwrap();
        for (_, env_var) in PROVIDER_ENV_MAPPING {
            env::remove_var(env_var);
        }

        let mut auth_storage = HashMap::new();
        auth_storage.insert(
            "context7".to_string(),
            AuthEntry {
                cred_type: "api_key".to_string(),
                key: Some("sk-from-storage".to_string()),
            },
        );

        // Set env var first
        env::set_var("CONTEXT7_API_KEY", "sk-already-set");

        load_stored_env_keys(&auth_storage);

        // Should keep existing value
        assert_eq!(
            env::var("CONTEXT7_API_KEY"),
            Ok("sk-already-set".to_string())
        );

        // Cleanup
        env::remove_var("CONTEXT7_API_KEY");
    }

    #[test]
    fn test_load_stored_env_keys_only_api_key_type() {
        let _guard = test_lock().lock().unwrap();
        let mut auth_storage = HashMap::new();
        auth_storage.insert(
            "jina".to_string(),
            AuthEntry {
                cred_type: "oauth_token".to_string(), // Wrong type
                key: Some("sk-test-placeholder".to_string()),
            },
        );

        env::remove_var("JINA_API_KEY");

        load_stored_env_keys(&auth_storage);

        // Should not set env var for non-api_key type
        assert_eq!(env::var("JINA_API_KEY"), Err(env::VarError::NotPresent));
    }

    #[test]
    fn test_load_stored_env_keys_empty_key() {
        let _guard = test_lock().lock().unwrap();
        let mut auth_storage = HashMap::new();
        auth_storage.insert(
            "tavily".to_string(),
            AuthEntry {
                cred_type: "api_key".to_string(),
                key: Some("".to_string()), // Empty key
            },
        );

        env::remove_var("TAVILY_API_KEY");

        load_stored_env_keys(&auth_storage);

        // Should not set env var for empty key
        assert_eq!(env::var("TAVILY_API_KEY"), Err(env::VarError::NotPresent));
    }

    #[test]
    fn test_load_stored_env_keys_none_key() {
        let _guard = test_lock().lock().unwrap();
        let mut auth_storage = HashMap::new();
        auth_storage.insert(
            "groq".to_string(),
            AuthEntry {
                cred_type: "api_key".to_string(),
                key: None, // No key
            },
        );

        env::remove_var("GROQ_API_KEY");

        load_stored_env_keys(&auth_storage);

        // Should not set env var for None key
        assert_eq!(env::var("GROQ_API_KEY"), Err(env::VarError::NotPresent));
    }

    #[test]
    fn test_load_stored_env_keys_multiple_providers() {
        let _guard = test_lock().lock().unwrap();
        // Clear all relevant env vars first
        for (_, env_var) in PROVIDER_ENV_MAPPING {
            env::remove_var(env_var);
        }

        let mut auth_storage = HashMap::new();
        auth_storage.insert(
            "brave".to_string(),
            AuthEntry {
                cred_type: "api_key".to_string(),
                key: Some("sk-brave-123".to_string()),
            },
        );
        auth_storage.insert(
            "jina".to_string(),
            AuthEntry {
                cred_type: "api_key".to_string(),
                key: Some("sk-jina-456".to_string()),
            },
        );
        auth_storage.insert(
            "context7".to_string(),
            AuthEntry {
                cred_type: "api_key".to_string(),
                key: Some("sk-context7-789".to_string()),
            },
        );

        load_stored_env_keys(&auth_storage);

        assert_eq!(env::var("BRAVE_API_KEY"), Ok("sk-brave-123".to_string()));
        assert_eq!(env::var("JINA_API_KEY"), Ok("sk-jina-456".to_string()));
        assert_eq!(
            env::var("CONTEXT7_API_KEY"),
            Ok("sk-context7-789".to_string())
        );

        // Cleanup
        env::remove_var("BRAVE_API_KEY");
        env::remove_var("JINA_API_KEY");
        env::remove_var("CONTEXT7_API_KEY");
    }

    #[test]
    fn test_load_stored_env_keys_from_json() {
        let _guard = test_lock().lock().unwrap();
        // Clear all relevant env vars first
        for (_, env_var) in PROVIDER_ENV_MAPPING {
            env::remove_var(env_var);
        }

        let auth_json = serde_json::json!({
            "brave": {
                "type": "api_key",
                "key": "sk-brave-json"
            },
            "jina": {
                "type": "api_key",
                "key": "sk-jina-json"
            }
        });

        load_stored_env_keys_from_json(&auth_json);

        assert_eq!(env::var("BRAVE_API_KEY"), Ok("sk-brave-json".to_string()));
        assert_eq!(env::var("JINA_API_KEY"), Ok("sk-jina-json".to_string()));

        // Cleanup
        env::remove_var("BRAVE_API_KEY");
        env::remove_var("JINA_API_KEY");
    }

    #[test]
    fn test_load_stored_env_keys_unknown_provider() {
        let _guard = test_lock().lock().unwrap();
        let mut auth_storage = HashMap::new();
        auth_storage.insert(
            "unknown_provider".to_string(),
            AuthEntry {
                cred_type: "api_key".to_string(),
                key: Some("sk-test-placeholder".to_string()),
            },
        );

        load_stored_env_keys(&auth_storage);

        // Unknown provider should be ignored (no env var set)
        // This test just verifies it doesn't panic
    }

    #[test]
    fn test_all_provider_mappings() {
        let _guard = test_lock().lock().unwrap();
        // Verify all providers have mappings
        assert_eq!(PROVIDER_ENV_MAPPING.len(), 17);

        // Check a few known mappings
        assert!(PROVIDER_ENV_MAPPING.contains(&("brave", "BRAVE_API_KEY")));
        assert!(PROVIDER_ENV_MAPPING.contains(&("context7", "CONTEXT7_API_KEY")));
        assert!(PROVIDER_ENV_MAPPING.contains(&("jina", "JINA_API_KEY")));
        assert!(PROVIDER_ENV_MAPPING.contains(&("tavily", "TAVILY_API_KEY")));
        assert!(PROVIDER_ENV_MAPPING.contains(&("kimi-cn", "KIMI_CN_API_KEY")));
        assert!(PROVIDER_ENV_MAPPING.contains(&("kimi-global", "KIMI_GLOBAL_API_KEY")));
        assert!(PROVIDER_ENV_MAPPING.contains(&("alibaba-cn", "ALIBABA_CN_API_KEY")));
        assert!(PROVIDER_ENV_MAPPING.contains(&("alibaba-global", "ALIBABA_GLOBAL_API_KEY")));
        assert!(PROVIDER_ENV_MAPPING.contains(&("vertex", "VERTEX_ACCESS_TOKEN")));
        assert!(PROVIDER_ENV_MAPPING.contains(&("vertex-sa", "VERTEX_SERVICE_ACCOUNT_KEY")));
    }
}
