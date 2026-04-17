//! Orchestra Structured Data Formatter — Token-efficient prompt formatting
//!
//! Converts Orchestra data structures into a compact format that removes markdown
//! table overhead, redundant labels, and formatting while remaining perfectly
//! readable by LLMs.
//!
//! # Format Rules
//!
//! - No table pipes, dashes, or header rows
//! - Use indentation (2 spaces) for structure instead of delimiters
//! - Omit field names when the pattern is clear from a header
//! - Use single-line entries for simple records
//! - Use multi-line with indentation for complex records

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Decision record input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionInput {
    pub id: String,
    pub when_context: String,
    pub scope: String,
    pub decision: String,
    pub choice: String,
    pub rationale: String,
    pub revisable: String,
}

/// Requirement record input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementInput {
    pub id: String,
    pub class: String,
    pub status: String,
    pub description: String,
    pub why: String,
    pub primary_owner: String,
    pub validation: String,
}

/// Task plan entry input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlanInput {
    pub id: String,
    pub title: String,
    pub description: String,
    pub done: bool,
    pub estimate: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
}

// ---------------------------------------------------------------------------
// Decisions
// ---------------------------------------------------------------------------

/// Compact format for a single decision record (pipe-separated, no padding)
///
/// # Arguments
/// * `decision` - Decision record to format
///
/// # Returns
/// Formatted decision string
///
/// # Example
/// ```
/// use rustycode_orchestra::structured_data_formatter::*;
///
/// let decision = DecisionInput {
///     id: "D01".to_string(),
///     when_context: "2024-01-15".to_string(),
///     scope: "M01".to_string(),
///     decision: "Use Rust for implementation".to_string(),
///     choice: "Rust".to_string(),
///     rationale: "Performance and safety".to_string(),
///     revisable: "no".to_string(),
/// };
///
/// let formatted = format_decision_compact(&decision);
/// assert!(formatted.contains("D01 | 2024-01-15"));
/// ```
pub fn format_decision_compact(decision: &DecisionInput) -> String {
    [
        &decision.id,
        &decision.when_context,
        &decision.scope,
        &decision.decision,
        &decision.choice,
        &decision.rationale,
        &decision.revisable,
    ]
    .iter()
    .map(|s| s.as_str())
    .collect::<Vec<&str>>()
    .join(" | ")
}

/// Format multiple decisions in compact notation with a Fields header
///
/// # Arguments
/// * `decisions` - Vector of decision records to format
///
/// # Returns
/// Formatted decisions string with header
///
/// # Example
/// ```
/// use rustycode_orchestra::structured_data_formatter::*;
///
/// let decisions = vec![];
/// let formatted = format_decisions_compact(&decisions);
/// assert!(formatted.contains("(none)"));
/// ```
pub fn format_decisions_compact(decisions: &[DecisionInput]) -> String {
    if decisions.is_empty() {
        return "# Decisions (compact)\n(none)".to_string();
    }

    let header = "# Decisions (compact)\nFields: id | when | scope | decision | choice | rationale | revisable";
    let lines: Vec<String> = decisions.iter().map(format_decision_compact).collect();
    format!("{}\n\n{}", header, lines.join("\n"))
}

// ---------------------------------------------------------------------------
// Requirements
// ---------------------------------------------------------------------------

/// Compact format for a single requirement record (multi-line)
///
/// # Arguments
/// * `req` - Requirement record to format
///
/// # Returns
/// Formatted requirement string with indentation
///
/// # Example
/// ```
/// use rustycode_orchestra::structured_data_formatter::*;
///
/// let req = RequirementInput {
///     id: "R01".to_string(),
///     class: "feature".to_string(),
///     status: "active".to_string(),
///     description: "User authentication".to_string(),
///     why: "Security requirement".to_string(),
///     primary_owner: "alice".to_string(),
///     validation: "Login works".to_string(),
/// };
///
/// let formatted = format_requirement_compact(&req);
/// assert!(formatted.contains("R01 [feature] (active)"));
/// assert!(formatted.contains("owner:alice"));
/// ```
pub fn format_requirement_compact(req: &RequirementInput) -> String {
    [
        format!(
            "{} [{}] ({}) owner:{}",
            req.id, req.class, req.status, req.primary_owner
        ),
        format!("  {}", req.description),
        format!("  why: {}", req.why),
        format!("  validate: {}", req.validation),
    ]
    .join("\n")
}

