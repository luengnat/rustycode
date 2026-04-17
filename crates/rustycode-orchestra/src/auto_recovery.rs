//! Orchestra Auto Recovery — Crash Recovery and State Reconciliation
//!
//! Orchestrates crash recovery using session forensics:
//! * Artifact resolution and verification
//! * Blocker placeholders for stuck units
//! * Completed-unit persistence
//! * Merge state reconciliation
//! * Self-heal runtime records
//! * Loop remediation steps
//!
//! Pure functions that receive all needed state as parameters — no module-level
//! globals or shared state dependencies.
//!
//! Critical for production autonomous systems to ensure clean recovery from crashes.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ─── Types ────────────────────────────────────────────────────────────────────

/// Unit type identifier
pub type UnitType = String;

/// Unit identifier (e.g., "M01/S01/T01")
pub type UnitId = String;

/// Completion key (e.g., "execute-task/M01/S01/T01")
pub type CompletionKey = String;

// ─── Artifact Resolution ───────────────────────────────────────────────────────

/// Resolve the expected artifact path for a unit to an absolute path.
///
/// Returns None if the unit type has no standard artifact path.
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_recovery::*;
///
/// let path = resolve_expected_artifact_path(
///     "execute-task",
///     "M01/S01/T01",
///     "/project"
/// );
/// assert!(path.is_some());
/// ```
pub fn resolve_expected_artifact_path(
    unit_type: &str,
    unit_id: &str,
    base: &Path,
) -> Option<PathBuf> {
    let parts: Vec<&str> = unit_id.split('/').collect();
    let mid = parts.first()?;
    let sid = parts.get(1);

    match unit_type {
        "research-milestone" => {
            let dir = base.join(".orchestra").join("milestones").join(mid);
            Some(dir.join(format!("{}-RESEARCH.md", mid)))
        }
        "plan-milestone" => {
            let dir = base.join(".orchestra").join("milestones").join(mid);
            Some(dir.join(format!("{}-ROADMAP.md", mid)))
        }
        "research-slice" => {
            let sid = sid?;
            let dir = base
                .join(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid);
            Some(dir.join(format!("{}-RESEARCH.md", sid)))
        }
        "plan-slice" => {
            let sid = sid?;
            let dir = base
                .join(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid);
            Some(dir.join(format!("{}-PLAN.md", sid)))
        }
        "reassess-roadmap" => {
            let sid = sid?;
            let dir = base
                .join(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid);
            Some(dir.join(format!("{}-ASSESSMENT.md", sid)))
        }
        "run-uat" => {
            let sid = sid?;
            let dir = base
                .join(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid);
            Some(dir.join(format!("{}-UAT-RESULT.md", sid)))
        }
        "execute-task" => {
            let tid = parts.get(2)?;
            let sid = sid?;
            let dir = base
                .join(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid)
                .join("tasks");
            Some(dir.join(format!("{}-SUMMARY.md", tid)))
        }
        "complete-slice" => {
            let sid = sid?;
            let dir = base
                .join(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid);
            Some(dir.join(format!("{}-SUMMARY.md", sid)))
        }
        "validate-milestone" => {
            let dir = base.join(".orchestra").join("milestones").join(mid);
            Some(dir.join(format!("{}-VALIDATION.md", mid)))
        }
        "complete-milestone" => {
            let dir = base.join(".orchestra").join("milestones").join(mid);
            Some(dir.join(format!("{}-SUMMARY.md", mid)))
        }
        "replan-slice" => {
            let sid = sid?;
            let dir = base
                .join(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid);
            Some(dir.join(format!("{}-REPLAN.md", sid)))
        }
        "rewrite-docs" => None,
        _ => None,
    }
}

