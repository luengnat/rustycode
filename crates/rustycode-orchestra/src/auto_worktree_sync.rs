//! Worktree ↔ project root state synchronization for auto-mode.
//!
//! When auto-mode runs inside a worktree, dispatch-critical state files
//! (.orchestra/ metadata) diverge between the worktree (where work happens)
//! and the project root (where startAutoMode reads initial state on restart).
//! Without syncing, restarting auto-mode reads stale state from the project
//! root and re-dispatches already-completed units.
//!
//! Also contains resource staleness detection and stale worktree escape.
//!
//! Matches orchestra-2's auto-worktree-sync.ts implementation.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::safe_fs::{safe_copy, safe_copy_recursive};

// ─── Project Root → Worktree Sync ─────────────────────────────────────────

/// Sync milestone artifacts from project root INTO worktree before deriveState.
///
/// Covers the case where the LLM wrote artifacts to the main repo filesystem
/// (e.g. via absolute paths) but the worktree has stale data. Also deletes
/// orchestra.db in the worktree so it rebuilds from fresh disk state.
///
/// Non-fatal — sync failure should never block dispatch.
///
/// # Arguments
/// * `project_root` - Path to project root
/// * `worktree_path` - Path to worktree (may equal project_root)
/// * `milestone_id` - Current milestone ID (e.g. "M001")
pub fn sync_project_root_to_worktree(
    project_root: &Path,
    worktree_path: &Path,
    milestone_id: Option<&str>,
) {
    if worktree_path == project_root {
        return;
    }
    let milestone_id = match milestone_id {
        Some(id) => id,
        None => return,
    };

    let pr_orchestra = project_root.join(".orchestra");
    let wt_orchestra = worktree_path.join(".orchestra");

    // Copy milestone directory from project root to worktree if the project root
    // has newer artifacts (e.g. slices that don't exist in the worktree yet)
    let _ = safe_copy_recursive(
        &pr_orchestra.join("milestones").join(milestone_id),
        &wt_orchestra.join("milestones").join(milestone_id),
    );

    // Delete worktree orchestra.db so it rebuilds from the freshly synced files.
    // Stale DB rows are the root cause of the infinite skip loop.
    let wt_db = wt_orchestra.join("orchestra.db");
    if wt_db.exists() {
        let _ = fs::remove_file(&wt_db);
    }
}

// ─── Worktree → Project Root Sync ─────────────────────────────────────────

/// Sync dispatch-critical .orchestra/ state files from worktree to project root.
///
/// Only runs when inside an auto-worktree (worktree_path differs from project_root).
/// Copies: STATE.md + active milestone directory (roadmap, slice plans, task summaries).
///
/// Non-fatal — sync failure should never block dispatch.
///
/// # Arguments
/// * `worktree_path` - Path to worktree
/// * `project_root` - Path to project root
/// * `milestone_id` - Current milestone ID (e.g. "M001")
pub fn sync_state_to_project_root(
    worktree_path: &Path,
    project_root: &Path,
    milestone_id: Option<&str>,
) {
    if worktree_path == project_root {
        return;
    }
    let milestone_id = match milestone_id {
        Some(id) => id,
        None => return,
    };

    let wt_orchestra = worktree_path.join(".orchestra");
    let pr_orchestra = project_root.join(".orchestra");

    // 1. STATE.md — the quick-glance status used by initial deriveState()
    let _ = safe_copy(
        &wt_orchestra.join("STATE.md"),
        &pr_orchestra.join("STATE.md"),
    );

    // 2. Milestone directory — ROADMAP, slice PLANs, task summaries
    // Copy the entire milestone .orchestra subtree so deriveState reads current checkboxes
    let _ = safe_copy_recursive(
        &wt_orchestra.join("milestones").join(milestone_id),
        &pr_orchestra.join("milestones").join(milestone_id),
    );

    // 3. Merge completed-units.json (set-union of both locations)
    // Prevents already-completed units from being re-dispatched after crash/restart.
    let src_keys_file = wt_orchestra.join("completed-units.json");
    let dst_keys_file = pr_orchestra.join("completed-units.json");
    if src_keys_file.exists() {
        if let Ok(src_content) = fs::read_to_string(&src_keys_file) {
            if let Ok(src_keys) = serde_json::from_str::<Vec<String>>(&src_content) {
                let mut dst_keys = Vec::new();
                if dst_keys_file.exists() {
                    if let Ok(dst_content) = fs::read_to_string(&dst_keys_file) {
                        let _ = serde_json::from_str::<Vec<String>>(&dst_content)
                            .map(|keys| dst_keys = keys);
                    }
                }
                let merged: HashSet<String> = dst_keys.into_iter().chain(src_keys).collect();
                let merged_vec: Vec<String> = merged.into_iter().collect();
                let content = serde_json::to_string_pretty(&merged_vec).unwrap_or_default();
                let _ = fs::write(&dst_keys_file, content);
            }
        }
    }

    // 4. Runtime records — unit dispatch state used by selfHealRuntimeRecords().
    // Without this, a crash during a unit leaves the runtime record only in the
    // worktree. If the next session resolves basePath before worktree re-entry,
    // selfHeal can't find or clear the stale record.
    let _ = safe_copy_recursive(
        &wt_orchestra.join("runtime").join("units"),
        &pr_orchestra.join("runtime").join("units"),
    );
}

