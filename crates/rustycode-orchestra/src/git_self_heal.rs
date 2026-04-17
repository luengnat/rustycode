//! Git Self-Heal — Automated git state recovery utilities
//!
//! Four synchronous functions for recovering from broken git state
//! during auto-mode operations. Uses only `git reset --hard HEAD` —
//! never `git clean` (which would delete untracked .orchestra/ dirs).
//!
//! Observability: Each function returns structured results describing
//! what actions were taken. `format_git_error` maps raw git errors to
//! user-friendly messages suggesting `/orchestra doctor`.

use crate::error::{OrchestraV2Error, Result};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

// ─── Error Type ───────────────────────────────────────────────────────────────

/// Merge conflict error
#[derive(Debug, Clone, PartialEq)]
pub struct MergeConflictError {
    pub message: String,
}

impl std::fmt::Display for MergeConflictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MergeConflictError {}

// ─── Result Types ─────────────────────────────────────────────────────────────

/// Result from abort_and_reset describing what was cleaned up.
#[derive(Debug, Clone, Default)]
pub struct AbortAndResetResult {
    /// List of actions taken, e.g. ["aborted merge", "removed SQUASH_MSG", "reset to HEAD"]
    pub cleaned: Vec<String>,
}

// ─── Git Operations ───────────────────────────────────────────────────────────

