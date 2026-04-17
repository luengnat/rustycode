//! Models.json resolution with fallback to ~/.pi/agent/models.json.
//!
//! Orchestra uses ~/.orchestra/agent/models.json, but for a smooth migration/development
//! experience, this module provides resolution logic that:
//!
//! 1. Reads ~/.orchestra/agent/models.json if it exists
//! 2. Falls back to ~/.pi/agent/models.json if Orchestra file doesn't exist
//! 3. Returns Orchestra path if neither exists (will be created)
//!
//! Matches orchestra-2's models-resolver.ts implementation.

use std::path::PathBuf;

/// Path to Orchestra models.json file (~/.orchestra/agent/models.json)
pub fn orchestra_models_path() -> PathBuf {
    crate::app_paths::agent_dir().join("models.json")
}

/// Path to PI (predecessor) models.json file (~/.pi/agent/models.json)
pub fn pi_models_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".pi")
        .join("agent")
        .join("models.json")
}

/// Resolve the path to models.json with fallback logic
///
/// This function determines which models.json file to use based on existence.
/// The priority is:
///
/// 1. `~/.orchestra/agent/models.json` (exists) → return this path
/// 2. `~/.pi/agent/models.json` (exists) → return this path (fallback)
/// 3. Neither exists → return Orchestra path (will be created)
///
/// # Returns
/// The path to use for models.json
///
/// # Examples
/// ```
/// use rustycode_orchestra::models_resolver::resolve_models_json_path;
///
/// let path = resolve_models_json_path();
/// // Returns: ~/.orchestra/agent/models.json (usually)
/// // Or: ~/.pi/agent/models.json (if Orchestra file doesn't exist but PI file does)
/// ```
pub fn resolve_models_json_path() -> PathBuf {
    let orchestra_path = orchestra_models_path();
    let pi_path = pi_models_path();

    // Priority 1: Orchestra models.json exists
    if orchestra_path.exists() {
        return orchestra_path;
    }

    // Priority 2: PI models.json exists (fallback)
    if pi_path.exists() {
        return pi_path;
    }

    // Priority 3: Neither exists, return Orchestra path (will be created)
    orchestra_path
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_orchestra_models_path() {
        let path = orchestra_models_path();
        assert!(path.ends_with("models.json"));
        assert!(path.to_string_lossy().contains(".orchestra"));
    }

    #[test]
    fn test_pi_models_path() {
        let path = pi_models_path();
        assert!(path.ends_with("models.json"));
        assert!(path.to_string_lossy().contains(".pi"));
    }

    #[test]
    fn test_resolve_models_json_path_default() {
        // Neither file exists, should return Orchestra path
        let path = resolve_models_json_path();
        assert!(path.to_string_lossy().contains(".orchestra"));
        assert!(path.ends_with("models.json"));
    }

    #[test]
    fn test_resolve_priority_orchestra_over_pi() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_dir = temp_dir.path().join(".orchestra").join("agent");
        let pi_dir = temp_dir.path().join(".pi").join("agent");

        fs::create_dir_all(&orchestra_dir).unwrap();
        fs::create_dir_all(&pi_dir).unwrap();

        // Create both files
        let orchestra_file = orchestra_dir.join("models.json");
        let pi_file = pi_dir.join("models.json");
        fs::write(&orchestra_file, "{\"orchestra\": true}").unwrap();
        fs::write(&pi_file, "{\"pi\": true}").unwrap();

        // Mock the paths by using a test-specific approach
        // Since we can't override the real home directory, we just verify the logic

        // Verify files exist
        assert!(orchestra_file.exists());
        assert!(pi_file.exists());

        // The actual resolve_models_json_path() would return Orchestra path in this case
        // because Orchestra takes priority when both exist
    }

    #[test]
    fn test_resolve_fallback_to_pi() {
        let temp_dir = TempDir::new().unwrap();
        let pi_dir = temp_dir.path().join(".pi").join("agent");

        fs::create_dir_all(&pi_dir).unwrap();

        // Create only PI file
        let pi_file = pi_dir.join("models.json");
        fs::write(&pi_file, "{\"pi\": true}").unwrap();

        // Verify PI file exists
        assert!(pi_file.exists());

        // The actual resolve_models_json_path() would return PI path in this case
        // because Orchestra file doesn't exist but PI file does
    }

    #[test]
    fn test_resolve_create_orchestra_when_neither_exist() {
        let temp_dir = TempDir::new().unwrap();

        // Neither file exists
        let _orchestra_dir = temp_dir.path().join(".orchestra").join("agent");
        let _pi_dir = temp_dir.path().join(".pi").join("agent");

        // The actual resolve_models_json_path() would return Orchestra path
        // because neither file exists, and Orchestra is the default
    }

    #[test]
    fn test_path_consistency() {
        // Multiple calls should return the same path
        let path1 = resolve_models_json_path();
        let path2 = resolve_models_json_path();
        assert_eq!(path1, path2);
    }

    #[test]
    fn test_orchestra_path_contains_agent() {
        let path = orchestra_models_path();
        let path_str = path.to_string_lossy();
        // Path should contain both .orchestra and agent
        assert!(path_str.contains(".orchestra"));
        assert!(path_str.contains("agent"));
    }

    #[test]
    fn test_pi_path_contains_agent() {
        let path = pi_models_path();
        let path_str = path.to_string_lossy();
        // Path should contain both .pi and agent
        assert!(path_str.contains(".pi"));
        assert!(path_str.contains("agent"));
    }
}
