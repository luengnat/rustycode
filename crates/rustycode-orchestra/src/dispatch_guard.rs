//! Orchestra Dispatch Guard — Prevents out-of-order slice dispatch
//!
//! Checks roadmap files from disk (working tree) to ensure earlier slices
//! are complete before allowing dispatch of later slices. This prevents
//! false-positive blockers when worktree branches have completed slices
//! that haven't been merged to main yet.

use crate::paths::resolve_milestone_file;
use std::path::Path;

/// Slice dispatch types that need guard checking
const SLICE_DISPATCH_TYPES: &[&str] = &[
    "research-slice",
    "plan-slice",
    "replan-slice",
    "execute-task",
    "complete-slice",
];

/// Read a roadmap file from disk (working tree)
///
/// Returns the trimmed content or null if file doesn't exist or can't be read.
///
/// # Arguments
/// * `base_path` - Project base path
/// * `milestone_id` - Milestone ID (e.g., "M01")
///
/// # Returns
/// Roadmap content or null
fn read_roadmap_from_disk(base_path: &Path, milestone_id: &str) -> Option<String> {
    let abs_path = resolve_milestone_file(base_path, milestone_id, "ROADMAP")?;

    match std::fs::read_to_string(&abs_path) {
        Ok(content) => Some(content.trim().to_string()),
        Err(_) => None,
    }
}

/// Get prior slice completion blocker
///
/// Checks if any earlier slice (in queue order) is incomplete before
/// allowing dispatch of the target unit. Returns null if no blocker,
/// or an error message describing the blocker.
///
/// # Arguments
/// * `base_path` - Project base path
/// * `main_branch` - Main branch name (unused, for compatibility)
/// * `unit_type` - Unit type being dispatched
/// * `unit_id` - Unit ID (e.g., "M01/S02")
///
/// # Returns
/// Blocker message, or null if no blocker
///
/// # Example
/// ```
/// use rustycode_orchestra::dispatch_guard::*;
///
/// // Try to dispatch M01/S02 when M01/S01 is incomplete
/// let blocker = get_prior_slice_completion_blocker(
///     "/project",
///     "main",
///     "plan-slice",
///     "M01/S02"
/// );
/// // Returns: Some("Cannot dispatch plan-slice M01/S02: earlier slice M01/S01 is not complete.")
/// ```
pub fn get_prior_slice_completion_blocker(
    base_path: &Path,
    _main_branch: &str,
    unit_type: &str,
    unit_id: &str,
) -> Option<String> {
    // Only check slice dispatch types
    if !SLICE_DISPATCH_TYPES.contains(&unit_type) {
        return None;
    }

    // Parse unit_id (format: "M01/S02" or "M01/S02/T01")
    let parts: Vec<&str> = unit_id.split('/').collect();
    if parts.len() < 2 {
        return None;
    }

    let target_mid = parts[0];
    let target_sid = parts[1];

    // Get all milestone IDs in queue order (this would come from guided-flow module)
    // For now, we'll just scan the milestones directory
    let base_path_str = base_path.to_str().unwrap_or(".");
    let milestone_ids = find_milestone_ids(base_path_str);

    let target_idx = milestone_ids.iter().position(|id| id == target_mid)?;
    let prior_milestones = &milestone_ids[..=target_idx];

    for mid in prior_milestones {
        // Read from disk (working tree) — always has the latest state
        let roadmap_content = match read_roadmap_from_disk(base_path, mid) {
            Some(content) => content,
            None => continue,
        };

        // Parse slices from roadmap content
        let slices = parse_roadmap_slices(&roadmap_content);

        if mid != target_mid {
            // Check for incomplete slices in prior milestones
            for slice in &slices {
                if !slice.done {
                    return Some(format!(
                        "Cannot dispatch {} {}: earlier slice {}/{} is not complete.",
                        unit_type, unit_id, mid, slice.id
                    ));
                }
            }
            continue;
        }

        // For target milestone, check slices before target slice
        let target_index = slices.iter().position(|s| s.id == target_sid)?;
        let prior_slices = &slices[..target_index];

        for slice in prior_slices {
            if !slice.done {
                return Some(format!(
                    "Cannot dispatch {} {}: earlier slice {}/{} is not complete.",
                    unit_type, unit_id, target_mid, slice.id
                ));
            }
        }
    }

    None
}

