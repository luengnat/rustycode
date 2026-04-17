//! Orchestra Observability Validator — Pre-Dispatch Quality Checks
//!
//! Validates plan/summary file quality and builds repair instructions
//! for the agent to fix gaps before proceeding with the unit.
//!
//! Critical for ensuring autonomous development has proper observability,
//! verification, and diagnostic surfaces.

use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

use crate::files::load_file;
use crate::paths::{resolve_slice_file, resolve_task_file, resolve_task_files, resolve_tasks_dir};

// ─── Regex Patterns ─────────────────────────────────────────────────────────────

/// Pattern to match task checkboxes in slice plans
static TASK_LINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^- \[[ x]\] \*\*T\d+:").unwrap());

/// Pattern to match frontmatter estimated_steps
static ESTIMATED_STEPS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^estimated_steps:\s*(\d+)").unwrap());

/// Pattern to match frontmatter estimated_files
static ESTIMATED_FILES_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^estimated_files:\s*(\d+)").unwrap());

/// Pattern to match frontmatter keys
#[allow(dead_code)] // Kept for future use
static FRONTMATTER_KEY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\w+):").unwrap());

// ─── Types ──────────────────────────────────────────────────────────────────────

/// Validation issue found in plan/summary files
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub severity: ValidationSeverity,
    pub scope: ValidationScope,
    pub file: String,
    pub rule_id: String,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Severity level of validation issue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
}

/// Scope of validation issue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationScope {
    SlicePlan,
    TaskPlan,
    TaskSummary,
    SliceSummary,
}

// ─── Public API ────────────────────────────────────────────────────────────────

