//! Orchestra Queue Order — Custom milestone execution ordering.
//!
//! Stores an explicit execution order in `.orchestra/QUEUE-ORDER.json`.
//! When present, milestone ID sorting uses this order instead of
//! the default numeric sort (milestone_id_sort).
//!
//! The file is committed to git (not gitignored) so ordering
//! survives branch switches and is shared across sessions.
//!
//! Matches orchestra-2's queue-order.ts implementation.

use crate::error::{OrchestraV2Error, Result};
use crate::milestone_ids::milestone_id_sort;
use crate::paths::orchestra_root;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

// ─── Types ───────────────────────────────────────────────────────────────────

/// Queue order file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueueOrderFile {
    order: Vec<String>,
    updated_at: String,
}

/// Dependency violation type
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DependencyViolationType {
    /// A milestone is placed before one of its dependencies
    WouldBlock,
    /// Two or more milestones form a dependency cycle
    Circular,
    /// A milestone depends on an ID that doesn't exist
    MissingDep,
}

/// A dependency violation
#[derive(Debug, Clone)]
pub struct DependencyViolation {
    pub milestone: String,
    pub depends_on: String,
    pub violation_type: DependencyViolationType,
    pub message: String,
}

/// A redundant dependency (satisfied by queue position)
#[derive(Debug, Clone)]
pub struct DependencyRedundancy {
    pub milestone: String,
    pub depends_on: String,
}

/// Dependency validation result
#[derive(Debug, Clone)]
pub struct DependencyValidation {
    pub valid: bool,
    pub violations: Vec<DependencyViolation>,
    pub redundant: Vec<DependencyRedundancy>,
}

// ─── Path ────────────────────────────────────────────────────────────────────

/// Get the path to the queue order file
fn queue_order_path(base_path: &Path) -> std::path::PathBuf {
    orchestra_root(base_path).join("QUEUE-ORDER.json")
}

// ─── Read / Write ────────────────────────────────────────────────────────────

/// Load the custom queue order.
///
/// Returns None if no file exists or if the file is corrupt/unreadable.
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// Custom queue order, or None if not found
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::queue_order::*;
///
/// let order = load_queue_order(Path::new("/my/project"));
/// if let Some(custom_order) = order {
///     println!("Custom order: {:?}", custom_order);
/// }
/// ```
pub fn load_queue_order(base_path: &Path) -> Option<Vec<String>> {
    let path = queue_order_path(base_path);
    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(&path).ok()?;
    let data: QueueOrderFile = serde_json::from_str(&content).ok()?;

    Some(data.order)
}

