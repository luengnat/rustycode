//! Slice plan (PLAN.md) parser
//!
//! Parses plan files containing:
//! - Slice ID and title
//! - Goal and demo specifications
//! - Must-have requirements
//! - Task breakdown with estimates

use crate::files::parsers::common::{extract_section, parse_bullets};
use regex::Regex;
use std::sync::LazyLock;

static CB_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^-\s+\[([xX ])\]\s+\*\*([\w.]+):\s+(.+?)\*\*\s*(.*)"#).unwrap());
static EST_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"`est:([^`]+)`"#).unwrap());
static FILES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*-\s+Files:\s*(.*)").unwrap());
static VERIFY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*-\s+Verify:\s*(.*)").unwrap());

/// Slice plan structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SlicePlan {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub demo: String,
    pub must_haves: Vec<String>,
    pub tasks: Vec<TaskPlanEntry>,
    pub files_likely_touched: Vec<String>,
}

/// Task plan entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskPlanEntry {
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

/// Parse slice plan
pub fn parse_plan(content: &str) -> SlicePlan {
    let lines: Vec<&str> = content.lines().collect();

    let h1 = lines.iter().find(|l| l.starts_with("# "));
    let (mut id, mut title) = (String::new(), String::new());

    if let Some(h1_line) = h1 {
        let h1_re = Regex::new(r"^#\s+(\w+):\s+(.+)").unwrap();
        if let Some(caps) = h1_re.captures(h1_line) {
            id = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            title = caps
                .get(2)
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
        } else {
            title = h1_line[2..].trim().to_string();
        }
    }

    let goal = extract_bold_field(content, "Goal").unwrap_or_default();
    let demo = extract_bold_field(content, "Demo").unwrap_or_default();

    let mh_section = extract_section(content, "Must-Haves", 2);
    let must_haves = mh_section.map(|s| parse_bullets(&s)).unwrap_or_default();

    let tasks_section = extract_section(content, "Tasks", 2);
    let tasks = parse_tasks(tasks_section.as_deref().unwrap_or(""));

    let files_section = extract_section(content, "Files Likely Touched", 2);
    let files_likely_touched = files_section.map(|s| parse_bullets(&s)).unwrap_or_default();

    SlicePlan {
        id,
        title,
        goal,
        demo,
        must_haves,
        tasks,
        files_likely_touched,
    }
}

/// Extract a bold field value from markdown
///
/// Looks for `**key:** value` patterns and returns the value
fn extract_bold_field(text: &str, key: &str) -> Option<String> {
    let pattern = format!(r"(?m)^\*\*{}:\*\*\s*(.+)$", regex_escape(key));
    let re = Regex::new(&pattern).unwrap();
    re.captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string())
}