/// Validate plan boundary (slice plan + all task plans)
///
/// # Arguments
/// * `base_path` - Project base path
/// * `milestone_id` - Milestone ID
/// * `slice_id` - Slice ID
///
/// # Returns
/// List of validation issues
pub fn validate_plan_boundary(
    base_path: &str,
    milestone_id: &str,
    slice_id: &str,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Validate slice plan
    if let Some(slice_plan_path) =
        resolve_slice_file(Path::new(base_path), milestone_id, slice_id, "PLAN")
    {
        if let Some(content) = load_file(&slice_plan_path) {
            issues.extend(validate_slice_plan_content(
                &slice_plan_path.to_string_lossy(),
                &content,
            ));
        }
    }

    // Validate all task plans
    if let Some(tasks_dir) = resolve_tasks_dir(Path::new(base_path), milestone_id, slice_id) {
        let task_plans = resolve_task_files(&tasks_dir, "PLAN");
        for file_path in task_plans {
            let file_name = Path::new(&file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let task_id = file_name.split('-').next().unwrap_or("");

            if let Some(task_plan_path) = resolve_task_file(
                Path::new(base_path),
                milestone_id,
                slice_id,
                task_id,
                "PLAN",
            ) {
                if let Some(content) = load_file(&task_plan_path) {
                    issues.extend(validate_task_plan_content(
                        &task_plan_path.to_string_lossy(),
                        &content,
                    ));
                }
            }
        }
    }

    issues
}

/// Validate execute boundary (slice plan + task plan)
///
/// # Arguments
/// * `base_path` - Project base path
/// * `milestone_id` - Milestone ID
/// * `slice_id` - Slice ID
/// * `task_id` - Task ID
///
/// # Returns
/// List of validation issues
pub fn validate_execute_boundary(
    base_path: &str,
    milestone_id: &str,
    slice_id: &str,
    task_id: &str,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Validate slice plan
    if let Some(slice_plan_path) =
        resolve_slice_file(Path::new(base_path), milestone_id, slice_id, "PLAN")
    {
        if let Some(content) = load_file(&slice_plan_path) {
            issues.extend(validate_slice_plan_content(
                &slice_plan_path.to_string_lossy(),
                &content,
            ));
        }
    }

    // Validate task plan
    if let Some(task_plan_path) = resolve_task_file(
        Path::new(base_path),
        milestone_id,
        slice_id,
        task_id,
        "PLAN",
    ) {
        if let Some(content) = load_file(&task_plan_path) {
            issues.extend(validate_task_plan_content(
                &task_plan_path.to_string_lossy(),
                &content,
            ));
        }
    }

    issues
}

/// Validate complete boundary (all task summaries + slice summary)
///
/// # Arguments
/// * `base_path` - Project base path
/// * `milestone_id` - Milestone ID
/// * `slice_id` - Slice ID
///
/// # Returns
/// List of validation issues
pub fn validate_complete_boundary(
    base_path: &str,
    milestone_id: &str,
    slice_id: &str,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Validate all task summaries
    if let Some(tasks_dir) = resolve_tasks_dir(Path::new(base_path), milestone_id, slice_id) {
        let task_summaries = resolve_task_files(&tasks_dir, "SUMMARY");
        for file_path in task_summaries {
            let file_name = Path::new(&file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let task_id = file_name.split('-').next().unwrap_or("");

            if let Some(task_summary_path) = resolve_task_file(
                Path::new(base_path),
                milestone_id,
                slice_id,
                task_id,
                "SUMMARY",
            ) {
                if let Some(content) = load_file(&task_summary_path) {
                    issues.extend(validate_task_summary_content(
                        &task_summary_path.to_string_lossy(),
                        &content,
                    ));
                }
            }
        }
    }

    // Validate slice summary
    if let Some(slice_summary_path) =
        resolve_slice_file(Path::new(base_path), milestone_id, slice_id, "SUMMARY")
    {
        if let Some(content) = load_file(&slice_summary_path) {
            issues.extend(validate_slice_summary_content(
                &slice_summary_path.to_string_lossy(),
                &content,
            ));
        }
    }

    issues
}

/// Format validation issues for display
///
/// # Arguments
/// * `issues` - List of validation issues
/// * `limit` - Maximum number of issues to format (default 4)
///
/// # Returns
/// Formatted string
pub fn format_validation_issues(issues: &[ValidationIssue], limit: Option<usize>) -> String {
    let limit = limit.unwrap_or(4);
    if issues.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = issues
        .iter()
        .take(limit)
        .map(|issue| {
            let file_name = Path::new(&issue.file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&issue.file);
            format!("- {}: {}", file_name, issue.message)
        })
        .collect();

    if issues.len() > limit {
        let more = format!("- ...and {} more", issues.len() - limit);
        [&lines[..], &[more]].concat().join("\n")
    } else {
        lines.join("\n")
    }
}

// ─── Content Validation Functions ───────────────────────────────────────────────

/// Validate slice plan content
pub fn validate_slice_plan_content(file: &str, content: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // ── Plan quality rules (always run, not gated by runtime relevance) ──

    if let Some(tasks_section) = get_section(content, "Tasks", 2) {
        let lines: Vec<&str> = tasks_section.lines().collect();
        let task_line_indices: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| TASK_LINE_RE.is_match(l))
            .map(|(i, _)| i)
            .collect();

        for (t, &start) in task_line_indices.iter().enumerate() {
            let end = if t + 1 < task_line_indices.len() {
                task_line_indices[t + 1]
            } else {
                lines.len()
            };

            let body_lines: Vec<&str> = lines
                .iter()
                .skip(start + 1)
                .take(end - start - 1)
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect();

            if body_lines.is_empty() {
                issues.push(ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    scope: ValidationScope::SlicePlan,
                    file: file.to_string(),
                    rule_id: "empty_task_entry".to_string(),
                    message: "Inline task entry has no description content beneath the checkbox line.".to_string(),
                    suggestion: Some("Add at least a Why/Files/Do/Verify summary so the task is self-describing.".to_string()),
                });
            }
        }
    }

    // ── Observability rules (gated by runtime relevance) ──

    if !text_suggests_observability_relevant(content) {
        return issues;
    }

    let obs = get_section(content, "Observability / Diagnostics", 2);
    let verification = get_section(content, "Verification", 2);

    if obs.is_none() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::SlicePlan,
            file: file.to_string(),
            rule_id: "missing_observability_section".to_string(),
            message: "Slice plan appears non-trivial but is missing `## Observability / Diagnostics`.".to_string(),
            suggestion: Some("Add runtime signals, inspection surfaces, failure visibility, and redaction constraints.".to_string()),
        });
    } else if section_looks_placeholder_only(obs.as_deref()) {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::SlicePlan,
            file: file.to_string(),
            rule_id: "observability_section_placeholder_only".to_string(),
            message: "Slice plan has `## Observability / Diagnostics` but it still looks like placeholder text.".to_string(),
            suggestion: Some("Replace placeholders with concrete signals and inspection surfaces a future agent should trust.".to_string()),
        });
    }

    if !verification_mentions_diagnostics(verification.as_deref()) {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::SlicePlan,
            file: file.to_string(),
            rule_id: "verification_missing_diagnostic_check".to_string(),
            message: "Slice verification does not appear to include any diagnostic or failure-path check.".to_string(),
            suggestion: Some("Add at least one verification step for inspectable failure state, structured error output, status surface, or equivalent.".to_string()),
        });
    }

    issues
}

