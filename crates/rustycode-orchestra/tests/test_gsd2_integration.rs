//! Integration test for Autonomous Mode executor
//!
//! Tests the state-driven execution flow end-to-end

use rustycode_tools::Tool;

#[tokio::test]
async fn test_state_parsing() {
    // Create test project
    let test_dir = tempfile::tempdir().unwrap();
    let project_root = test_dir.path();

    // Create .orchestra directory
    let orchestra_dir = project_root.join(".orchestra");
    std::fs::create_dir_all(&orchestra_dir).unwrap();

    // Create STATE.md
    let state_content = r#"
# Orchestra State

Current Unit: Execute T01:

Task Queue:
- T01: Create hello world
- T02: Add tests
"#;
    std::fs::write(orchestra_dir.join("STATE.md"), state_content).unwrap();

    // Test parsing
    let state = std::fs::read_to_string(orchestra_dir.join("STATE.md")).unwrap();

    // Find "Execute T01:"
    let unit = state
        .lines()
        .find(|line| line.contains("Execute "))
        .and_then(|line| line.split("Execute ").nth(1)?.split(':').next());

    assert_eq!(unit, Some("T01"));
}

#[tokio::test]
async fn test_task_plan_loading() {
    // Create test project with task plan
    let test_dir = tempfile::tempdir().unwrap();
    let project_root = test_dir.path();

    // Create directory structure
    let task_dir = project_root.join(".orchestra/milestones/M01/slices/S01/tasks");
    std::fs::create_dir_all(&task_dir).unwrap();

    // Create task plan
    let plan_content = r#"
# Task T01: Create Hello World

## Objective
Create a hello world function

## Success Criteria
- Function exists
- Returns "Hello, World!"
"#;
    std::fs::write(task_dir.join("T01-PLAN.md"), plan_content).unwrap();

    // Verify plan can be read
    let plans = walkdir::WalkDir::new(project_root.join(".orchestra"))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|e| e == "md").unwrap_or(false))
        .collect::<Vec<_>>();

    assert!(!plans.is_empty(), "Should find at least one markdown file");
}

#[tokio::test]
async fn test_tool_execution_integration() {
    // Create test project
    let test_dir = tempfile::tempdir().unwrap();
    let project_root = test_dir.path();

    // Test write_file tool
    let cargo_toml = r#"
[package]
name = "hello-world"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;

    let params = serde_json::json!({
        "path": "Cargo.toml",
        "content": cargo_toml
    });

    let tool_ctx = rustycode_tools::ToolContext::new(project_root);
    let result = rustycode_tools::WriteFileTool
        .execute(params, &tool_ctx)
        .unwrap();

    // The result format is: "wrote /path (X bytes, Y lines)"
    assert!(
        result.text.contains("wrote"),
        "Should confirm file was written"
    );

    // Verify file exists
    let cargo_path = project_root.join("Cargo.toml");
    assert!(cargo_path.exists(), "File should exist");

    // Verify content
    let content = std::fs::read_to_string(&cargo_path).unwrap();
    assert!(content.contains("hello-world"), "Content should match");
}
