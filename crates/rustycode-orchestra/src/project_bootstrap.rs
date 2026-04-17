//! Canonical Orchestra v2 project bootstrap helpers.
//!
//! These helpers create a minimal runnable `.orchestra/` project layout so a new
//! project can immediately enter the autonomous runtime loop.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::state_derivation::StateDeriver;

#[derive(Debug, Clone)]
pub struct BootstrapInfo {
    pub project_root: PathBuf,
    pub project_name: String,
    pub description: String,
    pub vision: String,
    pub task_title: String,
    pub task_goal: String,
    pub milestone_id: String,
    pub slice_id: String,
    pub task_id: String,
    pub roadmap_path: PathBuf,
    pub slice_plan_path: PathBuf,
    pub task_plan_path: PathBuf,
    pub state_path: PathBuf,
}

/// Bootstrap a new Orchestra project with the canonical milestone/slice/task layout.
pub async fn bootstrap_project(
    project_root: &Path,
    name: &str,
    description: &str,
    vision: &str,
    task_title: &str,
    task_goal: &str,
) -> Result<BootstrapInfo> {
    let orchestra_dir = project_root.join(".orchestra");
    let milestone_dir = orchestra_dir.join("milestones").join("M01");
    let slice_dir = milestone_dir.join("slices").join("S01");
    let task_dir = slice_dir.join("tasks").join("T01");

    tokio::fs::create_dir_all(&task_dir)
        .await
        .context("Failed to create canonical Orchestra bootstrap directories")?;

    // Write template config.json if it doesn't exist
    let config_path = orchestra_dir.join("config.json");
    if !config_path.exists() {
        let template_config = r#"{
  "_comment": "Override which model runs per task type. Delete _comment keys to activate.",
  "task_models": {}
}
"#;
        tokio::fs::write(&config_path, template_config)
            .await
            .context("Failed to write .orchestra/config.json template")?;
    }

    let roadmap = r#"# Milestone M01

## Slices
- [ ] S01: Initial improvement
"#;
    tokio::fs::write(milestone_dir.join("ROADMAP.md"), roadmap)
        .await
        .context("Failed to write milestone ROADMAP.md")?;

    let plan = r#"# Slice S01

## Tasks
- [ ] T01: Initial improvement
"#;
    tokio::fs::write(slice_dir.join("PLAN.md"), plan)
        .await
        .context("Failed to write slice PLAN.md")?;

    let task_plan = format!(
        r#"# Task T01: {}

## Goal
{}

## Project
- Name: {}
- Description: {}
- Vision: {}

## Instructions
- Make one meaningful improvement that follows the project vision.
- Keep the change scoped and shippable.
- Verify the result with concrete checks before marking the task complete.
- Update the task summary when finished.

## Verification
- Use the most relevant tests or checks for the files you change.
"#,
        task_title, task_goal, name, description, vision
    );
    tokio::fs::write(task_dir.join("T01-PLAN.md"), task_plan)
        .await
        .context("Failed to write task T01-PLAN.md")?;

    let state_deriver = StateDeriver::new(project_root.to_path_buf());
    let state = state_deriver
        .derive_state()
        .context("Failed to derive initial Orchestra state after bootstrap")?;
    state_deriver
        .write_state_cache(&state)
        .context("Failed to write initial STATE.md cache")?;

    Ok(BootstrapInfo {
        project_root: project_root.to_path_buf(),
        project_name: name.to_string(),
        description: description.to_string(),
        vision: vision.to_string(),
        task_title: task_title.to_string(),
        task_goal: task_goal.to_string(),
        milestone_id: "M01".to_string(),
        slice_id: "S01".to_string(),
        task_id: "T01".to_string(),
        roadmap_path: milestone_dir.join("ROADMAP.md"),
        slice_plan_path: slice_dir.join("PLAN.md"),
        task_plan_path: task_dir.join("T01-PLAN.md"),
        state_path: orchestra_dir.join("STATE.md"),
    })
}

/// Bootstrap a quick-task project if no canonical milestone tree exists yet.
pub async fn bootstrap_quick_task_project(
    project_root: &Path,
    task_description: &str,
) -> Result<BootstrapInfo> {
    bootstrap_project(
        project_root,
        "Quick Task Project",
        task_description,
        "Complete the requested quick task.",
        task_description,
        "Initial quick improvement",
    )
    .await
}

/// Bootstrap a default project name from the directory name when no metadata exists yet.
pub async fn bootstrap_default_project(project_root: &Path) -> Result<BootstrapInfo> {
    let fallback_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Orchestra Project");

    bootstrap_project(
        project_root,
        fallback_name,
        "Autonomous Orchestra project",
        "Make the first meaningful improvement in this repository.",
        "Initial improvement",
        "Seed the repository with a first runnable Orchestra task",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn bootstrap_project_creates_canonical_tree() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let info = bootstrap_project(
            root,
            "Test Project",
            "A project for bootstrap testing",
            "Improve the test harness",
            "Bootstrap task",
            "Seed the canonical Orchestra project structure",
        )
        .await
        .unwrap();

        let milestone_dir = root.join(".orchestra/milestones/M01");
        let slice_dir = milestone_dir.join("slices/S01");
        let task_dir = slice_dir.join("tasks/T01");

        assert!(milestone_dir.join("ROADMAP.md").exists());
        assert!(slice_dir.join("PLAN.md").exists());
        assert!(task_dir.join("T01-PLAN.md").exists());
        assert!(root.join(".orchestra/STATE.md").exists());
        assert_eq!(info.project_name, "Test Project");
        assert_eq!(info.task_id, "T01");
        assert_eq!(info.milestone_id, "M01");

        let state = StateDeriver::new(root.to_path_buf())
            .derive_state()
            .unwrap();
        assert_eq!(
            state.active_task.as_ref().map(|t| t.id.as_str()),
            Some("T01")
        );
        assert_eq!(
            state.active_slice.as_ref().map(|s| s.id.as_str()),
            Some("S01")
        );
        assert_eq!(
            state.active_milestone.as_ref().map(|m| m.id.as_str()),
            Some("M01")
        );
        assert_eq!(state.phase.as_str(), "execute");
    }

    #[tokio::test]
    async fn bootstrap_default_project_uses_directory_name() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("demo-project");
        tokio::fs::create_dir_all(&root).await.unwrap();

        let info = bootstrap_default_project(&root).await.unwrap();

        let state = StateDeriver::new(root.clone()).derive_state().unwrap();
        assert_eq!(
            state.active_task.as_ref().map(|t| t.id.as_str()),
            Some("T01")
        );
        assert_eq!(
            state.active_milestone.as_ref().map(|m| m.id.as_str()),
            Some("M01")
        );
        assert_eq!(info.project_name, "demo-project");
    }
}
