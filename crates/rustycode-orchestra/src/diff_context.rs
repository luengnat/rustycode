//! Diff-aware context module — prioritizes recently-changed files when building
//! context for the AI agent. Uses git diff/status to discover changes, then
//! provides ranking utilities for context-window budget allocation.
//!
//! Standalone module: only uses std::process::Command for git operations.
//!
//! Matches orchestra-2's diff-context.ts implementation.

use crate::error::{OrchestraV2Error, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

// ─── Types ───────────────────────────────────────────────────────────────────

/// File change information with metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFileInfo {
    /// File path relative to git root
    pub path: String,
    /// Type of change
    pub change_type: ChangeType,
    /// Approximate number of lines changed (if available)
    pub lines_changed: Option<usize>,
}

/// Type of file change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChangeType {
    /// File was modified
    Modified,
    /// File was added
    Added,
    /// File was deleted
    Deleted,
    /// File is staged for commit
    Staged,
}

/// Options for querying recently changed files
#[derive(Debug, Clone)]
pub struct RecentFilesOptions {
    /// Maximum number of files to return (default 20)
    pub max_files: Option<usize>,
    /// Only consider commits within this many days (default 7)
    pub since_days: Option<u64>,
}

impl Default for RecentFilesOptions {
    fn default() -> Self {
        Self {
            max_files: Some(20),
            since_days: Some(7),
        }
    }
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Returns recently-changed file paths, deduplicated and sorted by recency
/// (most recent first). Combines committed diffs, staged changes, and
/// unstaged/untracked files from `git status`.
///
/// # Arguments
/// * `cwd` - Current working directory (should be in a git repo)
/// * `options` - Optional query parameters
///
/// # Returns
/// Vector of file paths (relative to git root), or empty vector on error
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::diff_context::*;
///
/// let files = get_recently_changed_files(
///     Path::new("/my/project"),
///     Some(&RecentFilesOptions {
///         max_files: Some(10),
///         since_days: Some(3),
///         ..Default::default()
///     }),
/// );
/// ```
pub fn get_recently_changed_files(cwd: &Path, options: Option<&RecentFilesOptions>) -> Vec<String> {
    let default_opts = RecentFilesOptions::default();
    let opts = options.unwrap_or(&default_opts);
    let max_files = opts.max_files.unwrap_or(20);
    let since_days = opts.since_days.unwrap_or(7);

    let days = std::cmp::max(1, since_days);

    // Run all three queries — they read independent git state
    let log_result = git_log_since(cwd, days).or_else(|_| git_log_since_fallback(cwd));

    let staged_result = git_diff_cached(cwd);
    let status_result = git_status_porcelain(cwd);

    let committed_files: Vec<String> = log_result
        .into_iter()
        .flat_map(|s| split_lines(&s))
        .collect();

    let staged_files: Vec<String> = staged_result
        .into_iter()
        .flat_map(|s| split_lines(&s))
        .collect();

    let status_files: Vec<String> = status_result
        .into_iter()
        .flat_map(|s| split_lines(&s))
        .map(|line| {
            // Strip XY status code and space
            if line.len() > 3 {
                line[3..].to_string()
            } else {
                line
            }
        })
        .collect();

    // Deduplicate, preserving insertion order (most-recent-first: status → staged → committed)
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for file in status_files
        .into_iter()
        .chain(staged_files)
        .chain(committed_files)
    {
        if !seen.contains(&file) {
            seen.insert(file.clone());
            result.push(file);
        }
    }

    result.truncate(max_files);
    result
}

/// Returns richer change metadata: change type and approximate line counts.
///
/// The three git queries (diff --cached --numstat, diff --numstat, status --porcelain)
/// run concurrently — they read independent git state.
///
/// # Arguments
/// * `cwd` - Current working directory (should be in a git repo)
///
/// # Returns
/// Vector of changed file info, or empty vector on error
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::diff_context::*;
///
/// let changes = get_changed_files_with_context(Path::new("/my/project"));
/// for change in &changes {
///     println!("{:?}: {} lines changed",
///         change.change_type,
///         change.lines_changed.unwrap_or(0));
/// }
/// ```
pub fn get_changed_files_with_context(cwd: &Path) -> Vec<ChangedFileInfo> {
    let cached_numstat = git_diff_cached_numstat(cwd);
    let unstaged_numstat = git_diff_numstat(cwd);
    let status_raw = git_status_porcelain(cwd);

    let mut result = Vec::new();
    let mut seen = HashSet::new();

    // Helper function to add unique entries
    fn add_entry(
        result: &mut Vec<ChangedFileInfo>,
        seen: &mut HashSet<String>,
        info: ChangedFileInfo,
    ) {
        if !seen.contains(&info.path) {
            seen.insert(info.path.clone());
            result.push(info);
        }
    }

    // 1. Staged files with numstat
    for line in cached_numstat.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let file_path = parts[2];
            let lines = parse_numstat(parts[0], parts[1]);
            add_entry(
                &mut result,
                &mut seen,
                ChangedFileInfo {
                    path: file_path.to_string(),
                    change_type: ChangeType::Staged,
                    lines_changed: lines,
                },
            );
        }
    }

    // 2. Unstaged modifications with numstat
    for line in unstaged_numstat.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let file_path = parts[2];
            let lines = parse_numstat(parts[0], parts[1]);
            add_entry(
                &mut result,
                &mut seen,
                ChangedFileInfo {
                    path: file_path.to_string(),
                    change_type: ChangeType::Modified,
                    lines_changed: lines,
                },
            );
        }
    }

    // 3. Untracked / deleted from porcelain status
    if let Ok(status) = status_raw {
        for line in status.lines() {
            if line.len() < 3 {
                continue;
            }

            let code = &line[0..2];
            let file_path = &line[3..];

            if seen.contains(file_path) {
                continue;
            }

            let change_type = if code.contains('?') {
                ChangeType::Added
            } else if code.contains('D') {
                ChangeType::Deleted
            } else if code.contains('A') {
                ChangeType::Added
            } else {
                ChangeType::Modified
            };

            add_entry(
                &mut result,
                &mut seen,
                ChangedFileInfo {
                    path: file_path.to_string(),
                    change_type,
                    lines_changed: None,
                },
            );
        }
    }

    result
}