/// Run git command and return output
fn git_cmd(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| OrchestraV2Error::Git(format!("Failed to execute git command: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let suffix = if stderr.is_empty() {
            String::new()
        } else {
            format!(": {}", stderr)
        };
        return Err(OrchestraV2Error::Git(format!(
            "Git command failed: git {}{}",
            args.join(" "),
            suffix
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run git command silently, ignoring errors
fn git_cmd_silent(cwd: &Path, args: &[&str]) {
    let _ = Command::new("git").current_dir(cwd).args(args).output();
}

/// Abort in-progress merge
fn native_merge_abort(cwd: &Path) -> Result<()> {
    git_cmd(cwd, &["merge", "--abort"])?;
    Ok(())
}

/// Abort in-progress rebase
fn native_rebase_abort(cwd: &Path) -> Result<()> {
    git_cmd(cwd, &["rebase", "--abort"])?;
    Ok(())
}

/// Hard reset to HEAD
fn native_reset_hard(cwd: &Path) -> Result<()> {
    git_cmd(cwd, &["reset", "--hard", "HEAD"])?;
    Ok(())
}

/// Unstage Orchestra runtime noise files after `git add -A`
///
/// This is used by commit hooks to prevent auto-generated files
/// from being committed while still allowing milestone/plan files.
///
/// Files excluded:
/// - .orchestra/.lock - runtime lock file
/// - .orchestra/activity.logl - append-only activity log
/// - .orchestra/STATE.md - auto-generated state cache
///
/// # Arguments
/// * `cwd` - Working directory (project root)
///
/// # Example
/// ```
/// use rustycode_orchestra::git_self_heal::unstage_orchestra_runtime_files;
/// use std::path::Path;
///
/// unstage_orchestra_runtime_files(Path::new("/project"));
/// ```
pub fn unstage_orchestra_runtime_files(cwd: &Path) {
    git_cmd_silent(
        cwd,
        &[
            "reset",
            "HEAD",
            "--",
            ".orchestra/.lock",
            ".orchestra/activity.logl",
            ".orchestra/STATE.md",
        ],
    );
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Detect and clean up leftover merge/rebase state, then hard-reset.
///
/// Checks for: .git/MERGE_HEAD, .git/SQUASH_MSG, .git/rebase-apply.
/// Aborts in-progress merge or rebase if detected. Always finishes
/// with `git reset --hard HEAD`.
///
/// # Arguments
/// * `cwd` - Current working directory (git repository root)
///
/// # Returns
/// Structured result listing what was cleaned. Empty `cleaned`
/// array means repo was already in a clean state.
///
/// # Example
/// ```
/// use rustycode_orchestra::git_self_heal::*;
///
/// let result = abort_and_reset(Path::new("/project"));
/// for action in &result.cleaned {
///     println!("Cleaned: {}", action);
/// }
/// ```
pub fn abort_and_reset(cwd: &Path) -> AbortAndResetResult {
    let git_dir = cwd.join(".git");
    let mut cleaned = Vec::new();

    // Abort in-progress merge
    let merge_head = git_dir.join("MERGE_HEAD");
    if merge_head.exists() {
        match native_merge_abort(cwd) {
            Ok(()) => cleaned.push("aborted merge".to_string()),
            Err(_) => cleaned.push("merge abort attempted (may have failed)".to_string()),
        }
    }

    // Remove leftover SQUASH_MSG (squash-merge leaves this without MERGE_HEAD)
    let squash_msg_path = git_dir.join("SQUASH_MSG");
    if squash_msg_path.exists() {
        if let Ok(()) = fs::remove_file(&squash_msg_path) {
            cleaned.push("removed SQUASH_MSG".to_string())
        } // Not critical
    }

    // Abort in-progress rebase
    let rebase_apply = git_dir.join("rebase-apply");
    let rebase_merge = git_dir.join("rebase-merge");
    if rebase_apply.exists() || rebase_merge.exists() {
        match native_rebase_abort(cwd) {
            Ok(()) => cleaned.push("aborted rebase".to_string()),
            Err(_) => cleaned.push("rebase abort attempted (may have failed)".to_string()),
        }
    }

    // Always hard-reset to HEAD
    match native_reset_hard(cwd) {
        Ok(()) => {
            if !cleaned.is_empty() {
                cleaned.push("reset to HEAD".to_string());
            }
        }
        Err(_) => cleaned.push("reset to HEAD failed".to_string()),
    }

    AbortAndResetResult { cleaned }
}

/// Known git error patterns mapped to user-friendly messages.
struct ErrorPattern {
    pattern: regex_lite::Regex,
    message: &'static str,
}

fn get_error_patterns() -> &'static [ErrorPattern] {
    static PATTERNS: OnceLock<Vec<ErrorPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            ErrorPattern {
                pattern: regex_lite::Regex::new("(?i)conflict|CONFLICT|merge conflict").unwrap(),
                message: "A merge conflict occurred. Code changes on different branches touched the same files. Run `/orchestra doctor` to diagnose.",
            },
            ErrorPattern {
                pattern: regex_lite::Regex::new("(?i)cannot checkout|did not match any|pathspec .* did not match").unwrap(),
                message: "Git could not switch branches — the target branch may not exist or the working tree is dirty. Run `/orchestra doctor` to diagnose.",
            },
            ErrorPattern {
                pattern: regex_lite::Regex::new("(?i)HEAD detached|detached HEAD").unwrap(),
                message: "Git is in a detached HEAD state — not on any branch. Run `/orchestra doctor` to diagnose and reattach.",
            },
            ErrorPattern {
                pattern: regex_lite::Regex::new("(?i)\\.lock|Unable to create .* lock|lock file").unwrap(),
                message: "A git lock file is blocking operations. Another git process may be running, or a previous one crashed. Run `/orchestra doctor` to diagnose.",
            },
            ErrorPattern {
                pattern: regex_lite::Regex::new("(?i)fatal: not a git repository").unwrap(),
                message: "This directory is not a git repository. Run `/orchestra doctor` to check your project setup.",
            },
        ]
    })
}

/// Translate raw git error strings into user-friendly messages.
///
/// Pattern-matches against common git error strings and returns
/// a non-technical message suggesting `/orchestra doctor`. Returns the
/// original message if no pattern matches.
///
/// # Arguments
/// * `error` - Error string or Error object
///
/// # Returns
/// User-friendly error message
///
/// # Example
/// ```
/// use rustycode_orchestra::git_self_heal::*;
///
/// let msg = format_git_error("CONFLICT: content in src/main.rs");
/// assert!(msg.contains("merge conflict occurred"));
/// ```
pub fn format_git_error<'a>(error: impl Into<GitErrorInput<'a>>) -> String {
    let error_str = match error.into() {
        GitErrorInput::Str(s) => s.to_string(),
        GitErrorInput::String(s) => s,
        GitErrorInput::Error(e) => e.to_string(),
    };

    for ep in get_error_patterns() {
        if ep.pattern.is_match(&error_str) {
            return ep.message.to_string();
        }
    }

    if error_str.len() > 200 {
        format!(
            "A git error occurred: {}{}. Run `/orchestra doctor` for help.",
            error_str.chars().take(197).collect::<String>(),
            "..."
        )
    } else {
        format!(
            "A git error occurred: {}. Run `/orchestra doctor` for help.",
            error_str
        )
    }
}

/// Input type for format_git_error
#[non_exhaustive]
pub enum GitErrorInput<'a> {
    Str(&'a str),
    String(String),
    Error(&'a dyn std::error::Error),
}

impl<'a> From<&'a str> for GitErrorInput<'a> {
    fn from(s: &'a str) -> Self {
        GitErrorInput::Str(s)
    }
}

impl<'a> From<String> for GitErrorInput<'a> {
    fn from(s: String) -> Self {
        GitErrorInput::String(s)
    }
}

impl<'a> From<&'a String> for GitErrorInput<'a> {
    fn from(s: &'a String) -> Self {
        GitErrorInput::Str(s.as_str())
    }
}