/// Check whether the expected artifact(s) for a unit exist on disk.
///
/// Returns true if all required artifacts exist, or if the unit type has no
/// single verifiable artifact (e.g., hook units).
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_recovery::*;
///
/// let exists = verify_expected_artifact(
///     "execute-task",
///     "M01/S01/T01",
///     "/project"
/// );
/// ```
pub fn verify_expected_artifact(unit_type: &str, unit_id: &str, base: &Path) -> bool {
    // Hook units have no standard artifact — always pass
    if unit_type.starts_with("hook/") {
        return true;
    }

    let abs_path = match resolve_expected_artifact_path(unit_type, unit_id, base) {
        Some(p) => p,
        // For unit types with no verifiable artifact (null path), the parent
        // directory is missing on disk — treat as stale completion state
        None => return false,
    };

    if !abs_path.exists() {
        return false;
    }

    // Additional verification for specific unit types

    // plan-slice must produce a plan with actual task entries
    if unit_type == "plan-slice" {
        if let Ok(content) = std::fs::read_to_string(&abs_path) {
            // Check for task pattern: "- [T] **T123:"
            if !content.contains("- [") || !content.contains("**T") {
                return false;
            }
        }
    }

    // execute-task must have its checkbox marked [x] in the slice plan
    if unit_type == "execute-task" {
        let parts: Vec<&str> = unit_id.split('/').collect();
        if let (Some(mid), Some(sid), Some(tid)) = (parts.first(), parts.get(1), parts.get(2)) {
            let plan_path = base
                .join(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid)
                .join(format!("{}-PLAN.md", sid));

            // Plan file must exist
            if !plan_path.exists() {
                return false;
            }

            // Check for "- [x] **Tid:" pattern
            if let Ok(content) = std::fs::read_to_string(&plan_path) {
                let checkbox_pattern = format!("- [x] **{}:", tid);
                if !content.contains(&checkbox_pattern) {
                    return false;
                }
            } else {
                return false;
            }
        }
    }

    // complete-slice must also produce a UAT file
    if unit_type == "complete-slice" {
        let parts: Vec<&str> = unit_id.split('/').collect();
        if let (Some(mid), Some(sid)) = (parts.first(), parts.get(1)) {
            let uat_path = base
                .join(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid)
                .join(format!("{}-UAT-RESULT.md", sid));

            if !uat_path.exists() {
                return false;
            }
        }
    }

    true
}

/// Write a placeholder artifact so the pipeline can advance past a stuck unit.
///
/// Returns the relative path written, or None if the path couldn't be resolved.
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_recovery::*;
///
/// let path = write_blocker_placeholder(
///     "execute-task",
///     "M01/S01/T01",
///     "/project",
///     "LLM failed to respond"
/// );
/// ```
pub fn write_blocker_placeholder(
    unit_type: &str,
    unit_id: &str,
    base: &Path,
    reason: &str,
) -> Result<Option<String>> {
    let abs_path = match resolve_expected_artifact_path(unit_type, unit_id, base) {
        Some(p) => p,
        None => return Ok(None),
    };

    let dir = abs_path.parent().context("Artifact path has no parent")?;
    std::fs::create_dir_all(dir)?;

    let content = format!(
        "# BLOCKER — auto-mode recovery failed\n\n\
         Unit `{}` for `{}` failed to produce this artifact after idle recovery exhausted all retries.\n\n\
         **Reason**: {}\n\n\
         This placeholder was written by auto-mode so the pipeline can advance.\n\
         Review and replace this file before relying on downstream artifacts.",
        unit_type, unit_id, reason
    );

    std::fs::write(&abs_path, content)?;

    Ok(Some(diagnose_expected_artifact(unit_type, unit_id, base)))
}

