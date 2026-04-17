//! Orchestra Auto Observability — Pre-Dispatch Observability Checks
//!
//! Collects observability warnings and builds repair instructions
//! for the agent to fix gaps before proceeding with the unit.
//!
//! Critical for ensuring autonomous development produces observable,
//! debuggable, and maintainable systems.

use std::path::Path;

use crate::observability_validator::{
    validate_complete_boundary, validate_execute_boundary, validate_plan_boundary, ValidationIssue,
};

// ─── Public API ────────────────────────────────────────────────────────────────

/// Collect observability warnings for a unit
///
/// Validates plan/summary file quality and builds repair instructions.
/// Hook units have custom artifacts — skip standard observability checks.
///
/// # Arguments
/// * `base_path` - Project base path
/// * `unit_type` - The unit type
/// * `unit_id` - The unit ID (e.g. "M01/S01/T01")
///
/// # Returns
/// List of validation issues
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_observability::*;
///
/// let issues = collect_observability_warnings(
///     "/project",
///     "execute-task",
///     "M01/S01/T01"
/// );
/// ```
pub fn collect_observability_warnings(
    base_path: &str,
    unit_type: &str,
    unit_id: &str,
) -> Vec<ValidationIssue> {
    // Hook units have custom artifacts — skip standard observability checks
    if unit_type.starts_with("hook/") {
        return Vec::new();
    }

    let parts: Vec<&str> = unit_id.split('/').collect();
    let mid = parts.first().copied();
    let sid = parts.get(1).copied();
    let tid = parts.get(2).copied();

    if mid.is_none() || sid.is_none() {
        return Vec::new();
    }

    match unit_type {
        "plan-slice" => validate_plan_boundary(base_path, mid.unwrap(), sid.unwrap()),
        "execute-task" => {
            if let Some(task_id) = tid {
                validate_execute_boundary(base_path, mid.unwrap(), sid.unwrap(), task_id)
            } else {
                Vec::new()
            }
        }
        "complete-slice" => validate_complete_boundary(base_path, mid.unwrap(), sid.unwrap()),
        _ => Vec::new(),
    }
}

/// Build observability repair block for issues
///
/// Creates a markdown block that explains the issues and how to fix them,
/// to be injected into the agent's prompt before unit execution.
///
/// # Arguments
/// * `issues` - List of validation issues
///
/// # Returns
/// Markdown repair block, or empty string if no issues
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_observability::*;
/// use rustycode_orchestra::observability_validator::ValidationIssue;
///
/// let issues = vec![
///     ValidationIssue {
///         severity: ValidationSeverity::Warning,
///         scope: ValidationScope::SlicePlan,
///         file: "/test/PLAN.md".to_string(),
///         rule_id: "test".to_string(),
///         message: "Test message".to_string(),
///         suggestion: Some("Fix it".to_string()),
///     }
/// ];
///
/// let block = build_observability_repair_block(&issues);
/// assert!(block.contains("Pre-flight: Observability gaps"));
/// ```
pub fn build_observability_repair_block(issues: &[ValidationIssue]) -> String {
    if issues.is_empty() {
        return String::new();
    }

    let items: Vec<String> = issues
        .iter()
        .map(|issue| {
            let file_name = Path::new(&issue.file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&issue.file);

            let mut line = format!("- **{}**: {}", file_name, issue.message);
            if let Some(suggestion) = &issue.suggestion {
                line.push_str(&format!(" → {}", suggestion));
            }
            line
        })
        .collect();

    let items_str = items.join("\n");

    format!(
        "\n---\n\n## Pre-flight: Observability gaps to fix FIRST\n\nThe following issues were detected in plan/summary files for this unit.\n**Read each flagged file, apply the fix described, then proceed with the unit.**\n\n{}\n\n---\n\n",
        items_str
    )
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability_validator::{ValidationIssue, ValidationScope, ValidationSeverity};

    #[test]
    fn test_collect_observability_warnings_hook() {
        let issues = collect_observability_warnings("/nonexistent", "hook/pre-commit", "H01");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_collect_observability_warnings_invalid_id() {
        let issues = collect_observability_warnings("/nonexistent", "execute-task", "invalid");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_collect_observability_warnings_unknown_unit() {
        let issues = collect_observability_warnings("/nonexistent", "unknown-unit", "M01/S01");
        assert!(issues.is_empty());
    }

    #[test]
    fn test_build_observability_repair_block_empty() {
        let block = build_observability_repair_block(&[]);
        assert_eq!(block, "");
    }

    #[test]
    fn test_build_observability_repair_block_with_issues() {
        let issues = vec![ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::SlicePlan,
            file: "/test/PLAN.md".to_string(),
            rule_id: "test".to_string(),
            message: "Missing section".to_string(),
            suggestion: Some("Add section".to_string()),
        }];

        let block = build_observability_repair_block(&issues);
        assert!(block.contains("Pre-flight: Observability gaps"));
        assert!(block.contains("PLAN.md"));
        assert!(block.contains("Missing section"));
        assert!(block.contains("Add section"));
    }

    #[test]
    fn test_build_observability_repair_block_no_suggestion() {
        let issues = vec![ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::SlicePlan,
            file: "/test/PLAN.md".to_string(),
            rule_id: "test".to_string(),
            message: "Test message".to_string(),
            suggestion: None,
        }];

        let block = build_observability_repair_block(&issues);
        assert!(block.contains("Test message"));
        assert!(!block.contains("→"));
    }
}
