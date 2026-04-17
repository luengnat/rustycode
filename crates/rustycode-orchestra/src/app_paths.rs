//! Orchestra Application Paths — Standard paths for Orchestra application directories.
//!
//! Provides centralized path definitions for Orchestra application directories
//! and configuration files in the user's home directory.
//!
//! Matches orchestra-2's app-paths.ts implementation.

use std::path::PathBuf;

/// Get the Orchestra application root directory
///
/// Returns `~/.orchestra` (user's home directory + .orchestra)
///
/// # Examples
/// ```
/// use rustycode_orchestra::app_paths::app_root;
///
/// let root = app_root();
/// assert!(root.ends_with(".orchestra"));
/// ```
pub fn app_root() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".orchestra"))
        .unwrap_or_else(|| PathBuf::from("~/.orchestra"))
}

/// Get the Orchestra agent directory
///
/// Returns `~/.orchestra/agent`
///
/// # Examples
/// ```
/// use rustycode_orchestra::app_paths::agent_dir;
///
/// let agent = agent_dir();
/// assert!(agent.ends_with(".orchestra/agent"));
/// ```
pub fn agent_dir() -> PathBuf {
    app_root().join("agent")
}

/// Get the Orchestra sessions directory
///
/// Returns `~/.orchestra/sessions`
///
/// # Examples
/// ```
/// use rustycode_orchestra::app_paths::sessions_dir;
///
/// let sessions = sessions_dir();
/// assert!(sessions.ends_with(".orchestra/sessions"));
/// ```
pub fn sessions_dir() -> PathBuf {
    app_root().join("sessions")
}

/// Get the Orchestra authentication file path
///
/// Returns `~/.orchestra/agent/auth.json`
///
/// # Examples
/// ```
/// use rustycode_orchestra::app_paths::auth_file_path;
///
/// let auth = auth_file_path();
/// assert!(auth.ends_with(".orchestra/agent/auth.json"));
/// ```
pub fn auth_file_path() -> PathBuf {
    agent_dir().join("auth.json")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_root() {
        let root = app_root();
        assert!(root.ends_with(".orchestra"));
    }

    #[test]
    fn test_agent_dir() {
        let agent = agent_dir();
        assert!(agent.ends_with(".orchestra/agent") || agent.ends_with(".orchestra\\agent"));
    }

    #[test]
    fn test_sessions_dir() {
        let sessions = sessions_dir();
        assert!(
            sessions.ends_with(".orchestra/sessions") || sessions.ends_with(".orchestra\\sessions")
        );
    }

    #[test]
    fn test_auth_file_path() {
        let auth = auth_file_path();
        assert!(
            auth.ends_with(".orchestra/agent/auth.json")
                || auth.ends_with(".orchestra\\agent\\auth.json")
        );
    }

    #[test]
    fn test_path_hierarchy() {
        // Verify that agent_dir is a child of app_root
        let root = app_root();
        let agent = agent_dir();
        assert!(agent.starts_with(&root));

        // Verify that sessions_dir is a child of app_root
        let sessions = sessions_dir();
        assert!(sessions.starts_with(&root));

        // Verify that auth_file_path is a child of agent_dir
        let auth = auth_file_path();
        assert!(auth.starts_with(&agent));
    }

    #[test]
    fn test_path_consistency() {
        // Multiple calls should return the same path
        let root1 = app_root();
        let root2 = app_root();
        assert_eq!(root1, root2);

        let agent1 = agent_dir();
        let agent2 = agent_dir();
        assert_eq!(agent1, agent2);

        let sessions1 = sessions_dir();
        let sessions2 = sessions_dir();
        assert_eq!(sessions1, sessions2);

        let auth1 = auth_file_path();
        let auth2 = auth_file_path();
        assert_eq!(auth1, auth2);
    }
}