/// Get a human-readable description of the expected artifact.
pub fn diagnose_expected_artifact(unit_type: &str, unit_id: &str, _base: &Path) -> String {
    let parts: Vec<&str> = unit_id.split('/').collect();
    let mid = parts.first().unwrap_or(&"?");
    let sid = parts.get(1).unwrap_or(&"?");

    match unit_type {
        "research-milestone" => {
            format!(
                "{:?} (milestone research)",
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join(format!("{}-RESEARCH.md", mid))
            )
        }
        "plan-milestone" => {
            format!(
                "{:?} (milestone roadmap)",
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join(format!("{}-ROADMAP.md", mid))
            )
        }
        "research-slice" => {
            format!(
                "{:?} (slice research)",
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join("slices")
                    .join(sid)
                    .join(format!("{}-RESEARCH.md", sid))
            )
        }
        "plan-slice" => {
            format!(
                "{:?} (slice plan)",
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join("slices")
                    .join(sid)
                    .join(format!("{}-PLAN.md", sid))
            )
        }
        "execute-task" => {
            let tid = parts.get(2).unwrap_or(&"?");
            format!(
                "Task {} marked [x] in {:?} + summary written",
                tid,
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join("slices")
                    .join(sid)
                    .join(format!("{}-PLAN.md", sid))
            )
        }
        "complete-slice" => {
            format!(
                "Slice {} marked [x] in {:?} + summary + UAT written",
                sid,
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join(format!("{}-ROADMAP.md", mid))
            )
        }
        "replan-slice" => {
            format!(
                "{:?} + updated {:?}",
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join("slices")
                    .join(sid)
                    .join(format!("{}-REPLAN.md", sid)),
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join("slices")
                    .join(sid)
                    .join(format!("{}-PLAN.md", sid))
            )
        }
        "rewrite-docs" => {
            "Active overrides resolved in .orchestra/OVERRIDES.md + plan documents updated"
                .to_string()
        }
        "reassess-roadmap" => {
            format!(
                "{:?} (roadmap reassessment)",
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join("slices")
                    .join(sid)
                    .join(format!("{}-ASSESSMENT.md", sid))
            )
        }
        "run-uat" => {
            format!(
                "{:?} (UAT result)",
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join("slices")
                    .join(sid)
                    .join(format!("{}-UAT-RESULT.md", sid))
            )
        }
        "validate-milestone" => {
            format!(
                "{:?} (milestone validation report)",
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join(format!("{}-VALIDATION.md", mid))
            )
        }
        "complete-milestone" => {
            format!(
                "{:?} (milestone summary)",
                Path::new(".orchestra")
                    .join("milestones")
                    .join(mid)
                    .join(format!("{}-SUMMARY.md", mid))
            )
        }
        _ => "Unknown unit type".to_string(),
    }
}

// ─── Completed Unit Persistence ───────────────────────────────────────────────

/// Path to the persisted completed-unit keys file.
pub fn completed_keys_path(base: &Path) -> PathBuf {
    base.join(".orchestra").join("completed-units.json")
}

/// Write a completed unit key to disk (read-modify-write append to set).
pub fn persist_completed_key(base: &Path, key: &str) -> Result<()> {
    use crate::atomic_write;

    let file = completed_keys_path(base);
    let mut keys: Vec<String> = Vec::new();

    if file.exists() {
        if let Ok(content) = std::fs::read_to_string(&file) {
            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&content) {
                keys = parsed;
            }
        }
    }

    let key_set: HashSet<_> = keys.iter().cloned().collect();
    if !key_set.contains(key) {
        keys.push(key.to_string());
        atomic_write(&file, &serde_json::to_string_pretty(&keys)?)?;
    }

    Ok(())
}

/// Remove a stale completed unit key from disk.
pub fn remove_persisted_key(base: &Path, key: &str) -> Result<()> {
    let file = completed_keys_path(base);

    if file.exists() {
        if let Ok(content) = std::fs::read_to_string(&file) {
            if let Ok(mut keys) = serde_json::from_str::<Vec<String>>(&content) {
                let original_len = keys.len();
                keys.retain(|k| k != key);

                // Only write if the key was actually present
                if keys.len() != original_len {
                    use crate::atomic_write;
                    atomic_write(&file, &serde_json::to_string_pretty(&keys)?)?;
                }
            }
        }
    }

    Ok(())
}

/// Load all completed unit keys from disk into the in-memory set.
pub fn load_persisted_keys(base: &Path, target: &mut HashSet<String>) -> Result<()> {
    let file = completed_keys_path(base);

    if file.exists() {
        if let Ok(content) = std::fs::read_to_string(&file) {
            if let Ok(keys) = serde_json::from_str::<Vec<String>>(&content) {
                for key in keys {
                    target.insert(key);
                }
            }
        }
    }

    Ok(())
}