/// Validate task plan content
pub fn validate_task_plan_content(file: &str, content: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // ── Plan quality rules (always run, not gated by runtime relevance) ──

    // Rule: empty or missing Steps section
    let steps_section = get_section(content, "Steps", 2);
    if steps_section.is_none() || section_looks_placeholder_only(steps_section.as_deref()) {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::TaskPlan,
            file: file.to_string(),
            rule_id: "empty_steps_section".to_string(),
            message: "Task plan has an empty or missing `## Steps` section.".to_string(),
            suggestion: Some(
                "Add concrete numbered implementation steps so execution has a clear sequence."
                    .to_string(),
            ),
        });
    }

    // Rule: placeholder-only Verification section
    let verification_section = get_section(content, "Verification", 2);
    if verification_section.is_some()
        && section_looks_placeholder_only(verification_section.as_deref())
    {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::TaskPlan,
            file: file.to_string(),
            rule_id: "placeholder_verification".to_string(),
            message: "Task plan has `## Verification` but it still looks like placeholder text.".to_string(),
            suggestion: Some("Replace placeholders with concrete verification commands, test runs, or observable checks.".to_string()),
        });
    }

    // Rule: scope estimate thresholds
    if let Some(fm) = get_frontmatter(content) {
        if let Some(caps) = ESTIMATED_STEPS_RE.captures(fm.as_str()) {
            if let Ok(steps) = caps[1].parse::<usize>() {
                if steps >= 10 {
                    issues.push(ValidationIssue {
                        severity: ValidationSeverity::Warning,
                        scope: ValidationScope::TaskPlan,
                        file: file.to_string(),
                        rule_id: "scope_estimate_steps_high".to_string(),
                        message: format!("Task plan estimates {} steps (threshold: 10). Consider splitting into smaller tasks.", steps),
                        suggestion: Some("Break the task into sub-tasks or reduce scope so each task stays focused and completable in one pass.".to_string()),
                    });
                }
            }
        }

        if let Some(caps) = ESTIMATED_FILES_RE.captures(fm.as_str()) {
            if let Ok(files) = caps[1].parse::<usize>() {
                if files >= 12 {
                    issues.push(ValidationIssue {
                        severity: ValidationSeverity::Warning,
                        scope: ValidationScope::TaskPlan,
                        file: file.to_string(),
                        rule_id: "scope_estimate_files_high".to_string(),
                        message: format!("Task plan estimates {} files (threshold: 12). Consider splitting into smaller tasks.", files),
                        suggestion: Some("Break the task into sub-tasks or reduce scope to keep the change footprint manageable.".to_string()),
                    });
                }
            }
        }
    }

    // ── Observability rules (gated by runtime relevance) ──

    if !text_suggests_observability_relevant(content) {
        return issues;
    }

    let obs = get_section(content, "Observability Impact", 2);
    if obs.is_none() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::TaskPlan,
            file: file.to_string(),
            rule_id: "missing_observability_impact".to_string(),
            message: "Task plan appears runtime-relevant but is missing `## Observability Impact`.".to_string(),
            suggestion: Some("Explain what signals change, how a future agent inspects this task, and what failure state becomes visible.".to_string()),
        });
    } else if section_looks_placeholder_only(obs.as_deref()) {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::TaskPlan,
            file: file.to_string(),
            rule_id: "observability_impact_placeholder_only".to_string(),
            message: "Task plan has `## Observability Impact` but it still looks empty or placeholder-only.".to_string(),
            suggestion: Some("Fill in concrete inspection surfaces or explicitly justify why observability is not applicable.".to_string()),
        });
    }

    issues
}

