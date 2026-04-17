//! Orchestra Doctor Format — Doctor report formatting utilities
//!
//! Provides formatting functions for doctor reports and issues.

use crate::doctor_types::{
    DoctorIssue, DoctorIssueCodeCount, DoctorReport, DoctorSeverity, DoctorSummary,
};

/// Check if unit ID matches scope
///
/// # Arguments
/// * `unit_id` - Unit ID (e.g., "M01", "M01/S01", "M01/S01/T01")
/// * `scope` - Optional scope to match against
///
/// # Returns
/// true if unit_id matches or is within scope
///
/// # Example
/// ```
/// use rustycode_orchestra::doctor_format::*;
///
/// assert!(matches_scope("M01/S01", Some("M01")));
/// assert!(matches_scope("M01/S01", None));
/// assert!(!matches_scope("M02/S01", Some("M01")));
/// ```
pub fn matches_scope(unit_id: &str, scope: Option<&str>) -> bool {
    match scope {
        None => true,
        Some(s) => {
            unit_id == s || unit_id.starts_with(&format!("{}/", s)) || unit_id.starts_with(s)
        }
    }
}

/// Summarize doctor issues into statistics
///
/// # Arguments
/// * `issues` - List of doctor issues
///
/// # Returns
/// Doctor summary with counts and issue code breakdown
///
/// # Example
/// ```
/// use rustycode_orchestra::doctor_format::*;
/// use rustycode_orchestra::doctor_types::*;
///
/// let issues = vec![
///     DoctorIssue {
///         severity: DoctorSeverity::Error,
///         code: DoctorIssueCode::StateFileMissing,
///         scope: crate::doctor_types::DoctorScope::Project,
///         unit_id: "PROJECT".to_string(),
///         message: "State file missing".to_string(),
///         file: Some("/path/to/file".to_string()),
///         fixable: true,
///     },
/// ];
///
/// let summary = summarize_doctor_issues(&issues);
/// assert_eq!(summary.total, 1);
/// assert_eq!(summary.errors, 1);
/// ```
pub fn summarize_doctor_issues(issues: &[DoctorIssue]) -> DoctorSummary {
    let errors = issues
        .iter()
        .filter(|i| i.severity == DoctorSeverity::Error)
        .count();
    let warnings = issues
        .iter()
        .filter(|i| i.severity == DoctorSeverity::Warning)
        .count();
    let infos = issues
        .iter()
        .filter(|i| i.severity == DoctorSeverity::Info)
        .count();
    let fixable = issues.iter().filter(|i| i.fixable).count();

    let mut by_code_map = std::collections::HashMap::new();
    for issue in issues {
        *by_code_map.entry(issue.code.clone()).or_insert(0) += 1;
    }

    let mut by_code: Vec<_> = by_code_map
        .into_iter()
        .map(|(code, count)| DoctorIssueCodeCount { code, count })
        .collect();

    by_code.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.code.as_str().cmp(b.code.as_str()))
    });

    DoctorSummary {
        total: issues.len(),
        errors,
        warnings,
        infos,
        fixable,
        by_code,
    }
}

/// Filter doctor issues by options
///
/// # Arguments
/// * `issues` - List of doctor issues
/// * `scope` - Optional scope filter
/// * `include_warnings` - Whether to include warnings (default: false)
///
/// # Returns
/// Filtered list of issues
///
/// # Example
/// ```
/// use rustycode_orchestra::doctor_format::*;
/// use rustycode_orchestra::doctor_types::*;
///
/// let issues = vec![/* ... */];
///
/// // Only errors in M01
/// let filtered = filter_doctor_issues(&issues, Some("M01"), false);
///
/// // All issues in M01
/// let all = filter_doctor_issues(&issues, Some("M01"), true);
/// ```
pub fn filter_doctor_issues<'a>(
    issues: &'a [DoctorIssue],
    scope: Option<&str>,
    include_warnings: bool,
) -> Vec<&'a DoctorIssue> {
    let mut filtered: Vec<&DoctorIssue> = issues.iter().collect();

    if let Some(s) = scope {
        filtered.retain(|issue| matches_scope(&issue.unit_id, Some(s)));
    }

    if !include_warnings {
        filtered.retain(|issue| issue.severity == DoctorSeverity::Error);
    }

    filtered
}