// ─── Resource Staleness ───────────────────────────────────────────────────

/// Read the resource version (semver) from the managed-resources manifest.
///
/// Uses orchestraVersion instead of syncedAt so that launching a second session
/// doesn't falsely trigger staleness.
pub fn read_resource_version() -> Option<String> {
    let agent_dir = if let Ok(dir) = env::var("Orchestra_CODING_AGENT_DIR") {
        PathBuf::from(dir)
    } else {
        dirs::home_dir()?.join(".orchestra").join("agent")
    };

    let manifest_path = agent_dir.join("managed-resources.json");
    if !manifest_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&manifest_path).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&content).ok()?;
    let version = manifest.get("orchestraVersion")?.as_str()?;
    Some(version.to_string())
}

/// Check if managed resources have been updated since session start.
///
/// Returns a warning message if stale, null otherwise.
pub fn check_resources_stale(version_on_start: Option<&str>) -> Option<String> {
    let version_on_start = version_on_start?;

    let current = read_resource_version()?;

    if current != version_on_start {
        Some("Orchestra resources were updated since this session started. Restart orchestra to load the new code.".to_string())
    } else {
        None
    }
}

// ─── Stale Worktree Escape ────────────────────────────────────────────────

/// Detect and escape a stale worktree cwd.
///
/// After milestone completion + merge, the worktree directory is removed but
/// the process cwd may still point inside `.orchestra/worktrees/<MID>/`.
/// When a new session starts, `process.cwd()` is passed as `base` to startAuto
/// and all subsequent writes land in the wrong directory. This function detects
/// that scenario and chdir back to the project root.
///
/// Returns the corrected base path.
pub fn escape_stale_worktree(base: &Path) -> PathBuf {
    let base_str = base.to_string_lossy();
    let sep = std::path::MAIN_SEPARATOR;
    let marker = format!("{}.orchestra{}worktrees{}", sep, sep, sep);

    if let Some(idx) = base_str.find(&marker) {
        // base is inside .orchestra/worktrees/<something> — extract the project root
        let project_root = PathBuf::from(&base_str[..idx]);
        if env::set_current_dir(&project_root).is_ok() {
            return project_root;
        }
    }

    base.to_path_buf()
}

