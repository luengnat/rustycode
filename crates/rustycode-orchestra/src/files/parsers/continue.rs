//! Continue file (CONTINUE.md) parser
//!
//! Parses continue files containing:
//! - Session state frontmatter
//! - Completed and remaining work
//! - Decisions made and next actions

use crate::files::parsers::common::{extract_section, parse_frontmatter_map, split_frontmatter};
use std::collections::HashMap;

/// Continue structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Continue {
    pub frontmatter: ContinueFrontmatter,
    pub completed_work: String,
    pub remaining_work: String,
    pub decisions: String,
    pub context: String,
    pub next_action: String,
}

/// Continue frontmatter
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContinueFrontmatter {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub milestone: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub slice: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub task: String,
    #[serde(default)]
    pub step: usize,
    #[serde(default)]
    pub total_steps: usize,
    #[serde(default = "default_continue_status")]
    pub status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub saved_at: String,
}

fn default_continue_status() -> String {
    "in_progress".to_string()
}

/// Parse continue file
pub fn parse_continue(content: &str) -> Continue {
    let (fm_lines, body) = split_frontmatter(content);

    let fm = if let Some(fm) = fm_lines {
        parse_frontmatter_map(&fm)
    } else {
        HashMap::new()
    };

    let frontmatter = parse_continue_frontmatter(&fm);

    let completed_work = extract_section(&body, "Completed Work", 2).unwrap_or_default();
    let remaining_work = extract_section(&body, "Remaining Work", 2).unwrap_or_default();
    let decisions = extract_section(&body, "Decisions Made", 2).unwrap_or_default();
    let context = extract_section(&body, "Context", 2).unwrap_or_default();
    let next_action = extract_section(&body, "Next Action", 2).unwrap_or_default();

    Continue {
        frontmatter,
        completed_work,
        remaining_work,
        decisions,
        context,
        next_action,
    }
}

fn parse_continue_frontmatter(fm: &HashMap<String, serde_yaml::Value>) -> ContinueFrontmatter {
    ContinueFrontmatter {
        milestone: fm
            .get("milestone")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        slice: fm
            .get("slice")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        task: fm
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        step: fm
            .get("step")
            .and_then(|v| v.as_i64())
            .or_else(|| {
                fm.get("step")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0) as usize,
        total_steps: fm
            .get("total_steps")
            .and_then(|v| v.as_i64())
            .or_else(|| fm.get("totalSteps").and_then(|v| v.as_i64()))
            .or_else(|| {
                fm.get("total_steps")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .or_else(|| {
                fm.get("totalSteps")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0) as usize,
        status: fm
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("in_progress")
            .to_string(),
        saved_at: fm
            .get("saved_at")
            .and_then(|v| v.as_str())
            .or_else(|| fm.get("savedAt").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string(),
    }
}

/// Format continue data
pub fn format_continue(cont: &Continue) -> String {
    let fm = cont.frontmatter.clone();
    let mut lines = Vec::new();

    lines.push("---".to_string());
    if !fm.milestone.is_empty() {
        lines.push(format!("milestone: {}", fm.milestone));
    }
    if !fm.slice.is_empty() {
        lines.push(format!("slice: {}", fm.slice));
    }
    if !fm.task.is_empty() {
        lines.push(format!("task: {}", fm.task));
    }
    lines.push(format!("step: {}", fm.step));
    lines.push(format!("total_steps: {}", fm.total_steps));
    lines.push(format!("status: {}", fm.status));
    if !fm.saved_at.is_empty() {
        lines.push(format!("saved_at: {}", fm.saved_at));
    }
    lines.push("---".to_string());

    lines.push(String::new());
    lines.push("## Completed Work".to_string());
    lines.push(cont.completed_work.clone());
    lines.push(String::new());
    lines.push("## Remaining Work".to_string());
    lines.push(cont.remaining_work.clone());
    lines.push(String::new());
    lines.push("## Decisions Made".to_string());
    lines.push(cont.decisions.clone());
    lines.push(String::new());
    lines.push("## Context".to_string());
    lines.push(cont.context.clone());
    lines.push(String::new());
    lines.push("## Next Action".to_string());
    lines.push(cont.next_action.clone());

    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_continue_basic() {
        let content = r#"---
milestone: M001
slice: S01
step: 3
total_steps: 10
status: in_progress
---

## Completed Work

Work done so far.

## Remaining Work

Work left to do.

## Decisions Made

Decisions.

## Context

Context.

## Next Action

Next step.
"#;

        let cont = parse_continue(content);

        assert_eq!(cont.frontmatter.milestone, "M001");
        assert_eq!(cont.frontmatter.slice, "S01");
        assert_eq!(cont.frontmatter.step, 3);
        assert_eq!(cont.frontmatter.total_steps, 10);
        assert_eq!(cont.frontmatter.status, "in_progress");
        assert_eq!(cont.completed_work, "Work done so far.");
        assert_eq!(cont.remaining_work, "Work left to do.");
    }

    #[test]
    fn test_parse_continue_minimal() {
        let content = r#"# Continue

## Completed Work

Done.
"#;

        let cont = parse_continue(content);

        assert_eq!(cont.frontmatter.milestone, "");
        assert_eq!(cont.frontmatter.step, 0);
        assert_eq!(cont.completed_work, "Done.");
    }

    #[test]
    fn test_format_continue() {
        let cont = Continue {
            frontmatter: ContinueFrontmatter {
                milestone: "M001".to_string(),
                slice: "S01".to_string(),
                task: "T01".to_string(),
                step: 5,
                total_steps: 10,
                status: "in_progress".to_string(),
                saved_at: String::new(),
            },
            completed_work: "Work done".to_string(),
            remaining_work: "Work left".to_string(),
            decisions: "Decisions".to_string(),
            context: "Context".to_string(),
            next_action: "Next".to_string(),
        };

        let formatted = format_continue(&cont);

        assert!(formatted.contains("milestone: M001"));
        assert!(formatted.contains("step: 5"));
        assert!(formatted.contains("## Completed Work"));
    }
}
