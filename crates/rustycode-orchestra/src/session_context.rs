//! Autonomous Mode Style Session Context Builder
//!
//! Each unit (task) gets a fresh session with minimal context:
//! - Task plan (full detail)
//! - Slice excerpt (goal/demo/verification only)
//! - Prior task summaries (compressed)
//!
//! This prevents context pollution and matches orchestra-2's approach.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Session context for a single task unit
#[derive(Debug, Clone)]
pub struct SessionContext {
    pub task_id: String,
    pub task_title: String,
    pub slice_id: String,
    pub slice_title: String,
    pub milestone_id: String,
    pub task_plan: String,
    pub slice_excerpt: String,
    pub prior_summaries: Vec<PriorTaskSummary>,
    pub working_directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PriorTaskSummary {
    pub task_id: String,
    pub title: String,
    pub summary_path: PathBuf,
}

/// Build minimal session context for a task (matches orchestra-2's approach)
pub fn build_session_context(
    project_root: &Path,
    milestone_id: &str,
    slice_id: &str,
    task_id: &str,
) -> Result<SessionContext> {
    let slice_path = project_root
        .join(".orchestra/milestones")
        .join(milestone_id)
        .join("slices")
        .join(slice_id);

    let task_path = slice_path.join("tasks").join(task_id);
    let plan_path = slice_path.join("PLAN.md");
    let task_plan_path = task_path.join(format!("{}-PLAN.md", task_id));

    // Read task plan (full detail - this is the primary contract)
    let task_plan = fs::read_to_string(&task_plan_path)
        .with_context(|| format!("Failed to read task plan: {:?}", task_plan_path))?;

    // Extract title from task plan (first line or # heading)
    let task_title = extract_title(&task_plan).unwrap_or_else(|| format!("Task {}", task_id));

    // Read slice plan and extract only goal/demo/verification (minimal context)
    let slice_plan = fs::read_to_string(&plan_path)
        .with_context(|| format!("Failed to read slice plan: {:?}", plan_path))?;
    let slice_excerpt = extract_slice_excerpt(&slice_plan);

    // Extract slice title
    let slice_title =
        extract_title(&slice_excerpt).unwrap_or_else(|| format!("Slice {}", slice_id));

    // Find prior task summaries (compressed context from earlier tasks)
    let prior_summaries = find_prior_summaries(&slice_path, task_id)?;

    Ok(SessionContext {
        task_id: task_id.to_string(),
        task_title,
        slice_id: slice_id.to_string(),
        slice_title,
        milestone_id: milestone_id.to_string(),
        task_plan,
        slice_excerpt,
        prior_summaries,
        working_directory: project_root.to_path_buf(),
    })
}

/// Extract only goal/demo/verification from slice plan (orchestra-2 pattern)
fn extract_slice_excerpt(slice_plan: &str) -> String {
    let lines: Vec<&str> = slice_plan.lines().collect();
    let mut excerpt = String::new();
    let mut in_goal = false;
    let mut in_demo = false;
    let mut in_verification = false;
    let mut found_any = false;

    for line in lines {
        // Track sections we want
        if line.starts_with("## Goal") {
            in_goal = true;
            found_any = true;
        } else if line.starts_with("## Demo") || line.starts_with("## Verification") {
            in_demo = true;
            in_goal = false;
            found_any = true;
        } else if line.starts_with("## Slice-Level Verification") {
            in_verification = true;
            in_demo = false;
            in_goal = false;
            found_any = true;
        } else if line.starts_with("## ") && (in_goal || in_demo || in_verification) {
            // End of target section
            in_goal = false;
            in_demo = false;
            in_verification = false;
        }

        // Include line if we're in a target section
        if in_goal || in_demo || in_verification {
            excerpt.push_str(line);
            excerpt.push('\n');
        }

        // Stop at Tasks section (we don't want task list in excerpt)
        if line.starts_with("## Tasks") {
            break;
        }
    }

    if found_any {
        excerpt
    } else {
        // Fallback: return first 20 lines if no sections found
        slice_plan.lines().take(20).collect::<Vec<_>>().join("\n")
    }
}

/// Find prior task summaries (T01-SUMMARY.md, T02-SUMMARY.md, etc.)
fn find_prior_summaries(slice_path: &Path, current_task_id: &str) -> Result<Vec<PriorTaskSummary>> {
    let tasks_dir = slice_path.join("tasks");
    let mut summaries = Vec::new();

    if !tasks_dir.exists() {
        return Ok(summaries);
    }

    let entries = fs::read_dir(&tasks_dir)
        .with_context(|| format!("Failed to read tasks directory: {:?}", tasks_dir))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Extract task ID from directory name (e.g., "T01" from "T01-SUMMARY.md" path)
        let task_id = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Skip current task and tasks without summaries
        if task_id == current_task_id {
            continue;
        }

        let summary_file = path.join(format!("{}-SUMMARY.md", task_id));
        if summary_file.exists() {
            // Read summary to extract title
            let title = fs::read_to_string(&summary_file)
                .ok()
                .and_then(|s| extract_title(&s))
                .unwrap_or_else(|| format!("Task {}", task_id));

            summaries.push(PriorTaskSummary {
                task_id: task_id.to_string(),
                title,
                summary_path: summary_file,
            });
        }
    }

    // Sort by task ID to ensure chronological order
    summaries.sort_by(|a, b| a.task_id.cmp(&b.task_id));