/// Validate task summary content
pub fn validate_task_summary_content(file: &str, content: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if !has_frontmatter_key(content, "observability_surfaces") {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::TaskSummary,
            file: file.to_string(),
            rule_id: "missing_observability_frontmatter".to_string(),
            message: "Task summary is missing `observability_surfaces` in frontmatter.".to_string(),
            suggestion: Some(
                "List the durable status/log/error surfaces a future agent should use.".to_string(),
            ),
        });
    }

    let diagnostics = get_section(content, "Diagnostics", 2);
    if diagnostics.is_none() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::TaskSummary,
            file: file.to_string(),
            rule_id: "missing_diagnostics_section".to_string(),
            message: "Task summary is missing `## Diagnostics`.".to_string(),
            suggestion: Some("Document how to inspect what this task built later.".to_string()),
        });
    } else if section_looks_placeholder_only(diagnostics.as_deref()) {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::TaskSummary,
            file: file.to_string(),
            rule_id: "diagnostics_placeholder_only".to_string(),
            message: "Task summary diagnostics section still looks like placeholder text.".to_string(),
            suggestion: Some("Replace placeholders with concrete commands, endpoints, logs, error shapes, or failure artifacts.".to_string()),
        });
    }

    let evidence = get_section(content, "Verification Evidence", 2);
    if evidence.is_none() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::TaskSummary,
            file: file.to_string(),
            rule_id: "evidence_block_missing".to_string(),
            message: "Task summary is missing `## Verification Evidence`.".to_string(),
            suggestion: Some("Add a verification evidence table showing gate check results (command, exit code, verdict, duration).".to_string()),
        });
    } else if section_looks_placeholder_only(evidence.as_deref()) {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::TaskSummary,
            file: file.to_string(),
            rule_id: "evidence_block_placeholder".to_string(),
            message: "Task summary verification evidence section still looks like placeholder text.".to_string(),
            suggestion: Some("Replace placeholders with actual gate results or note that no verification commands were discovered.".to_string()),
        });
    }

    issues
}

/// Validate slice summary content
pub fn validate_slice_summary_content(file: &str, content: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if !has_frontmatter_key(content, "observability_surfaces") {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::SliceSummary,
            file: file.to_string(),
            rule_id: "missing_observability_frontmatter".to_string(),
            message: "Slice summary is missing `observability_surfaces` in frontmatter.".to_string(),
            suggestion: Some("List the authoritative diagnostics and durable inspection surfaces for this slice.".to_string()),
        });
    }

    let diagnostics = get_section(content, "Authoritative diagnostics", 3);
    if diagnostics.is_none() {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::SliceSummary,
            file: file.to_string(),
            rule_id: "missing_authoritative_diagnostics".to_string(),
            message:
                "Slice summary is missing `### Authoritative diagnostics` in Forward Intelligence."
                    .to_string(),
            suggestion: Some(
                "Tell future agents where to look first and why that signal is trustworthy."
                    .to_string(),
            ),
        });
    } else if section_looks_placeholder_only(diagnostics.as_deref()) {
        issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::SliceSummary,
            file: file.to_string(),
            rule_id: "authoritative_diagnostics_placeholder_only".to_string(),
            message: "Slice summary includes authoritative diagnostics but it still looks like placeholder text.".to_string(),
            suggestion: Some("Replace placeholders with the real first-stop diagnostic surface for this slice.".to_string()),
        });
    }

    issues
}

// ─── Helper Functions ─────────────────────────────────────────────────────────

/// Get a section from markdown content
fn get_section(content: &str, heading: &str, level: usize) -> Option<String> {
    let prefix = "#".repeat(level) + " ";
    let escaped = regex_escape(heading);
    let pattern = format!(r"(?m)^{}\{}\s*$", prefix, escaped);

    let re = Regex::new(&pattern).ok()?;
    let m = re.find(content)?;

    let start = m.start() + m.len();
    let rest = &content[start..];

    // Find next heading at same or higher level
    let next_heading_pattern = format!(r"(?m)^{{1,{}\}} ", level);
    let next_heading_re = Regex::new(&next_heading_pattern).ok()?;

    let end = next_heading_re
        .find(rest)
        .map(|m| m.start())
        .unwrap_or(rest.len());

    Some(rest[..end].trim().to_string())
}

/// Get frontmatter from content
fn get_frontmatter(content: &str) -> Option<String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }

    let after_first = trimmed.find('\n')?;
    let rest = &trimmed[after_first + 1..];
    let end_idx = rest.find("\n---")?;

    Some(rest[..end_idx].to_string())
}

/// Check if frontmatter has a key
fn has_frontmatter_key(content: &str, key: &str) -> bool {
    let fm = match get_frontmatter(content) {
        Some(f) => f,
        None => return false,
    };

    let pattern = format!(r"(?m)^{}:", regex_escape(key));
    Regex::new(&pattern)
        .ok()
        .map(|r| r.is_match(&fm))
        .unwrap_or(false)
}

/// Normalize meaningful lines (filter out comments, templates, etc.)
fn normalize_meaningful_lines(text: &str) -> Vec<String> {
    // Pre-compile regex patterns for efficiency
    let template_re1 = Regex::new(r"^[-*]\s*\{\{.+\}\}$").unwrap();
    let template_re2 = Regex::new(r"^\{\{.+\}\}$").unwrap();

    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with("<!--"))
        .filter(|l| !l.ends_with("-->"))
        .filter(|l| !template_re1.is_match(l))
        .filter(|l| !template_re2.is_match(l))
        .collect()
}