// ─── Loop Remediation ─────────────────────────────────────────────────────────

/// Build concrete, manual remediation steps for a loop-detected unit failure.
///
/// These are shown when automatic reconciliation is not possible.
pub fn build_loop_remediation_steps(
    unit_type: &str,
    unit_id: &str,
    _base: &Path,
) -> Option<String> {
    let parts: Vec<&str> = unit_id.split('/').collect();
    let mid = parts.first()?;
    let sid = parts.get(1).copied()?;
    let tid = parts.get(2).copied();

    match unit_type {
        "execute-task" => {
            let tid = tid?;
            let plan_rel = Path::new(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid)
                .join(format!("{}-PLAN.md", sid));

            let summary_rel = Path::new(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid)
                .join("tasks")
                .join(format!("{}-SUMMARY.md", tid));

            return Some(format!(
                "   1. Write {:?} (even a partial summary is sufficient to unblock the pipeline)\n\
                   2. Mark {} [x] in {:?}: change \"- [ ] **{}:\" → \"- [x] **{}:\"\n\
                   3. Run `orchestra doctor` to reconcile .orchestra/ state\n\
                   4. Resume auto-mode — it will pick up from the next task",
                summary_rel, tid, plan_rel, tid, tid
            ));
        }
        "plan-slice" | "research-slice" => {
            let artifact_rel = if unit_type == "plan-slice" {
                format!("{}-PLAN.md", sid)
            } else {
                format!("{}-RESEARCH.md", sid)
            };

            let path = Path::new(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid)
                .join(artifact_rel);

            return Some(format!(
                "   1. Write {:?} manually (or with the LLM in interactive mode)\n\
                 2. Run `orchestra doctor` to reconcile .orchestra/ state\n\
                   3. Resume auto-mode",
                path
            ));
        }
        "complete-slice" => {
            let slice_path = Path::new(".orchestra")
                .join("milestones")
                .join(mid)
                .join("slices")
                .join(sid);

            let roadmap_rel = Path::new(".orchestra")
                .join("milestones")
                .join(mid)
                .join(format!("{}-ROADMAP.md", mid));

            return Some(format!(
                "   1. Write the slice summary and UAT file for {} in {:?}\n\
                 2. Mark {} [x] in {:?}\n\
                 3. Run `orchestra doctor` to reconcile .orchestra/ state\n\
                 4. Resume auto-mode",
                sid, slice_path, sid, roadmap_rel
            ));
        }
        "validate-milestone" => {
            let artifact_rel = Path::new(".orchestra")
                .join("milestones")
                .join(mid)
                .join(format!("{}-VALIDATION.md", mid));

            return Some(format!(
                "   1. Write {:?} with verdict: pass\n\
                 2. Run `orchestra doctor`\n\
                   3. Resume auto-mode",
                artifact_rel
            ));
        }
        _ => {}
    }

    None
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_expected_artifact_path_execute_task() {
        let base = Path::new("/project");
        let path = resolve_expected_artifact_path("execute-task", "M01/S01/T01", base);

        assert!(path.is_some());
        let path = path.unwrap();
        assert!(path.ends_with(".orchestra/milestones/M01/slices/S01/tasks/T01-SUMMARY.md"));
    }

    #[test]
    fn test_resolve_expected_artifact_path_plan_slice() {
        let base = Path::new("/project");
        let path = resolve_expected_artifact_path("plan-slice", "M01/S01", base);

        assert!(path.is_some());
        let path = path.unwrap();
        assert!(path.ends_with(".orchestra/milestones/M01/slices/S01/S01-PLAN.md"));
    }

    #[test]
    fn test_resolve_expected_artifact_path_complete_milestone() {
        let base = Path::new("/project");
        let path = resolve_expected_artifact_path("complete-milestone", "M01", base);

        assert!(path.is_some());
        let path = path.unwrap();
        assert!(path.ends_with(".orchestra/milestones/M01/M01-SUMMARY.md"));
    }

    #[test]
    fn test_resolve_expected_artifact_path_unknown() {
        let base = Path::new("/project");
        let path = resolve_expected_artifact_path("unknown-type", "M01", base);

        assert!(path.is_none());
    }

    #[test]
    fn test_verify_expected_artifact_missing() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Artifact doesn't exist
        let result = verify_expected_artifact("execute-task", "M01/S01/T01", base);
        assert!(!result);
    }

    #[test]
    fn test_verify_expected_artifact_exists() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Create the artifact
        let artifact_path = base
            .join(".orchestra")
            .join("milestones")
            .join("M01")
            .join("slices")
            .join("S01")
            .join("tasks")
            .join("T01-SUMMARY.md");

        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        std::fs::write(&artifact_path, "# Summary").unwrap();

        let result = verify_expected_artifact("execute-task", "M01/S01/T01", base);
        // Still false because checkbox not marked in plan
        assert!(!result);
    }

    #[test]
    fn test_verify_hook_unit() {
        let base = Path::new("/project");

        // Hook units always pass
        let result = verify_expected_artifact("hook/some-hook", "H01", base);
        assert!(result);
    }

    #[test]
    fn test_completed_keys_path() {
        let base = Path::new("/project");
        let path = completed_keys_path(base);

        assert!(path.ends_with(".orchestra/completed-units.json"));
    }

    #[test]
    fn test_persist_and_load_completed_keys() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let base = temp_dir.path();

        // Persist some keys
        persist_completed_key(base, "execute-task/M01/S01/T01")?;
        persist_completed_key(base, "plan-slice/M01/S01")?;

        // Load them into a set
        let mut set = HashSet::new();
        load_persisted_keys(base, &mut set)?;

        assert_eq!(set.len(), 2);
        assert!(set.contains("execute-task/M01/S01/T01"));
        assert!(set.contains("plan-slice/M01/S01"));

        Ok(())
    }

    #[test]
    fn test_remove_persisted_key() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let base = temp_dir.path();

        // Persist keys
        persist_completed_key(base, "execute-task/M01/S01/T01")?;
        persist_completed_key(base, "plan-slice/M01/S01")?;

        // Remove one
        remove_persisted_key(base, "execute-task/M01/S01/T01")?;

        // Load and verify
        let mut set = HashSet::new();
        load_persisted_keys(base, &mut set)?;

        assert_eq!(set.len(), 1);
        assert!(!set.contains("execute-task/M01/S01/T01"));
        assert!(set.contains("plan-slice/M01/S01"));

        Ok(())
    }

    #[test]
    fn test_diagnose_expected_artifact() {
        let base = Path::new("/project");
        let diagnosis = diagnose_expected_artifact("execute-task", "M01/S01/T01", base);

        assert!(diagnosis.contains("T01"));
        assert!(diagnosis.contains("marked [x]"));
    }

    #[test]
    fn test_build_loop_remediation_steps() {
        let base = Path::new("/project");
        let steps = build_loop_remediation_steps("execute-task", "M01/S01/T01", base);

        assert!(steps.is_some());
        let steps = steps.unwrap();
        assert!(steps.contains("T01"));
        assert!(steps.contains("orchestra doctor"));
    }

    #[test]
    fn test_write_blocker_placeholder() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let base = temp_dir.path();

        let result = write_blocker_placeholder(
            "execute-task",
            "M01/S01/T01",
            base,
            "LLM failed to respond",
        )?;

        assert!(result.is_some());

        // Verify file was created
        let artifact_path = base
            .join(".orchestra")
            .join("milestones")
            .join("M01")
            .join("slices")
            .join("S01")
            .join("tasks")
            .join("T01-SUMMARY.md");

        assert!(artifact_path.exists());

        let content = std::fs::read_to_string(&artifact_path)?;
        assert!(content.contains("BLOCKER"));
        assert!(content.contains("LLM failed to respond"));

        Ok(())
    }
}
