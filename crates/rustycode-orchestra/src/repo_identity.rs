// rustycode-orchestra/src/repo_identity.rs
//! Orchestra Repo Identity — external state directory primitives.
//!
//! Computes a stable per-repo identity hash, resolves the external
//! `~/.orchestra/projects/<hash>/` state directory, and manages the
//! `<project>/.orchestra → external` symlink.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use sha2::{Digest, Sha256};

/// Default Orchestra state directory name.
pub const DEFAULT_ORCHESTRA_DIR: &str = ".orchestra";

/// Projects subdirectory name.
pub const PROJECTS_SUBDIR: &str = "projects";

/// Environment variable for custom Orchestra state directory.
pub const ORCHESTRA_STATE_DIR_ENV: &str = "Orchestra_STATE_DIR";

/// Get the git remote URL for "origin", or empty string if no remote is configured.
///
/// Uses `git config` rather than `git remote get-url` for broader compat.
///
/// # Arguments
/// * `base_path` - Path to the git repository
///
/// # Returns
/// Remote URL or empty string if not configured
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::repo_identity::get_remote_url;
/// use std::path::Path;
///
/// let url = get_remote_url(Path::new("/project"));
/// // Returns: "https://github.com/user/repo.git" or ""
/// ```
pub fn get_remote_url(base_path: &Path) -> String {
    let output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(base_path)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    }
}

/// Resolve the git toplevel (real root) for the given path.
///
/// For worktrees this returns the main repo root, not the worktree path.
///
/// # Arguments
/// * `base_path` - Path to resolve
///
/// # Returns
/// Git toplevel path or input path if not a git repo
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::repo_identity::resolve_git_root;
/// use std::path::Path;
///
/// let root = resolve_git_root(Path::new("/project/subdir"));
/// // Returns: "/project" or "/project/subdir" if not in git
/// ```
pub fn resolve_git_root(base_path: &Path) -> PathBuf {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(base_path)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            PathBuf::from(path)
        }
        _ => base_path.to_path_buf(),
    }
}

/// Compute a stable identity for a repository.
///
/// SHA-256 of `${remoteUrl}\n${resolvedRoot}`, truncated to 12 hex chars.
/// Deterministic: same repo always produces the same hash regardless of
/// which worktree the caller is inside.
///
/// # Arguments
/// * `base_path` - Path to the git repository
///
/// # Returns
/// 12-character hex string identifying the repository
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::repo_identity::repo_identity;
/// use std::path::Path;
///
/// let identity = repo_identity(Path::new("/project"));
/// assert_eq!(identity.len(), 12);
/// ```
pub fn repo_identity(base_path: &Path) -> String {
    let remote_url = get_remote_url(base_path);
    let root = resolve_git_root(base_path);
    let input = format!("{}\n{}", remote_url, root.display());

    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();

    // Truncate to 12 hex chars
    format!("{:x}", result)[..12].to_string()
}

/// Compute the external Orchestra state directory for a repository.
///
/// Returns `$Orchestra_STATE_DIR/projects/<hash>` if `Orchestra_STATE_DIR` is set,
/// otherwise `~/.orchestra/projects/<hash>`.
///
/// # Arguments
/// * `base_path` - Path to the git repository
///
/// # Returns
/// Path to external Orchestra state directory
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::repo_identity::external_orchestra_root;
/// use std::path::Path;
///
/// let external = external_orchestra_root(Path::new("/project"));
/// // Returns: "~/.orchestra/projects/<hash>" or custom Orchestra_STATE_DIR
/// ```
pub fn external_orchestra_root(base_path: &Path) -> PathBuf {
    let base = if let Ok(state_dir) = std::env::var(ORCHESTRA_STATE_DIR_ENV) {
        PathBuf::from(state_dir)
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(DEFAULT_ORCHESTRA_DIR)
    };

    base.join(PROJECTS_SUBDIR).join(repo_identity(base_path))
}