/// Save a custom queue order to disk.
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `order` - Order of milestone IDs
///
/// # Returns
/// Ok(()) on success, Err on failure
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::queue_order::*;
///
/// save_queue_order(
///     Path::new("/my/project"),
///     &vec!["M003".to_string(), "M001".to_string(), "M002".to_string()],
/// )?;
/// ```
pub fn save_queue_order(base_path: &Path, order: &[String]) -> Result<()> {
    let data = QueueOrderFile {
        order: order.to_vec(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    let json = serde_json::to_string_pretty(&data).map_err(|e| {
        OrchestraV2Error::Serialization(format!("Failed to serialize queue order: {}", e))
    })?;
    let path = queue_order_path(base_path);

    fs::write(&path, json + "\n").map_err(OrchestraV2Error::Io)?;

    Ok(())
}

// ─── Sorting ─────────────────────────────────────────────────────────────────

/// Sort milestone IDs respecting a custom order.
///
/// - IDs present in `custom_order` appear in that exact sequence.
/// - IDs on disk but NOT in `custom_order` are appended at the end,
///   sorted by the default `milestone_id_sort` (numeric).
/// - IDs in `custom_order` but NOT on disk are silently skipped.
/// - When `custom_order` is None, falls back to `milestone_id_sort`.
///
/// # Arguments
/// * `ids` - Milestone IDs to sort
/// * `custom_order` - Optional custom order (None = use default sort)
///
/// # Returns
/// Sorted milestone IDs
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::queue_order::*;
///
/// let ids = vec!["M001".to_string(), "M002".to_string(), "M003".to_string()];
/// let custom = Some(vec!["M003".to_string(), "M001".to_string()]);
/// let sorted = sort_by_queue_order(&ids, custom.as_deref());
/// // Result: ["M003", "M001", "M002"]
/// ```
pub fn sort_by_queue_order(ids: &[String], custom_order: Option<&[String]>) -> Vec<String> {
    let custom_order = match custom_order {
        Some(order) if !order.is_empty() => order,
        _ => {
            // Fall back to default sort
            let mut sorted = ids.to_vec();
            sorted.sort_by(|a, b| milestone_id_sort(a, b));
            return sorted;
        }
    };

    let id_set: HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    let mut ordered = Vec::new();

    // First: IDs from custom_order that exist on disk
    for id in custom_order {
        if id_set.contains(id.as_str()) {
            ordered.push(id.clone());
        }
    }

    // Then: remaining IDs not in custom_order, in default sort order
    let mut remaining: Vec<&String> = ids.iter().filter(|id| !custom_order.contains(id)).collect();

    remaining.sort_by(|a, b| milestone_id_sort(a, b));

    for id in remaining {
        ordered.push(id.clone());
    }

    ordered
}

// ─── Pruning ─────────────────────────────────────────────────────────────────

/// Remove IDs from the queue order file that are no longer valid
/// (completed or deleted milestones). No-op if file doesn't exist.
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `valid_ids` - Set of valid milestone IDs
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::queue_order::*;
///
/// let valid_ids = vec!["M001".to_string(), "M003".to_string()];
/// prune_queue_order(Path::new("/my/project"), &valid_ids);
/// ```
pub fn prune_queue_order(base_path: &Path, valid_ids: &[String]) {
    let order = match load_queue_order(base_path) {
        Some(o) => o,
        None => return,
    };

    let original_len = order.len();
    let valid_set: HashSet<&str> = valid_ids.iter().map(|s| s.as_str()).collect();
    let pruned: Vec<String> = order
        .into_iter()
        .filter(|id| valid_set.contains(id.as_str()))
        .collect();

    if pruned.len() != original_len {
        let _ = save_queue_order(base_path, &pruned);
    }
}

// ─── Validation ──────────────────────────────────────────────────────────────

/// Validate a proposed queue order against dependency constraints.
///
/// Checks:
/// - would_block: A milestone is placed before one of its dependencies
/// - circular: Two or more milestones form a dependency cycle
/// - missing_dep: A milestone depends on an ID that doesn't exist
/// - redundant: A dependency is satisfied by queue position (dep comes earlier)
///
/// # Arguments
/// * `order` - Proposed queue order
/// * `deps_map` - Map of milestone ID to its dependencies
/// * `completed_ids` - Set of already-completed milestone IDs
///
/// # Returns
/// Validation result with violations and redundancies
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::queue_order::*;
/// use std::collections::HashMap;
///
/// let order = vec!["M001".to_string(), "M002".to_string()];
/// let mut deps = HashMap::new();
/// deps.insert("M002".to_string(), vec!["M001".to_string()]);
/// let completed = HashSet::new();
///
/// let validation = validate_queue_order(&order, &deps, &completed);
/// assert!(validation.valid);
/// ```
pub fn validate_queue_order(
    order: &[String],
    deps_map: &HashMap<String, Vec<String>>,
    completed_ids: &HashSet<&str>,
) -> DependencyValidation {
    let mut violations = Vec::new();
    let mut redundant = Vec::new();

    let position_map: HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let all_known_ids: HashSet<&str> = order
        .iter()
        .map(|s| s.as_str())
        .chain(completed_ids.iter().copied())
        .collect();

    // First, check for circular dependencies
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();
    let mut has_circular_dep = false;

    fn has_cycle(
        node: &str,
        path: &[&str],
        deps_map: &HashMap<String, Vec<String>>,
        completed_ids: &HashSet<&str>,
        visited: &mut HashSet<String>,
        in_stack: &mut HashSet<String>,
    ) -> Option<Vec<String>> {
        if in_stack.contains(node) {
            let mut cycle: Vec<String> = path.iter().map(|s| s.to_string()).collect();
            cycle.push(node.to_string());
            return Some(cycle);
        }
        if visited.contains(node) {
            return None;
        }

        visited.insert(node.to_string());
        in_stack.insert(node.to_string());

        let deps = deps_map.get(node).cloned().unwrap_or_default();
        for dep in deps {
            if completed_ids.contains(dep.as_str()) {
                continue;
            }
            let mut new_path = path.to_vec();
            new_path.push(node);
            if let Some(cycle) =
                has_cycle(&dep, &new_path, deps_map, completed_ids, visited, in_stack)
            {
                return Some(cycle);
            }
        }

        in_stack.remove(node);
        None
    }

    for mid in order {
        let mid_str = mid.as_str();
        if !visited.contains(mid_str) {
            if let Some(cycle) = has_cycle(
                mid_str,
                &[],
                deps_map,
                completed_ids,
                &mut visited,
                &mut in_stack,
            ) {
                let cycle_str = cycle.join(" → ");
                let milestone = cycle
                    .first()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "?".to_string());
                let depends_on = cycle
                    .get(cycle.len().saturating_sub(2))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "?".to_string());
                violations.push(DependencyViolation {
                    milestone,
                    depends_on,
                    violation_type: DependencyViolationType::Circular,
                    message: format!("Circular dependency: {}", cycle_str),
                });
                has_circular_dep = true;
                break; // one cycle report is enough
            }
        }
    }

    // Only check for blocking/missing dependencies if no circular dependencies found
    if !has_circular_dep {
        for (mid, deps) in deps_map {
            let mid_pos = match position_map.get(mid.as_str()) {
                Some(&pos) => pos,
                None => continue, // not in pending order
            };

            for dep in deps {
                // Dep already completed — always satisfied
                if completed_ids.contains(dep.as_str()) {
                    continue;
                }

                // Dep doesn't exist anywhere
                if !all_known_ids.contains(dep.as_str()) {
                    violations.push(DependencyViolation {
                        milestone: mid.clone(),
                        depends_on: dep.clone(),
                        violation_type: DependencyViolationType::MissingDep,
                        message: format!("{} depends on {}, but {} does not exist.", mid, dep, dep),
                    });
                    continue;
                }

                let dep_pos = match position_map.get(dep.as_str()) {
                    Some(&pos) => pos,
                    None => continue, // dep not in pending order
                };

                if dep_pos > mid_pos {
                    // Dep comes AFTER this milestone in the order — violation
                    violations.push(DependencyViolation {
                        milestone: mid.clone(),
                        depends_on: dep.clone(),
                        violation_type: DependencyViolationType::WouldBlock,
                        message: format!(
                            "{} cannot run before {} — {} depends_on: [{}].",
                            mid, dep, mid, dep
                        ),
                    });
                } else {
                    // Dep comes before — satisfied by position, redundant
                    redundant.push(DependencyRedundancy {
                        milestone: mid.clone(),
                        depends_on: dep.clone(),
                    });
                }
            }
        }
    }

    let valid = violations.is_empty();

    DependencyValidation {
        valid,
        violations,
        redundant,
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_by_queue_order_none() {
        let ids = vec!["M003".to_string(), "M001".to_string(), "M002".to_string()];

        let sorted = sort_by_queue_order(&ids, None);

        assert_eq!(sorted, vec!["M001", "M002", "M003"]);
    }

    #[test]
    fn test_sort_by_queue_order_custom() {
        let ids = vec!["M001".to_string(), "M002".to_string(), "M003".to_string()];

        let custom = vec!["M003".to_string(), "M001".to_string()];

        let sorted = sort_by_queue_order(&ids, Some(&custom));

        assert_eq!(sorted, vec!["M003", "M001", "M002"]);
    }

    #[test]
    fn test_sort_by_queue_order_skip_invalid() {
        let ids = vec!["M001".to_string(), "M002".to_string()];

        let custom = vec![
            "M003".to_string(), // Not in ids
            "M002".to_string(),
            "M001".to_string(),
        ];

        let sorted = sort_by_queue_order(&ids, Some(&custom));

        assert_eq!(sorted, vec!["M002", "M001"]);
    }

    #[test]
    fn test_validate_queue_order_valid() {
        let order = vec!["M001".to_string(), "M002".to_string()];

        let mut deps = HashMap::new();
        deps.insert("M002".to_string(), vec!["M001".to_string()]);

        let completed = HashSet::new();

        let validation = validate_queue_order(&order, &deps, &completed);

        assert!(validation.valid);
        assert!(validation.violations.is_empty());
        assert_eq!(validation.redundant.len(), 1);
    }

    #[test]
    fn test_validate_queue_order_would_block() {
        let order = vec!["M002".to_string(), "M001".to_string()];

        let mut deps = HashMap::new();
        deps.insert("M002".to_string(), vec!["M001".to_string()]);

        let completed = HashSet::new();

        let validation = validate_queue_order(&order, &deps, &completed);

        assert!(!validation.valid);
        assert_eq!(validation.violations.len(), 1);
        assert_eq!(
            validation.violations[0].violation_type,
            DependencyViolationType::WouldBlock
        );
    }

    #[test]
    fn test_validate_queue_order_missing_dep() {
        let order = vec!["M001".to_string()];

        let mut deps = HashMap::new();
        deps.insert("M001".to_string(), vec!["M999".to_string()]); // Doesn't exist

        let completed = HashSet::new();

        let validation = validate_queue_order(&order, &deps, &completed);

        assert!(!validation.valid);
        assert_eq!(validation.violations.len(), 1);
        assert_eq!(
            validation.violations[0].violation_type,
            DependencyViolationType::MissingDep
        );
    }

    #[test]
    fn test_validate_queue_order_completed_dep() {
        let order = vec!["M002".to_string()];

        let mut deps = HashMap::new();
        deps.insert("M002".to_string(), vec!["M001".to_string()]);

        let mut completed = HashSet::new();
        completed.insert("M001");

        let validation = validate_queue_order(&order, &deps, &completed);

        assert!(validation.valid);
        assert!(validation.violations.is_empty());
        assert!(validation.redundant.is_empty()); // Completed deps aren't redundant
    }

    #[test]
    fn test_validate_queue_order_circular() {
        let order = vec!["M001".to_string(), "M002".to_string()];

        let mut deps = HashMap::new();
        deps.insert("M001".to_string(), vec!["M002".to_string()]);
        deps.insert("M002".to_string(), vec!["M001".to_string()]);

        let completed = HashSet::new();

        let validation = validate_queue_order(&order, &deps, &completed);

        assert!(!validation.valid);
        assert_eq!(validation.violations.len(), 1);
        assert_eq!(
            validation.violations[0].violation_type,
            DependencyViolationType::Circular
        );
    }

    #[test]
    fn test_validate_queue_order_no_deps() {
        let order = vec!["M001".to_string(), "M002".to_string()];

        let deps = HashMap::new();
        let completed = HashSet::new();

        let validation = validate_queue_order(&order, &deps, &completed);

        assert!(validation.valid);
        assert!(validation.violations.is_empty());
        assert!(validation.redundant.is_empty());
    }
}