impl<'a> From<&'a dyn std::error::Error> for GitErrorInput<'a> {
    fn from(e: &'a dyn std::error::Error) -> Self {
        GitErrorInput::Error(e)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_abort_and_reset_clean_state() {
        let temp_dir = TempDir::new().unwrap();
        let result = abort_and_reset(temp_dir.path());
        // In a non-git directory, git commands will fail
        // We expect either 0 (if all git commands fail silently) or some "failed" messages
        // The important thing is no "aborted" or "removed" messages
        assert!(!result
            .cleaned
            .iter()
            .any(|s| s.contains("aborted") || s.contains("removed")));
    }

    #[test]
    fn test_abort_and_reset_with_merge_head() {
        let temp_dir = TempDir::new().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();

        // Create MERGE_HEAD file
        let merge_head = git_dir.join("MERGE_HEAD");
        fs::write(&merge_head, "abc123\n").unwrap();

        let result = abort_and_reset(temp_dir.path());
        // merge --abort will fail since it's not a real git repo, but we attempted it
        assert!(result.cleaned.iter().any(|s| s.contains("merge abort")));
    }

    #[test]
    fn test_abort_and_reset_with_squash_msg() {
        let temp_dir = TempDir::new().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();

        // Create SQUASH_MSG file
        let squash_msg = git_dir.join("SQUASH_MSG");
        fs::write(&squash_msg, "Test squash message\n").unwrap();

        let result = abort_and_reset(temp_dir.path());
        assert!(result.cleaned.contains(&"removed SQUASH_MSG".to_string()));
    }

    #[test]
    fn test_abort_and_reset_with_restate_dir() {
        let temp_dir = TempDir::new().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();

        // Create rebase-apply directory
        let rebase_apply = git_dir.join("rebase-apply");
        fs::create_dir(&rebase_apply).unwrap();

        let result = abort_and_reset(temp_dir.path());
        assert!(result.cleaned.iter().any(|s| s.contains("rebase abort")));
    }

    #[test]
    fn test_format_git_error_conflict() {
        let msg = format_git_error("CONFLICT: content in src/main.rs");
        assert!(msg.contains("merge conflict occurred"));
    }

    #[test]
    fn test_format_git_error_checkout() {
        let msg = format_git_error("error: pathspec 'main' did not match any");
        assert!(msg.contains("could not switch branches"));
    }

    #[test]
    fn test_format_git_error_detached() {
        let msg = format_git_error("HEAD detached at abc123");
        assert!(msg.contains("detached HEAD state"));
    }

    #[test]
    fn test_format_git_error_lock() {
        let msg = format_git_error("Unable to create '.git/index.lock': File exists");
        assert!(msg.contains("lock file is blocking"));
    }

    #[test]
    fn test_format_git_error_not_repo() {
        let msg = format_git_error("fatal: not a git repository");
        assert!(msg.contains("not a git repository"));
    }

    #[test]
    fn test_format_git_error_unknown() {
        let msg = format_git_error("some unknown git error");
        assert!(msg.contains("git error occurred"));
        assert!(msg.contains("some unknown git error"));
    }

    #[test]
    fn test_format_git_error_long_error_truncated() {
        let long_error = "a".repeat(300);
        let msg = format_git_error(&long_error);
        assert!(msg.contains("..."));
        assert!(msg.len() < long_error.len() + 50);
    }

    #[test]
    fn test_format_git_error_from_string() {
        let msg = format_git_error("CONFLICT: content");
        assert!(msg.contains("merge conflict occurred"));
    }

    #[test]
    fn test_format_git_error_case_insensitive() {
        let msg = format_git_error("Conflict in src/main.rs");
        assert!(msg.contains("merge conflict occurred"));

        let msg = format_git_error("conflict: content");
        assert!(msg.contains("merge conflict occurred"));
    }

    #[test]
    fn test_merge_conflict_error_display() {
        let err = MergeConflictError {
            message: "Test conflict".to_string(),
        };
        assert_eq!(format!("{}", err), "Test conflict");
    }

    #[test]
    fn test_abort_and_reset_result_default() {
        let result = AbortAndResetResult::default();
        assert_eq!(result.cleaned.len(), 0);
    }

    #[test]
    fn test_abort_and_reset_multiple_issues() {
        let temp_dir = TempDir::new().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();

        // Create multiple issue indicators
        fs::write(git_dir.join("MERGE_HEAD"), "abc123\n").unwrap();
        fs::write(git_dir.join("SQUASH_MSG"), "Test\n").unwrap();

        let result = abort_and_reset(temp_dir.path());
        assert!(result.cleaned.len() >= 2);
    }
}