/// Ranks a file list so that recently-changed files appear first.
///
/// Files present in `changed_files` are placed at the front (in their
/// original changed_files order), followed by unchanged files in their
/// original order.
///
/// # Arguments
/// * `files` - List of files to rank
/// * `changed_files` - List of recently changed files (priority order)
///
/// # Returns
/// Ranked file list with changed files first
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::diff_context::*;
///
/// let all_files = vec!["file1.rs", "file2.rs", "file3.rs"];
/// let changed = vec!["file2.rs", "file1.rs"];
/// let ranked = rank_files_by_relevance(&all_files, &changed);
/// // Result: ["file2.rs", "file1.rs", "file3.rs"]
/// ```
pub fn rank_files_by_relevance(files: &[String], changed_files: &[String]) -> Vec<String> {
    let changed_set: HashSet<&str> = changed_files.iter().map(|s| s.as_str()).collect();
    let mut changed = Vec::new();
    let mut rest = Vec::new();

    for file in files {
        if changed_set.contains(file.as_str()) {
            changed.push(file.clone());
        } else {
            rest.push(file.clone());
        }
    }

    // Maintain changed_files priority order within the changed group
    let changed_order: HashMap<&str, usize> = changed_files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.as_str(), i))
        .collect();

    changed.sort_by_key(|f| changed_order.get(f.as_str()).unwrap_or(&usize::MAX));

    changed.into_iter().chain(rest).collect()
}

// ─── Internals ─────────────────────────────────────────────────────────────

/// Run git command and return stdout
fn git_command(args: &[&str], cwd: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(OrchestraV2Error::Io)?;

    if !output.status.success() {
        return Ok(String::new());
    }

    String::from_utf8(output.stdout)
        .map_err(|e| OrchestraV2Error::Serialization(format!("Invalid UTF-8: {}", e)))
}