/// Format multiple requirements in compact notation
///
/// # Arguments
/// * `requirements` - Vector of requirement records to format
///
/// # Returns
/// Formatted requirements string with header
///
/// # Example
/// ```
/// use rustycode_orchestra::structured_data_formatter::*;
///
/// let requirements = vec![];
/// let formatted = format_requirements_compact(&requirements);
/// assert!(formatted.contains("(none)"));
/// ```
pub fn format_requirements_compact(requirements: &[RequirementInput]) -> String {
    if requirements.is_empty() {
        return "# Requirements (compact)\n(none)".to_string();
    }

    let header = "# Requirements (compact)";
    let blocks: Vec<String> = requirements
        .iter()
        .map(format_requirement_compact)
        .collect();
    format!("{}\n\n{}", header, blocks.join("\n\n"))
}

// ---------------------------------------------------------------------------
// Task Plans
// ---------------------------------------------------------------------------

/// Compact format for task plan entries
///
/// # Arguments
/// * `tasks` - Vector of task plan entries to format
///
/// # Returns
/// Formatted tasks string with header
///
/// # Example
/// ```
/// use rustycode_orchestra::structured_data_formatter::*;
///
/// let tasks = vec![];
/// let formatted = format_task_plan_compact(&tasks);
/// assert!(formatted.contains("(none)"));
/// ```
pub fn format_task_plan_compact(tasks: &[TaskPlanInput]) -> String {
    if tasks.is_empty() {
        return "# Tasks (compact)\n(none)".to_string();
    }

    let header = "# Tasks (compact)";
    let blocks: Vec<String> = tasks
        .iter()
        .map(|t| {
            let check = if t.done { "x" } else { " " };
            let mut lines = vec![format!("{} [{}] {} ({})", t.id, check, t.title, t.estimate)];

            if let Some(ref files) = t.files {
                if !files.is_empty() {
                    lines.push(format!("  files: {}", files.join(", ")));
                }
            }

            if let Some(ref verify) = t.verify {
                lines.push(format!("  verify: {}", verify));
            }

            lines.push(format!("  {}", t.description));
            lines.join("\n")
        })
        .collect();

    format!("{}\n\n{}", header, blocks.join("\n\n"))
}

// ---------------------------------------------------------------------------
// Savings measurement
// ---------------------------------------------------------------------------