/// Ensure the `<project>/.orchestra` symlink points to the external state directory.
///
/// Algorithm:
/// 1. mkdir -p the external dir
/// 2. If `<project>/.orchestra` doesn't exist → create symlink
/// 3. If `<project>/.orchestra` is already the correct symlink → no-op
/// 4. If `<project>/.orchestra` is a real directory → return as-is (migration handles later)
///
/// # Arguments
/// * `project_path` - Path to the project root
///
/// # Returns
/// The resolved path (external or local)
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::repo_identity::ensure_orchestra_symlink;
/// use std::path::Path;
///
/// let path = ensure_orchestra_symlink(Path::new("/project"));
/// ```
pub fn ensure_orchestra_symlink(project_path: &Path) -> PathBuf {
    let external_path = external_orchestra_root(project_path);
    let local_orchestra = project_path.join(DEFAULT_ORCHESTRA_DIR);

    // Ensure external directory exists
    fs::create_dir_all(&external_path).unwrap_or_else(|e| {
        tracing::warn!("Failed to create external Orchestra directory: {}", e);
    });

    if !local_orchestra.exists() {
        // Nothing exists yet — create symlink
        create_symlink(&external_path, &local_orchestra);
        return external_path;
    }

    // Check what already exists
    let metadata = match fs::symlink_metadata(&local_orchestra) {
        Ok(meta) => meta,
        Err(_) => return local_orchestra,
    };

    if metadata.file_type().is_symlink() {
        // Already a symlink — verify it points to the right place
        if let Ok(target) = fs::canonicalize(&local_orchestra) {
            if target == external_path {
                return external_path; // correct symlink, no-op
            }
            // Symlink exists but points elsewhere — leave it for now
            return target;
        }
        return local_orchestra;
    }

    if metadata.is_dir() {
        // Real directory — migration will handle this later.
        // Return the local path so existing code still works.
        return local_orchestra;
    }

    local_orchestra
}

/// Create a symlink from target to link.
///
/// Platform-specific implementation.
fn create_symlink(target: &Path, link: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        if let Err(e) = symlink(target, link) {
            tracing::warn!("Failed to create symlink {:?} -> {:?}: {}", link, target, e);
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_dir;
        if let Err(e) = symlink_dir(target, link) {
            tracing::warn!("Failed to create symlink {:?} -> {:?}: {}", link, target, e);
        }
    }
}

/// Check if the given directory is a git worktree (not the main repo).
///
/// Git worktrees have a `.git` *file* (not directory) containing a
/// `gitdir:` pointer. This is git's native worktree indicator — no
/// string marker parsing needed.
///
/// # Arguments
/// * `cwd` - Path to check
///
/// # Returns
/// true if inside a worktree, false otherwise
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::repo_identity::is_inside_worktree;
/// use std::path::Path;
///
/// let is_worktree = is_inside_worktree(Path::new("/project"));
/// ```
pub fn is_inside_worktree(cwd: &Path) -> bool {
    let git_path = cwd.join(".git");

    let metadata = match fs::symlink_metadata(&git_path) {
        Ok(meta) => meta,
        Err(_) => return false,
    };

    if !metadata.is_file() {
        return false;
    }

    match fs::read_to_string(&git_path) {
        Ok(content) => {
            let trimmed = content.trim();
            trimmed.starts_with("gitdir:")
        }
        Err(_) => false,
    }
}