/// Clean stale runtime unit files for completed milestones.
///
/// After restart, stale runtime/units/*.json from prior milestones can
/// cause deriveState to resume the wrong milestone. Removes files
/// for milestones that have a SUMMARY (fully complete).
///
/// # Arguments
/// * `orchestra_root_path` - Path to .orchestra directory
/// * `has_milestone_summary` - Function that returns true if milestone has SUMMARY file
///
/// Returns the number of files cleaned.
pub fn clean_stale_runtime_units<F>(orchestra_root_path: &Path, has_milestone_summary: F) -> usize
where
    F: Fn(&str) -> bool,
{
    let runtime_units_dir = orchestra_root_path.join("runtime").join("units");
    if !runtime_units_dir.exists() {
        return 0;
    }

    let mut cleaned = 0;

    if let Ok(entries) = fs::read_dir(&runtime_units_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let file_name = match path.file_name() {
                Some(name) => name.to_string_lossy().into_owned(),
                None => continue,
            };

            // Extract milestone ID from filename (M001 or M001-abc123 format)
            // M001 followed by optional - and 6 char hex
            if let Some(mid) = file_name.split('-').next() {
                if mid.starts_with('M')
                    && has_milestone_summary(mid)
                    && fs::remove_file(&path).is_ok()
                {
                    cleaned += 1;
                }
            }
        }
    }

    cleaned
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sync_project_root_to_worktree_same_path() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Should not panic when paths are the same
        sync_project_root_to_worktree(path, path, Some("M001"));
    }

    #[test]
    fn test_sync_state_to_project_root_same_path() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Should not panic when paths are the same
        sync_state_to_project_root(path, path, Some("M001"));
    }

    #[test]
    fn test_sync_project_root_to_worktree_no_milestone() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().join("project");
        let worktree = temp_dir.path().join("worktree");
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&worktree).unwrap();

        // Should return early without milestone
        sync_project_root_to_worktree(&project_root, &worktree, None);
    }

    #[test]
    fn test_sync_state_to_project_root_no_milestone() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().join("project");
        let worktree = temp_dir.path().join("worktree");
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&worktree).unwrap();

        // Should return early without milestone
        sync_state_to_project_root(&worktree, &project_root, None);
    }

    #[test]
    fn test_read_resource_version_none() {
        // When Orchestra_CODING_AGENT_DIR is not set and ~/.orchestra/agent doesn't exist
        env::set_var("Orchestra_CODING_AGENT_DIR", "");
        let result = read_resource_version();
        assert!(result.is_none());
    }

    #[test]
    fn test_check_resources_stale_none() {
        env::set_var("Orchestra_CODING_AGENT_DIR", "");
        let result = check_resources_stale(Some("1.0.0"));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_resources_stale_no_version_on_start() {
        env::set_var("Orchestra_CODING_AGENT_DIR", "");
        let result = check_resources_stale(None);
        assert!(result.is_none());
    }

    #[test]
    fn test_escape_stale_worktree_normal_path() {
        let path = Path::new("/Users/test/project");
        let result = escape_stale_worktree(path);
        assert_eq!(result, path);
    }

    #[test]
    fn test_escape_stale_worktree_no_marker() {
        let path = Path::new("/Users/test/project/.orchestra/worktree/normal");
        let result = escape_stale_worktree(path);
        // "worktree" != "worktrees", so should return original
        assert_eq!(result, path);
    }

    #[test]
    fn test_clean_stale_runtime_units_no_dir() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_root = temp_dir.path().join(".orchestra");

        let cleaned = clean_stale_runtime_units(&orchestra_root, |_| false);
        assert_eq!(cleaned, 0);
    }

    #[test]
    fn test_clean_stale_runtime_units_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_root = temp_dir.path().join(".orchestra");
        let runtime_dir = orchestra_root.join("runtime").join("units");
        fs::create_dir_all(&runtime_dir).unwrap();

        let cleaned = clean_stale_runtime_units(&orchestra_root, |_| false);
        assert_eq!(cleaned, 0);
    }

    #[test]
    fn test_clean_stale_runtime_units_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_root = temp_dir.path().join(".orchestra");
        let runtime_dir = orchestra_root.join("runtime").join("units");
        fs::create_dir_all(&runtime_dir).unwrap();

        // Create test files
        fs::write(runtime_dir.join("M001-abc123.json"), "{}").unwrap();
        fs::write(runtime_dir.join("M002-def456.json"), "{}").unwrap();
        fs::write(runtime_dir.join("not-json.txt"), "").unwrap();

        // Only M001 has summary
        let cleaned = clean_stale_runtime_units(&orchestra_root, |mid| mid == "M001");
        assert_eq!(cleaned, 1);

        // M001-abc123.json should be deleted
        assert!(!runtime_dir.join("M001-abc123.json").exists());
        // M002-def456.json should still exist
        assert!(runtime_dir.join("M002-def456.json").exists());
    }
}