/// Measure the token savings of compact format vs markdown format
///
/// # Arguments
/// * `compact_content` - Compact formatted content
/// * `markdown_content` - Markdown formatted content
///
/// # Returns
/// Savings as a percentage (0-100). Positive means compact is smaller.
///
/// # Example
/// ```
/// use rustycode_orchestra::structured_data_formatter::*;
///
/// let compact = "short content";
/// let markdown = "much longer markdown content with tables";
/// let savings = measure_savings(compact, markdown);
/// assert!(savings > 0.0); // Compact saves tokens
/// ```
pub fn measure_savings(compact_content: &str, markdown_content: &str) -> f64 {
    if markdown_content.is_empty() {
        return 0.0;
    }

    let saved = markdown_content.len() as f64 - compact_content.len() as f64;
    (saved / markdown_content.len() as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_decision_compact() {
        let decision = DecisionInput {
            id: "D01".to_string(),
            when_context: "2024-01-15".to_string(),
            scope: "M01".to_string(),
            decision: "Use Rust".to_string(),
            choice: "Rust".to_string(),
            rationale: "Performance".to_string(),
            revisable: "no".to_string(),
        };

        let formatted = format_decision_compact(&decision);
        assert!(formatted.contains("D01 | 2024-01-15 | M01 | Use Rust | Rust | Performance | no"));
    }

    #[test]
    fn test_format_decisions_compact_empty() {
        let decisions = vec![];
        let formatted = format_decisions_compact(&decisions);
        assert!(formatted.contains("# Decisions (compact)"));
        assert!(formatted.contains("(none)"));
    }

    #[test]
    fn test_format_decisions_compact_multiple() {
        let decisions = vec![
            DecisionInput {
                id: "D01".to_string(),
                when_context: "2024-01-15".to_string(),
                scope: "M01".to_string(),
                decision: "Use Rust".to_string(),
                choice: "Rust".to_string(),
                rationale: "Performance".to_string(),
                revisable: "no".to_string(),
            },
            DecisionInput {
                id: "D02".to_string(),
                when_context: "2024-01-16".to_string(),
                scope: "M01".to_string(),
                decision: "Use SQLite".to_string(),
                choice: "SQLite".to_string(),
                rationale: "Simplicity".to_string(),
                revisable: "yes".to_string(),
            },
        ];

        let formatted = format_decisions_compact(&decisions);
        assert!(formatted.contains("# Decisions (compact)"));
        assert!(formatted
            .contains("Fields: id | when | scope | decision | choice | rationale | revisable"));
        assert!(formatted.contains("D01 | 2024-01-15"));
        assert!(formatted.contains("D02 | 2024-01-16"));
    }

    #[test]
    fn test_format_requirement_compact() {
        let req = RequirementInput {
            id: "R01".to_string(),
            class: "feature".to_string(),
            status: "active".to_string(),
            description: "User authentication".to_string(),
            why: "Security".to_string(),
            primary_owner: "alice".to_string(),
            validation: "Login works".to_string(),
        };

        let formatted = format_requirement_compact(&req);
        assert!(formatted.contains("R01 [feature] (active) owner:alice"));
        assert!(formatted.contains("  User authentication"));
        assert!(formatted.contains("  why: Security"));
        assert!(formatted.contains("  validate: Login works"));
    }

    #[test]
    fn test_format_requirements_compact_empty() {
        let requirements = vec![];
        let formatted = format_requirements_compact(&requirements);
        assert!(formatted.contains("# Requirements (compact)"));
        assert!(formatted.contains("(none)"));
    }

    #[test]
    fn test_format_task_plan_compact_empty() {
        let tasks = vec![];
        let formatted = format_task_plan_compact(&tasks);
        assert!(formatted.contains("# Tasks (compact)"));
        assert!(formatted.contains("(none)"));
    }

    #[test]
    fn test_format_task_plan_compact_with_files() {
        let tasks = vec![TaskPlanInput {
            id: "T01".to_string(),
            title: "Implement auth".to_string(),
            description: "Add login".to_string(),
            done: false,
            estimate: "2h".to_string(),
            files: Some(vec!["src/auth.rs".to_string(), "src/login.rs".to_string()]),
            verify: Some("Tests pass".to_string()),
        }];

        let formatted = format_task_plan_compact(&tasks);
        assert!(formatted.contains("T01 [ ] Implement auth (2h)"));
        assert!(formatted.contains("  files: src/auth.rs, src/login.rs"));
        assert!(formatted.contains("  verify: Tests pass"));
        assert!(formatted.contains("  Add login"));
    }

    #[test]
    fn test_format_task_plan_compact_done() {
        let tasks = vec![TaskPlanInput {
            id: "T01".to_string(),
            title: "Implement auth".to_string(),
            description: "Add login".to_string(),
            done: true,
            estimate: "2h".to_string(),
            files: None,
            verify: None,
        }];

        let formatted = format_task_plan_compact(&tasks);
        assert!(formatted.contains("T01 [x] Implement auth (2h)"));
        assert!(formatted.contains("  Add login"));
    }

    #[test]
    fn test_measure_savings() {
        let compact = "short";
        let markdown = "much longer content";
        let savings = measure_savings(compact, markdown);
        assert!(savings > 0.0);
    }

    #[test]
    fn test_measure_savings_empty_markdown() {
        let compact = "content";
        let markdown = "";
        let savings = measure_savings(compact, markdown);
        assert_eq!(savings, 0.0);
    }

    #[test]
    fn test_measure_savings_both_empty() {
        let compact = "";
        let markdown = "";
        let savings = measure_savings(compact, markdown);
        assert_eq!(savings, 0.0);
    }
}