/// Get the Orchestra root directory for a project.
///
/// This is a convenience function that returns the .orchestra directory path,
/// which may be a symlink to external storage.
///
/// # Arguments
/// * `base_path` - Path to the project root
///
/// # Returns
/// Path to the .orchestra directory
pub fn orchestra_root(base_path: &Path) -> PathBuf {
    base_path.join(DEFAULT_ORCHESTRA_DIR)
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoIdentityInfo {
    pub identity: String,
    pub remote_url: String,
    pub git_root: PathBuf,
    pub external_path: PathBuf,
}

/// Get complete repo identity information.
///
/// # Arguments
/// * `base_path` - Path to the git repository
///
/// # Returns
/// RepoIdentityInfo with all computed values
pub fn get_repo_identity_info(base_path: &Path) -> RepoIdentityInfo {
    let remote_url = get_remote_url(base_path);
    let git_root = resolve_git_root(base_path);
    let identity = repo_identity(base_path);
    let external_path = external_orchestra_root(base_path);

    RepoIdentityInfo {
        identity,
        remote_url,
        git_root,
        external_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_get_remote_url_no_repo() {
        let temp_dir = TempDir::new().unwrap();
        let url = get_remote_url(temp_dir.path());

        // Not a git repo, should return empty string
        assert_eq!(url, "");
    }

    #[test]
    fn test_resolve_git_root_no_repo() {
        let temp_dir = TempDir::new().unwrap();
        let root = resolve_git_root(temp_dir.path());

        // Not a git repo, should return input path
        assert_eq!(root, temp_dir.path());
    }

    #[test]
    fn test_repo_identity_consistent() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Same path should produce same identity
        let id1 = repo_identity(path);
        let id2 = repo_identity(path);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_repo_identity_length() {
        let temp_dir = TempDir::new().unwrap();
        let identity = repo_identity(temp_dir.path());

        // Should always be 12 characters
        assert_eq!(identity.len(), 12);
    }

    #[test]
    fn test_repo_identity_hex_chars() {
        let temp_dir = TempDir::new().unwrap();
        let identity = repo_identity(temp_dir.path());

        // Should be valid hex characters
        assert!(identity.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_external_orchestra_root_creates_path() {
        let temp_dir = TempDir::new().unwrap();
        let external = external_orchestra_root(temp_dir.path());

        // Path should include "projects" directory
        let external_str = external.to_string_lossy();
        assert!(
            external_str.contains("projects"),
            "External path should contain 'projects': {}",
            external_str
        );
    }

    #[test]
    fn test_ensure_orchestra_symlink_creates_external_dir() {
        let temp_dir = TempDir::new().unwrap();
        ensure_orchestra_symlink(temp_dir.path());

        let external = external_orchestra_root(temp_dir.path());
        assert!(external.exists());
    }

    #[test]
    fn test_orchestra_root_path() {
        let temp_dir = TempDir::new().unwrap();
        let root = orchestra_root(temp_dir.path());

        assert!(root.ends_with(".orchestra"));
    }

    #[test]
    fn test_is_inside_worktree_no_git() {
        let temp_dir = TempDir::new().unwrap();
        let result = is_inside_worktree(temp_dir.path());

        assert!(!result);
    }

    #[test]
    fn test_is_inside_worktree_directory() {
        let temp_dir = TempDir::new().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).unwrap();

        let result = is_inside_worktree(temp_dir.path());
        assert!(!result); // .git is a directory, not a file
    }

    #[test]
    fn test_is_inside_worktree_file_no_gitdir() {
        let temp_dir = TempDir::new().unwrap();
        let git_file = temp_dir.path().join(".git");
        fs::write(&git_file, "not a gitdir").unwrap();

        let result = is_inside_worktree(temp_dir.path());
        assert!(!result); // File doesn't start with "gitdir:"
    }

    #[test]
    fn test_is_inside_worktree_file_with_gitdir() {
        let temp_dir = TempDir::new().unwrap();
        let git_file = temp_dir.path().join(".git");
        fs::write(&git_file, "gitdir: /path/to/git").unwrap();

        let result = is_inside_worktree(temp_dir.path());
        assert!(result); // File starts with "gitdir:"
    }

    #[test]
    fn test_get_repo_identity_info() {
        let temp_dir = TempDir::new().unwrap();
        let info = get_repo_identity_info(temp_dir.path());

        // Should have valid structure
        assert_eq!(info.identity.len(), 12);
        assert!(info.identity.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_default_orchestra_dir_const() {
        assert_eq!(DEFAULT_ORCHESTRA_DIR, ".orchestra");
    }

    #[test]
    fn test_projects_subdir_const() {
        assert_eq!(PROJECTS_SUBDIR, "projects");
    }

    #[test]
    fn test_orchestra_state_dir_env_const() {
        assert_eq!(ORCHESTRA_STATE_DIR_ENV, "Orchestra_STATE_DIR");
    }

    #[test]
    fn test_repo_identity_different_paths() {
        let temp_dir = TempDir::new().unwrap();
        let path1 = temp_dir.path().join("project1");
        let path2 = temp_dir.path().join("project2");

        let id1 = repo_identity(&path1);
        let id2 = repo_identity(&path2);

        // Different paths should produce different identities
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_external_orchestra_root_include_identity() {
        let temp_dir = TempDir::new().unwrap();
        let external = external_orchestra_root(temp_dir.path());
        let identity = repo_identity(temp_dir.path());

        // External path should include the identity hash
        let external_str = external.to_string_lossy();
        assert!(external_str.contains(&identity));
    }

    #[test]
    fn test_ensure_orchestra_symlink_no_op_when_exists() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_dir = temp_dir.path().join(".orchestra");
        fs::create_dir(&orchestra_dir).unwrap();

        // Should not panic when directory exists
        let result = ensure_orchestra_symlink(temp_dir.path());
        assert!(result.exists());
    }

    #[test]
    fn test_is_inside_worktree_trailing_whitespace() {
        let temp_dir = TempDir::new().unwrap();
        let git_file = temp_dir.path().join(".git");
        fs::write(&git_file, "gitdir: /path/to/git\n\n").unwrap();

        let result = is_inside_worktree(temp_dir.path());
        assert!(result); // Should handle trailing whitespace
    }
}