/// Find all milestone IDs in queue order
///
/// Scans the milestones directory and returns IDs in sorted order.
fn find_milestone_ids(base_path: &str) -> Vec<String> {
    let orchestra_dir = Path::new(base_path).join(".orchestra").join("milestones");

    let mut ids = Vec::new();

    let entries = match std::fs::read_dir(&orchestra_dir) {
        Ok(entries) => entries,
        Err(_) => return ids,
    };

    for entry in entries.flatten() {
        let file_name_os = entry.file_name();
        let file_name = match file_name_os.to_str() {
            Some(name) => name,
            None => continue,
        };

        if file_name.starts_with('M') || file_name.starts_with('m') {
            let prefix_stripped = file_name
                .strip_prefix('M')
                .or_else(|| file_name.strip_prefix('m'));

            if let Some(name) = prefix_stripped {
                if name.parse::<u32>().is_ok() && entry.path().is_dir() {
                    ids.push(file_name.to_string());
                }
            }
        }
    }

    // Sort naturally (M01, M02, ..., M10, M11, etc.)
    ids.sort();
    ids
}

/// Parse slice information from roadmap content
///
/// Extracts slice IDs and done status from roadmap markdown.
fn parse_roadmap_slices(content: &str) -> Vec<RoadmapSlice> {
    let mut slices = Vec::new();

    // Look for slice sections in format: "## [ ] S01: Slice Name"
    let slice_re = regex::Regex::new(r"^##\s*\[[x\s]\]\s+S(\d+):\s*(.+)$").unwrap();

    for line in content.lines() {
        if let Some(caps) = slice_re.captures(line) {
            if let Ok(num) = caps[1].parse::<u32>() {
                slices.push(RoadmapSlice {
                    id: format!("S{:02}", num),
                    done: line.contains("[x]"),
                    name: caps[2].trim().to_string(),
                });
            }
        }
    }

    slices
}

/// Roadmap slice information
#[derive(Debug, Clone)]
struct RoadmapSlice {
    id: String,
    done: bool,
    #[allow(dead_code)] // Kept for future use
    name: String, // Reserved for future display purposes
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_slice_dispatch_types() {
        assert!(SLICE_DISPATCH_TYPES.contains(&"plan-slice"));
        assert!(SLICE_DISPATCH_TYPES.contains(&"execute-task"));
        assert!(!SLICE_DISPATCH_TYPES.contains(&"research-milestone"));
    }

    #[test]
    fn test_parse_roadmap_slices() {
        let content = r#"# Roadmap

## [ ] S01: First slice
## [x] S02: Second slice
## [ ] S03: Third slice
"#;

        let slices = parse_roadmap_slices(content);
        assert_eq!(slices.len(), 3);
        assert_eq!(slices[0].id, "S01");
        assert!(!slices[0].done);
        assert_eq!(slices[1].id, "S02");
        assert!(slices[1].done);
    }

    #[test]
    fn test_get_prior_slice_completion_blocker_no_blocker() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_dir = base_path.join(".orchestra").join("milestones");
        let m01_dir = orchestra_dir.join("M01");

        std::fs::create_dir_all(&m01_dir).unwrap();

        let roadmap_path = m01_dir.join("M01-ROADMAP.MD");
        std::fs::write(
            &roadmap_path,
            r#"# Roadmap M01

## [x] S01: First slice
## [x] S02: Second slice
## [ ] S03: Third slice
"#,
        )
        .unwrap();

        // Clear path cache so the new file is visible
        crate::paths::clear_path_cache();

        // All prior slices complete, no blocker
        let blocker =
            get_prior_slice_completion_blocker(base_path, "main", "plan-slice", "M01/S03");
        assert!(blocker.is_none());
    }

    #[test]
    fn test_get_prior_slice_completion_blocker_with_blocker() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_dir = base_path.join(".orchestra").join("milestones");
        let m01_dir = orchestra_dir.join("M01");

        std::fs::create_dir_all(&m01_dir).unwrap();

        let roadmap_path = m01_dir.join("M01-ROADMAP.MD");
        std::fs::write(
            &roadmap_path,
            r#"# Roadmap M01

## [x] S01: First slice
## [ ] S02: Second slice
## [ ] S03: Third slice
"#,
        )
        .unwrap();

        // Clear path cache so the new file is visible
        crate::paths::clear_path_cache();

        // S02 is incomplete, should block S03
        let blocker =
            get_prior_slice_completion_blocker(base_path, "main", "complete-slice", "M01/S03");

        assert!(blocker.is_some());
        assert!(blocker.unwrap().contains("S02 is not complete"));
    }

    #[test]
    fn test_get_prior_slice_completion_blocker_non_slice() {
        let temp_dir = TempDir::new().unwrap();

        // Non-slice unit types should return None
        let blocker = get_prior_slice_completion_blocker(
            temp_dir.path(),
            "main",
            "research-milestone",
            "M01",
        );
        assert!(blocker.is_none());
    }

    #[test]
    fn test_get_prior_slice_completion_blocker_invalid_unit_id() {
        let temp_dir = TempDir::new().unwrap();

        // Invalid unit ID format
        let blocker =
            get_prior_slice_completion_blocker(temp_dir.path(), "main", "plan-slice", "invalid");
        assert!(blocker.is_none());
    }
}