fn parse_tasks(tasks_section: &str) -> Vec<TaskPlanEntry> {
    let mut tasks = Vec::new();
    let mut current_task: Option<TaskPlanEntry> = None;

    for line in tasks_section.lines() {
        if let Some(caps) = CB_RE.captures(line) {
            if let Some(task) = current_task.take() {
                tasks.push(task);
            }

            let rest = caps.get(4).map(|m| m.as_str()).unwrap_or("");
            let estimate = EST_RE
                .captures(rest)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            current_task = Some(TaskPlanEntry {
                id: caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string(),
                title: caps.get(3).map(|m| m.as_str()).unwrap_or("").to_string(),
                description: String::new(),
                done: caps.get(1).map(|m| m.as_str()) == Some("x")
                    || caps.get(1).map(|m| m.as_str()) == Some("X"),
                estimate,
                files: None,
                verify: None,
            });
        } else if let Some(task) = &mut current_task {
            if let Some(caps) = FILES_RE.captures(line) {
                let files_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                task.files = Some(
                    files_str
                        .split(',')
                        .map(|f| f.replace('`', "").trim().to_string())
                        .filter(|f| !f.is_empty())
                        .collect(),
                );
            }

            if let Some(caps) = VERIFY_RE.captures(line) {
                task.verify = Some(
                    caps.get(1)
                        .map(|m| m.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                );
            }

            let trimmed = line.trim();
            if !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !FILES_RE.is_match(line)
                && !VERIFY_RE.is_match(line)
            {
                if task.description.is_empty() {
                    task.description = trimmed.to_string();
                } else {
                    task.description.push(' ');
                    task.description.push_str(trimmed);
                }
            }
        }
    }

    if let Some(task) = current_task {
        tasks.push(task);
    }

    tasks
}

/// Escape regex special characters
fn regex_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '[' | ']' | '\\' | '|' => {
                format!("\\{}", c)
            }
            _ => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plan_basic() {
        let content = r#"# S01: Build Feature

**Goal:** Create awesome feature

**Demo:** Show it working

## Must-Haves

- First requirement
- Second requirement

## Tasks

- [ ] **T01: Setup project** `est:5m`
  Description of setup task
  - Files: Cargo.toml
  - Verify: cargo build

- [x] **T02: Implement core** `est:15m`
  This is done

## Files Likely Touched

- src/main.rs
- src/lib.rs
"#;

        let plan = parse_plan(content);

        assert_eq!(plan.id, "S01");
        assert_eq!(plan.title, "Build Feature");
        assert_eq!(plan.goal, "Create awesome feature");
        assert_eq!(plan.demo, "Show it working");
        assert_eq!(plan.must_haves.len(), 2);
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].id, "T01");
        assert!(!plan.tasks[0].done);
        assert_eq!(plan.tasks[1].id, "T02");
        assert!(plan.tasks[1].done);
        assert_eq!(plan.files_likely_touched.len(), 2);
    }

    #[test]
    fn test_parse_task_with_files_and_verify() {
        let content = r#"# Test

**Goal:** Test

## Tasks

- [ ] **T01: Task** `est:10m`
  Task description
  - Files: src/a.rs, src/b.rs
  - Verify: cargo test
"#;

        let plan = parse_plan(content);

        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].description, "Task description");
        assert!(plan.tasks[0].files.is_some());
        assert_eq!(plan.tasks[0].files.as_ref().unwrap().len(), 2);
        assert_eq!(plan.tasks[0].files.as_ref().unwrap()[0], "src/a.rs");
        assert_eq!(plan.tasks[0].verify.as_ref().unwrap(), "cargo test");
    }

    #[test]
    fn test_parse_plan_without_id() {
        let content = r#"# Just a Title

**Goal:** Test

## Tasks

- [ ] **T01: Task** `est:5m`
"#;

        let plan = parse_plan(content);

        assert_eq!(plan.id, "");
        assert_eq!(plan.title, "Just a Title");
    }

    // --- Serde roundtrips ---

    #[test]
    fn slice_plan_serde_roundtrip() {
        let plan = SlicePlan {
            id: "S01".into(),
            title: "Build".into(),
            goal: "Create feature".into(),
            demo: "Show it".into(),
            must_haves: vec!["auth".into()],
            tasks: vec![TaskPlanEntry {
                id: "T01".into(),
                title: "Task".into(),
                description: "Do it".into(),
                done: false,
                estimate: "5m".into(),
                files: Some(vec!["src/main.rs".into()]),
                verify: Some("cargo test".into()),
            }],
            files_likely_touched: vec!["src/main.rs".into()],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let decoded: SlicePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "S01");
        assert_eq!(decoded.tasks.len(), 1);
        assert_eq!(decoded.tasks[0].verify, Some("cargo test".into()));
    }

    #[test]
    fn task_plan_entry_serde_roundtrip() {
        let entry = TaskPlanEntry {
            id: "T02".into(),
            title: "Fix bug".into(),
            description: "Fix the thing".into(),
            done: true,
            estimate: "10m".into(),
            files: None,
            verify: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: TaskPlanEntry = serde_json::from_str(&json).unwrap();
        assert!(decoded.done);
        assert!(decoded.files.is_none());
    }

    // --- Parse edge cases ---

    #[test]
    fn parse_plan_empty() {
        let plan = parse_plan("");
        assert!(plan.id.is_empty());
        assert!(plan.tasks.is_empty());
        assert!(plan.must_haves.is_empty());
    }

    #[test]
    fn parse_plan_no_tasks() {
        let content = "# S01: Empty\n\n**Goal:** Nothing\n";
        let plan = parse_plan(content);
        assert_eq!(plan.id, "S01");
        assert!(plan.tasks.is_empty());
    }

    #[test]
    fn parse_plan_task_no_estimate() {
        let content =
            "# Test\n\n**Goal:** G\n\n## Tasks\n\n- [ ] **T01: Task**\n  Description only\n";
        let plan = parse_plan(content);
        assert_eq!(plan.tasks.len(), 1);
        assert!(plan.tasks[0].estimate.is_empty());
    }
}
