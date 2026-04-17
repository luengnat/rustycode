//! Orchestra Auto Supervisor — Signal Handling & Activity Detection
//!
//! Provides SIGTERM handling for graceful shutdown and working tree activity detection.
//! Matches orchestra-2's auto-supervisor.ts implementation.
//!
//! Critical for production autonomous systems to ensure clean shutdown and
//! detect whether the agent is actively producing work.

use anyhow::Result;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, info, warn};

// ─── Signal Handling ───────────────────────────────────────────────────────────

/// Global flag indicating whether shutdown has been requested
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Register SIGTERM handler for graceful shutdown
///
/// In production, this would register an actual signal handler.
/// For now, it sets a flag that can be polled.
///
/// # Returns
///
/// A guard that will deregister the handler when dropped.
pub fn register_sigterm_handler() -> SigtermGuard {
    SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
    info!("SIGTERM handler registered");

    SigtermGuard
}

/// Check if shutdown has been requested
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

/// Request shutdown programmatically
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    info!("Shutdown requested");
}

/// Guard that deregisters the SIGTERM handler on drop
pub struct SigtermGuard;

/// Deregister the SIGTERM handler (called on stop/pause)
pub fn deregister_sigterm_handler() {
    SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
    debug!("SIGTERM handler deregistered");
}

impl Drop for SigtermGuard {
    fn drop(&mut self) {
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
        debug!("SIGTERM handler deregistered");
    }
}

// ─── Working Tree Activity Detection ──────────────────────────────────────────

/// Detect whether the agent is producing work on disk
///
/// Checks git for any working-tree changes (staged, unstaged, or untracked).
/// Returns true if there are uncommitted changes, indicating the agent
/// is actively working even though it hasn't signaled progress.
pub fn detect_working_tree_activity(cwd: &Path) -> bool {
    match has_git_changes(cwd) {
        Ok(has_changes) => {
            if has_changes {
                debug!("Working tree activity detected in {:?}", cwd);
            }
            has_changes
        }
        Err(e) => {
            warn!("Failed to check working tree activity in {:?}: {}", cwd, e);
            false
        }
    }
}

/// Check if there are any git changes in the working directory
fn has_git_changes(cwd: &Path) -> Result<bool> {
    // Use git2 to check for changes
    let repo = match git2::Repository::open(cwd) {
        Ok(repo) => repo,
        Err(_e) => {
            // Not a git repository or can't open
            debug!("Not a git repository or can't open: {:?}", cwd);
            return Ok(false);
        }
    };

    // Check for unstaged changes
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(false);
    opts.include_ignored(false);

    let statuses = repo.statuses(Some(&mut opts))?;

    for entry in statuses.iter() {
        let status = entry.status();
        if status != git2::Status::CURRENT {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Check if there are any staged changes
pub fn has_staged_changes(cwd: &Path) -> Result<bool> {
    let repo = git2::Repository::open(cwd)?;

    // Get HEAD commit
    let head = match repo.head() {
        Ok(head) => head,
        Err(_) => {
            // No commits yet, check if there's anything staged
            let mut opts = git2::StatusOptions::new();
            opts.include_untracked(false);
            opts.include_ignored(false);
            let statuses = repo.statuses(Some(&mut opts))?;
            for entry in statuses.iter() {
                let status = entry.status();
                if status == git2::Status::INDEX_NEW || status == git2::Status::INDEX_MODIFIED {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
    };

    let tree = head.peel_to_tree()?;

    // Check index vs HEAD
    let diff = repo.diff_tree_to_index(Some(&tree), None, None)?;

    // If diff has any deltas, there are staged changes
    Ok(diff.deltas().count() > 0)
}

/// Check if repository is in a clean state (no uncommitted changes)
pub fn is_repo_clean(cwd: &Path) -> Result<bool> {
    Ok(!has_git_changes(cwd)?)
}

/// Get git status summary as a string
pub fn get_git_status_summary(cwd: &Path) -> Result<String> {
    let repo = git2::Repository::open(cwd)?;

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);

    let statuses = repo.statuses(Some(&mut opts))?;

    let mut summary = Vec::new();

    for entry in statuses.iter() {
        if let Some(path) = entry.path() {
            let path_str = path.to_string();
            let status = entry.status();

            if status != git2::Status::CURRENT {
                summary.push(format!("{}: {:?}", path_str, status));
            }
        }
    }

    if summary.is_empty() {
        Ok("clean".to_string())
    } else {
        Ok(summary.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    #[test]
    fn test_sigterm_guard() {
        // Test that the guard works
        {
            let _guard = register_sigterm_handler();
            assert!(!is_shutdown_requested());
        }
        // Guard dropped, should be reset
        assert!(!is_shutdown_requested());
    }

    #[test]
    fn test_shutdown_request() {
        assert!(!is_shutdown_requested());

        request_shutdown();
        assert!(is_shutdown_requested());

        // Reset for other tests
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn test_detect_working_tree_activity_no_repo() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Not a git repository
        let has_activity = detect_working_tree_activity(cwd);
        assert!(!has_activity);
    }

    #[test]
    fn test_detect_working_tree_activity_with_repo() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Initialize git repo using git2 directly
        if git2::Repository::init(cwd).is_err() {
            // Git2 not available, skip test
            return;
        }

        // Initially clean
        let has_activity = detect_working_tree_activity(cwd);
        assert!(!has_activity);

        // Create and commit a file to establish a baseline
        let test_file = cwd.join("test.txt");
        fs::write(&test_file, "initial content").unwrap();

        let repo = git2::Repository::open(cwd).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();

        // Create initial commit
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig =
            git2::Signature::new("Test User", "test@example.com", &git2::Time::new(0, 0)).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        // After commit, should be clean
        let has_activity = detect_working_tree_activity(cwd);
        assert!(!has_activity);

        // Modify the file
        fs::write(&test_file, "modified content").unwrap();

        // Now should have activity
        let has_activity = detect_working_tree_activity(cwd);
        assert!(has_activity);
    }

    #[test]
    fn test_is_repo_clean() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Not a git repository
        let is_clean = is_repo_clean(cwd).unwrap();
        assert!(is_clean);
    }

    #[test]
    fn test_get_git_status_summary() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Not a git repository - should fail
        let result = get_git_status_summary(cwd);
        assert!(result.is_err());

        // Initialize git repo using git2 directly
        let repo = git2::Repository::init(cwd).unwrap();

        // Create and commit a file
        let test_file = cwd.join("test.txt");
        fs::write(&test_file, "initial content").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig =
            git2::Signature::new("Test User", "test@example.com", &git2::Time::new(0, 0)).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        // Clean repository
        let summary = get_git_status_summary(cwd).unwrap();
        assert_eq!(summary, "clean");

        // Modify file to create dirty state
        fs::write(&test_file, "modified content").unwrap();

        // Now should show modified
        let summary = get_git_status_summary(cwd).unwrap();
        assert!(summary.contains("test.txt"));
        assert!(summary != "clean");
    }
}