/// Get committed changes since N days ago (fallback to HEAD~10 on error)
fn git_log_since(cwd: &Path, days: u64) -> Result<String> {
    let output = git_command(
        &[
            "log",
            "--diff-filter=ACMR",
            "--name-only",
            "--pretty=format:",
            &format!("--since={} days ago", days),
        ],
        cwd,
    )?;

    if output.trim().is_empty() {
        // Fallback to HEAD~10
        git_command(&["diff", "--name-only", "HEAD~10"], cwd)
    } else {
        Ok(output)
    }
}

/// Fallback to HEAD~10 for recent commits
fn git_log_since_fallback(cwd: &Path) -> Result<String> {
    git_command(&["diff", "--name-only", "HEAD~10"], cwd)
}

/// Get staged changes
fn git_diff_cached(cwd: &Path) -> Result<String> {
    git_command(&["diff", "--cached", "--name-only"], cwd)
}

/// Get porcelain status
fn git_status_porcelain(cwd: &Path) -> Result<String> {
    git_command(&["status", "--porcelain"], cwd)
}

/// Get staged changes with numstat
fn git_diff_cached_numstat(cwd: &Path) -> String {
    git_command(&["diff", "--cached", "--numstat"], cwd).unwrap_or_default()
}

/// Get unstaged changes with numstat
fn git_diff_numstat(cwd: &Path) -> String {
    git_command(&["diff", "--numstat"], cwd).unwrap_or_default()
}

/// Split output into lines, trimming whitespace
fn split_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse numstat format (added deleted)
fn parse_numstat(added: &str, deleted: &str) -> Option<usize> {
    if added == "-" || deleted == "-" {
        return None;
    }

    let added_val: usize = added.parse().ok()?;
    let deleted_val: usize = deleted.parse().ok()?;

    Some(added_val.saturating_add(deleted_val))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_numstat() {
        // Normal case
        assert_eq!(parse_numstat("10", "5"), Some(15));
        assert_eq!(parse_numstat("0", "0"), Some(0));

        // Binary file
        assert_eq!(parse_numstat("-", "-"), None);

        // Invalid
        assert_eq!(parse_numstat("abc", "def"), None);
    }

    #[test]
    fn test_split_lines() {
        let output = "line1\nline2\n  \nline3\n";
        let result = split_lines(output);
        assert_eq!(result, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_split_lines_empty() {
        let output = "";
        let result = split_lines(output);
        assert!(result.is_empty());
    }

    #[test]
    fn test_split_lines_whitespace() {
        let output = "  \n  \n  \n";
        let result = split_lines(output);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rank_files_by_relevance_empty() {
        let files = vec!["a.rs".to_string(), "b.rs".to_string()];
        let changed = vec![];
        let ranked = rank_files_by_relevance(&files, &changed);
        assert_eq!(ranked, files);
    }

    #[test]
    fn test_rank_files_by_relevance_all_changed() {
        let files = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let changed = vec!["b.rs".to_string(), "a.rs".to_string(), "c.rs".to_string()];
        let ranked = rank_files_by_relevance(&files, &changed);
        // Changed files maintain their order from changed_files
        assert_eq!(
            ranked,
            vec!["b.rs".to_string(), "a.rs".to_string(), "c.rs".to_string()]
        );
    }

    #[test]
    fn test_rank_files_by_relevance_mixed() {
        let files = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let changed = vec!["b.rs".to_string()];
        let ranked = rank_files_by_relevance(&files, &changed);
        assert_eq!(ranked[0], "b.rs");
        assert!(ranked.contains(&"a.rs".to_string()));
        assert!(ranked.contains(&"c.rs".to_string()));
    }

    #[test]
    fn test_changed_file_info() {
        let info = ChangedFileInfo {
            path: "src/main.rs".to_string(),
            change_type: ChangeType::Modified,
            lines_changed: Some(10),
        };
        assert_eq!(info.path, "src/main.rs");
        assert_eq!(info.change_type, ChangeType::Modified);
        assert_eq!(info.lines_changed, Some(10));
    }
}