/// Format doctor report as human-readable text
///
/// # Arguments
/// * `report` - Doctor report
/// * `scope` - Optional scope filter
/// * `include_warnings` - Whether to include warnings (default: true)
/// * `max_issues` - Maximum issues to display (default: 12)
/// * `title` - Optional title for the report
///
/// # Returns
/// Formatted report string
///
/// # Example
/// ```
/// use rustycode_orchestra::doctor_format::*;
/// use rustycode_orchestra::doctor_types::*;
///
/// let report = DoctorReport {
///     ok: false,
///     base_path: "/project".to_string(),
///     issues: vec![],
///     fixes_applied: vec![],
/// };
///
/// let text = format_doctor_report(&report, None, true, 12, None);
/// println!("{}", text);
/// ```
pub fn format_doctor_report(
    report: &DoctorReport,
    scope: Option<&str>,
    include_warnings: bool,
    max_issues: usize,
    title: Option<&str>,
) -> String {
    let scoped_issues_refs = filter_doctor_issues(&report.issues, scope, include_warnings);
    let scoped_issues: Vec<_> = scoped_issues_refs.into_iter().cloned().collect();
    let summary = summarize_doctor_issues(&scoped_issues);

    let mut lines = Vec::new();

    let default_title = if summary.errors > 0 {
        "Orchestra doctor found blocking issues."
    } else {
        "Orchestra doctor report."
    };
    lines.push(title.unwrap_or(default_title).to_string());

    lines.push(format!("Scope: {}", scope.unwrap_or("all milestones")));
    lines.push(format!(
        "Issues: {} total · {} error(s) · {} warning(s) · {} fixable",
        summary.total, summary.errors, summary.warnings, summary.fixable
    ));

    if !summary.by_code.is_empty() {
        lines.push("Top issue types:".to_string());
        for item in summary.by_code.iter().take(5) {
            lines.push(format!("- {}: {}", item.code.as_str(), item.count));
        }
    }

    if !scoped_issues.is_empty() {
        lines.push("Priority issues:".to_string());
        for issue in scoped_issues.iter().take(max_issues) {
            let prefix = match issue.severity {
                DoctorSeverity::Error => "ERROR",
                DoctorSeverity::Warning => "WARN",
                DoctorSeverity::Info => "INFO",
            };
            lines.push(format!(
                "- [{}] {}: {}{}",
                prefix,
                issue.unit_id,
                issue.message,
                if let Some(file) = &issue.file {
                    format!(" ({})", file)
                } else {
                    String::new()
                }
            ));
        }
        if scoped_issues.len() > max_issues {
            lines.push(format!(
                "- ...and {} more in scope",
                scoped_issues.len() - max_issues
            ));
        }
    }

    if !report.fixes_applied.is_empty() {
        lines.push("Fixes applied:".to_string());
        for fix in report.fixes_applied.iter().take(max_issues) {
            lines.push(format!("- {}", fix));
        }
        if report.fixes_applied.len() > max_issues {
            lines.push(format!(
                "- ...and {} more",
                report.fixes_applied.len() - max_issues
            ));
        }
    }

    lines.join("\n")
}