    Ok(summaries)
}

/// Extract title from markdown (first # heading or first line)
fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            return Some(trimmed.trim_start_matches('#').trim().to_string());
        } else if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Format prior summaries as a list for inclusion in prompt
pub fn format_prior_summaries(summaries: &[PriorTaskSummary]) -> String {
    if summaries.is_empty() {
        return "None (this is the first task)".to_string();
    }

    summaries
        .iter()
        .map(|s| {
            format!(
                "- [{}]({}): {}",
                s.task_id,
                s.summary_path.display(),
                s.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_slice_excerpt() {
        let slice_plan = r#"# Slice S01: Create README

## Goal
Create a README file

## Demo
File exists at root

## Tasks
- [T01](./tasks/T01-PLAN.md): Do it
"#;

        let excerpt = extract_slice_excerpt(slice_plan);
        assert!(excerpt.contains("## Goal"));
        assert!(excerpt.contains("Create a README file"));
        assert!(excerpt.contains("## Demo"));
        assert!(excerpt.contains("File exists at root"));
        assert!(!excerpt.contains("## Tasks"));
    }

    #[test]
    fn test_extract_title() {
        let content = "# My Title\n\nSome content";
        assert_eq!(extract_title(content), Some("My Title".to_string()));
    }

    #[test]
    fn test_format_prior_summaries() {
        let summaries = vec![PriorTaskSummary {
            task_id: "T01".to_string(),
            title: "First task".to_string(),
            summary_path: PathBuf::from("/path/T01-SUMMARY.md"),
        }];

        let formatted = format_prior_summaries(&summaries);
        assert!(formatted.contains("T01"));
        assert!(formatted.contains("First task"));
    }

    // --- extract_title ---

    #[test]
    fn extract_title_from_heading() {
        assert_eq!(
            extract_title("# Task T01: Fix bug"),
            Some("Task T01: Fix bug".into())
        );
    }

    #[test]
    fn extract_title_from_subheading() {
        assert_eq!(extract_title("## Sub Title"), Some("Sub Title".into()));
    }

    #[test]
    fn extract_title_first_nonempty_line() {
        assert_eq!(
            extract_title("  First line\nSecond line"),
            Some("First line".into())
        );
    }

    #[test]
    fn extract_title_empty_returns_none() {
        assert_eq!(extract_title(""), None);
        assert_eq!(extract_title("  \n  \n"), None);
    }

    // --- extract_slice_excerpt ---

    #[test]
    fn slice_excerpt_no_matching_sections_fallback() {
        let plan = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10\nLine 11\nLine 12\nLine 13\nLine 14\nLine 15\nLine 16\nLine 17\nLine 18\nLine 19\nLine 20\nLine 21\n";
        let excerpt = extract_slice_excerpt(plan);
        // Fallback: first 20 lines
        assert!(excerpt.contains("Line 1"));
    }

    #[test]
    fn slice_excerpt_stops_at_tasks() {
        let plan = "## Goal\nDo something\n## Tasks\n- [ ] T01: stuff\n## Verification\nShould not appear\n";
        let excerpt = extract_slice_excerpt(plan);
        assert!(!excerpt.contains("Should not appear"));
    }

    #[test]
    fn slice_excerpt_includes_verification() {
        let plan = "## Verification\nAll tests pass\n## Tasks\n- done\n";
        let excerpt = extract_slice_excerpt(plan);
        assert!(excerpt.contains("All tests pass"));
    }

    // --- format_prior_summaries ---

    #[test]
    fn format_empty_summaries() {
        let formatted = format_prior_summaries(&[]);
        assert_eq!(formatted, "None (this is the first task)");
    }

    #[test]
    fn format_multiple_summaries() {
        let summaries = vec![
            PriorTaskSummary {
                task_id: "T01".into(),
                title: "First".into(),
                summary_path: PathBuf::from("/a/T01-SUMMARY.md"),
            },
            PriorTaskSummary {
                task_id: "T02".into(),
                title: "Second".into(),
                summary_path: PathBuf::from("/a/T02-SUMMARY.md"),
            },
        ];
        let formatted = format_prior_summaries(&summaries);
        assert!(formatted.contains("T01"));
        assert!(formatted.contains("T02"));
        assert!(formatted.contains("First"));
        assert!(formatted.contains("Second"));
    }

    // --- SessionContext construction ---

    #[test]
    fn session_context_fields() {
        let ctx = SessionContext {
            task_id: "T01".into(),
            task_title: "Fix bug".into(),
            slice_id: "S01".into(),
            slice_title: "Slice 1".into(),
            milestone_id: "M01".into(),
            task_plan: "# Plan".into(),
            slice_excerpt: "excerpt".into(),
            prior_summaries: vec![],
            working_directory: PathBuf::from("/project"),
        };
        assert_eq!(ctx.task_id, "T01");
        assert_eq!(ctx.milestone_id, "M01");
        assert!(ctx.prior_summaries.is_empty());
    }

    // --- PriorTaskSummary ---

    #[test]
    fn prior_task_summary_fields() {
        let s = PriorTaskSummary {
            task_id: "T05".into(),
            title: "Refactor module".into(),
            summary_path: PathBuf::from("/path/T05-SUMMARY.md"),
        };
        assert_eq!(s.task_id, "T05");
        assert_eq!(s.title, "Refactor module");
    }
}