/// Check if section looks like placeholder only
fn section_looks_placeholder_only(text: Option<&str>) -> bool {
    let text = match text {
        Some(t) => t,
        None => return true,
    };

    let lines = normalize_meaningful_lines(text)
        .into_iter()
        .map(|l| {
            // Remove leading bullet/dash and whitespace
            let trimmed = l.trim_start_matches('-').trim_start_matches('*');
            trimmed.trim().to_string()
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return true;
    }

    lines.iter().all(|line| {
        let lower = line.to_lowercase();
        lower == "none"
            || lower.ends_with(": none")
            || lower.contains("{{")
            || lower.contains("}}")
            || lower.starts_with("required for non-trivial")
            || lower.starts_with("describe how a future agent")
            || lower.starts_with("prefer:")
            || lower.starts_with("keep this section concise")
    })
}

/// Check if text suggests observability relevance
fn text_suggests_observability_relevant(content: &str) -> bool {
    let lower = content.to_lowercase();
    let needles = [
        " api",
        "route",
        "server",
        "worker",
        "queue",
        "job",
        "sync",
        "import",
        "webhook",
        "auth",
        "db",
        "database",
        "migration",
        "cache",
        "background",
        "polling",
        "realtime",
        "socket",
        "stateful",
        "integration",
        "ui",
        "form",
        "submit",
        "status",
        "service",
        "pipeline",
        "health endpoint",
        "error path",
    ];

    needles.iter().any(|needle| lower.contains(needle))
}

/// Check if verification section mentions diagnostics
fn verification_mentions_diagnostics(section: Option<&str>) -> bool {
    let section = match section {
        Some(s) => s,
        None => return false,
    };

    let lower = section.to_lowercase();
    let needles = [
        "error",
        "failure",
        "diagnostic",
        "status",
        "health",
        "inspect",
        "log",
        "network",
        "console",
        "retry",
        "last error",
        "correlation",
        "readiness",
    ];

    needles.iter().any(|needle| lower.contains(needle))
}

/// Escape special regex characters
fn regex_escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' | '^' | '$' => {
                format!("\\{}", c)
            }
            _ => c.to_string(),
        })
        .collect()
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_issue() {
        let issue = ValidationIssue {
            severity: ValidationSeverity::Warning,
            scope: ValidationScope::SlicePlan,
            file: "/test/PLAN.md".to_string(),
            rule_id: "test_rule".to_string(),
            message: "Test message".to_string(),
            suggestion: Some("Test suggestion".to_string()),
        };

        assert_eq!(issue.rule_id, "test_rule");
        assert!(issue.suggestion.is_some());
    }

    #[test]
    fn test_format_validation_issues_empty() {
        let issues = vec![];
        let formatted = format_validation_issues(&issues, Some(4));
        assert_eq!(formatted, "");
    }

    #[test]
    fn test_format_validation_issues_with_limit() {
        let issues = vec![
            ValidationIssue {
                severity: ValidationSeverity::Warning,
                scope: ValidationScope::SlicePlan,
                file: "/test/PLAN.md".to_string(),
                rule_id: "test1".to_string(),
                message: "Issue 1".to_string(),
                suggestion: None,
            },
            ValidationIssue {
                severity: ValidationSeverity::Warning,
                scope: ValidationScope::SlicePlan,
                file: "/test/PLAN2.md".to_string(),
                rule_id: "test2".to_string(),
                message: "Issue 2".to_string(),
                suggestion: None,
            },
        ];

        let formatted = format_validation_issues(&issues, Some(1));
        assert!(formatted.contains("PLAN.md"));
        assert!(formatted.contains("...and 1 more"));
    }

    #[test]
    fn test_section_looks_placeholder_only_none() {
        assert!(section_looks_placeholder_only(None));
    }

    #[test]
    fn test_section_looks_placeholder_only_empty() {
        assert!(section_looks_placeholder_only(Some("")));
    }

    #[test]
    fn test_section_looks_placeholder_only_with_content() {
        let content = "## Steps\n1. Do something\n2. Do something else";
        assert!(!section_looks_placeholder_only(Some(content)));
    }

    #[test]
    fn test_text_suggests_observability_relevant() {
        assert!(text_suggests_observability_relevant(
            "Build an API endpoint"
        ));
        assert!(text_suggests_observability_relevant("Add authentication"));
        assert!(!text_suggests_observability_relevant(
            "Update documentation"
        ));
    }

    #[test]
    fn test_regex_escape() {
        assert_eq!(regex_escape("test.file"), r"test\.file");
        assert_eq!(regex_escape("test*"), r"test\*");
    }
}