/// Format doctor issues for LLM prompt
///
/// # Arguments
/// * `issues` - List of doctor issues
///
/// # Returns
/// Formatted issues string for prompt inclusion
///
/// # Example
/// ```
/// use rustycode_orchestra::doctor_format::*;
///
/// let issues = vec![];
/// let text = format_doctor_issues_for_prompt(&issues);
/// assert_eq!(text, "- No remaining issues in scope.");
/// ```
pub fn format_doctor_issues_for_prompt(issues: &[DoctorIssue]) -> String {
    if issues.is_empty() {
        return "- No remaining issues in scope.".to_string();
    }

    issues
        .iter()
        .map(|issue| {
            let prefix = match issue.severity {
                DoctorSeverity::Error => "ERROR",
                DoctorSeverity::Warning => "WARN",
                DoctorSeverity::Info => "INFO",
            };

            format!(
                "- [{}] {} | {} | {}{} | fixable: {}",
                prefix,
                issue.unit_id,
                issue.code.as_str(),
                issue.message,
                if let Some(file) = &issue.file {
                    format!(" | file: {}", file)
                } else {
                    String::new()
                },
                if issue.fixable { "yes" } else { "no" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor_types::{DoctorIssueCode, DoctorScope};

    fn create_test_issue(
        severity: DoctorSeverity,
        code: DoctorIssueCode,
        unit_id: &str,
        message: &str,
    ) -> DoctorIssue {
        DoctorIssue {
            severity,
            code,
            scope: DoctorScope::Project,
            unit_id: unit_id.to_string(),
            message: message.to_string(),
            file: None,
            fixable: true,
        }
    }

    #[test]
    fn test_matches_scope() {
        assert!(matches_scope("M01/S01", Some("M01")));
        assert!(matches_scope("M01/S01", None));
        assert!(!matches_scope("M02/S01", Some("M01")));
        assert!(matches_scope("M01/S01/T01", Some("M01")));
        assert!(matches_scope("M01", Some("M01")));
    }

    #[test]
    fn test_summarize_doctor_issues() {
        let issues = vec![
            create_test_issue(
                DoctorSeverity::Error,
                DoctorIssueCode::StateFileMissing,
                "PROJECT",
                "Missing state file",
            ),
            create_test_issue(
                DoctorSeverity::Warning,
                DoctorIssueCode::StaleCrashLock,
                "PROJECT",
                "Stale crash lock",
            ),
            create_test_issue(
                DoctorSeverity::Info,
                DoctorIssueCode::GitignoreMissingPatterns,
                "PROJECT",
                "Gitignore missing patterns",
            ),
            create_test_issue(
                DoctorSeverity::Error,
                DoctorIssueCode::StateFileStale,
                "PROJECT",
                "State file stale",
            ),
        ];

        let summary = summarize_doctor_issues(&issues);

        assert_eq!(summary.total, 4);
        assert_eq!(summary.errors, 2);
        assert_eq!(summary.warnings, 1);
        assert_eq!(summary.infos, 1);
        assert_eq!(summary.fixable, 4);
        assert_eq!(summary.by_code.len(), 4); // All 4 issues have unique codes
    }

    #[test]
    fn test_filter_doctor_issues() {
        let issues = vec![
            create_test_issue(
                DoctorSeverity::Error,
                DoctorIssueCode::StateFileMissing,
                "M01",
                "Error 1",
            ),
            create_test_issue(
                DoctorSeverity::Warning,
                DoctorIssueCode::StaleCrashLock,
                "M01",
                "Warning 1",
            ),
            create_test_issue(
                DoctorSeverity::Error,
                DoctorIssueCode::StateFileStale,
                "M02",
                "Error 2",
            ),
        ];

        // Only errors in M01
        let filtered = filter_doctor_issues(&issues, Some("M01"), false);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].unit_id, "M01");

        // All issues in M01
        let all = filter_doctor_issues(&issues, Some("M01"), true);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_format_doctor_report() {
        let report = DoctorReport {
            ok: false,
            base_path: "/project".to_string(),
            issues: vec![create_test_issue(
                DoctorSeverity::Error,
                DoctorIssueCode::StateFileMissing,
                "PROJECT",
                "Missing state file",
            )],
            fixes_applied: vec!["Fixed issue 1".to_string()],
        };

        let text = format_doctor_report(&report, None, true, 12, None);

        assert!(text.contains("Orchestra doctor found blocking issues"));
        assert!(text.contains("Issues: 1 total · 1 error(s) · 0 warning(s) · 1 fixable"));
        assert!(text.contains("Priority issues:"));
        assert!(text.contains("[ERROR] PROJECT: Missing state file"));
        assert!(text.contains("Fixes applied:"));
        assert!(text.contains("Fixed issue 1"));
    }

    #[test]
    fn test_format_doctor_issues_for_prompt() {
        let issues = vec![create_test_issue(
            DoctorSeverity::Error,
            DoctorIssueCode::StateFileMissing,
            "PROJECT",
            "Missing state file",
        )];

        let text = format_doctor_issues_for_prompt(&issues);

        assert!(text.contains("[ERROR]"));
        assert!(text.contains("PROJECT |"));
        assert!(text.contains("state_file_missing"));
        assert!(text.contains("Missing state file"));
        assert!(text.contains("fixable: yes"));
    }

    #[test]
    fn test_format_doctor_issues_for_prompt_empty() {
        let issues = vec![];
        let text = format_doctor_issues_for_prompt(&issues);
        assert_eq!(text, "- No remaining issues in scope.");
    }

    #[test]
    fn test_format_doctor_report_empty() {
        let report = DoctorReport {
            ok: true,
            base_path: "/project".to_string(),
            issues: vec![],
            fixes_applied: vec![],
        };

        let text = format_doctor_report(&report, None, true, 12, None);

        assert!(text.contains("Orchestra doctor report."));
        assert!(text.contains("Issues: 0 total · 0 error(s) · 0 warning(s) · 0 fixable"));
    }

    #[test]
    fn test_format_doctor_report_max_issues() {
        let issues = (0..20)
            .map(|i| {
                create_test_issue(
                    DoctorSeverity::Error,
                    DoctorIssueCode::StateFileMissing,
                    &format!("M{:02}", i / 10 + 1),
                    &format!("Issue {}", i),
                )
            })
            .collect();

        let report = DoctorReport {
            ok: false,
            base_path: "/project".to_string(),
            issues,
            fixes_applied: vec![],
        };

        let text = format_doctor_report(&report, None, true, 5, None);

        // Should only show 5 issues (Issues 0-4)
        assert!(text.contains("[ERROR] M01: Issue 0"));
        assert!(text.contains("[ERROR] M01: Issue 1"));
        assert!(text.contains("[ERROR] M01: Issue 2"));
        assert!(text.contains("[ERROR] M01: Issue 3"));
        assert!(text.contains("[ERROR] M01: Issue 4"));
        assert!(text.contains("...and 15 more in scope"));
    }
}
