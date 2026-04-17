//! Integration tests for AutoMode with UnifiedToolExecutor integration
//!
//! These tests verify that AutoMode:
//! 1. Respects plan mode gating (read-only planning phase before implementation)
//! 2. Generates and approves plans
//! 3. Uses executor for modifications with proper cost tracking
//! 4. Tracks costs across all tool calls

use rustycode_orchestra::auto::AutoMode;

/// Test that AutoMode enforces plan approval before modifications
#[tokio::test]
async fn test_auto_mode_respects_plan_phase() {
    let auto = AutoMode::with_plan_enforcement();

    // Auto is in planning phase by default
    assert!(auto.current_phase() == "planning");

    // Attempting to modify a file should require plan approval first
    let result = auto.execute_task("Add error handling to main.rs").await;

    // Should fail because plan hasn't been approved yet
    assert!(result.is_err() || !result.as_ref().unwrap().success);
}

/// Test that AutoMode generates plans with cost estimates
#[tokio::test]
async fn test_auto_mode_estimates_cost_in_plan() {
    let auto = AutoMode::with_cost_tracking();

    let plan = auto.generate_plan("Refactor authentication module").await;

    assert!(plan.is_ok());
    let plan = plan.unwrap();

    // Plan should have a valid ID and summary
    assert!(!plan.id.is_empty());
    assert!(!plan.summary.is_empty());

    // Estimated cost should be non-negative
    assert!(plan.estimated_cost_usd >= 0.0);

    // Should have success criteria
    assert!(!plan.success_criteria.is_empty());
}

/// Test that AutoMode uses executor for modifications with proper gating
#[tokio::test]
async fn test_auto_mode_uses_executor_for_modifications() {
    let auto = AutoMode::with_plan_enforcement();

    // Generate plan
    let plan = auto
        .generate_plan("Fix critical bug in parser")
        .await
        .unwrap();
    assert!(!plan.id.is_empty());

    // Approve plan to move to implementation phase
    let result = auto.approve_plan(&plan).await;
    assert!(result.is_ok());

    // Now execute the plan
    let exec_result = auto.execute_plan(&plan).await;

    // Should succeed (with mock executor)
    assert!(exec_result.is_ok());
    let result = exec_result.unwrap();

    assert!(result.success);
    assert!(result.cost >= 0.0);
}

/// Test that AutoMode tracks costs across multiple file modifications
#[tokio::test]
async fn test_auto_mode_tracks_multiple_modifications() {
    let auto = AutoMode::with_cost_tracking();

    let plan = auto
        .generate_plan("Add logging to all modules")
        .await
        .unwrap();

    // Approve plan
    auto.approve_plan(&plan).await.unwrap();

    // Execute plan
    let result = auto.execute_plan(&plan).await.unwrap();

    // Should track cost for all modifications
    assert!(result.success);
    assert!(result.cost >= 0.0);

    // If plan has multiple files, cost should be accumulated
    if !plan.files_to_modify.is_empty() {
        // Cost should reflect multiple operations
        assert!(result.files_modified > 0);
    }
}

/// Test that plan approval transitions from planning to implementation phase
#[tokio::test]
async fn test_auto_mode_phase_transition() {
    let auto = AutoMode::with_plan_enforcement();

    // Initially in planning phase
    assert_eq!(auto.current_phase(), "planning");

    // Generate plan
    let plan = auto.generate_plan("Add feature X").await.unwrap();

    // Still in planning phase after generation
    assert_eq!(auto.current_phase(), "planning");

    // Approve plan
    auto.approve_plan(&plan).await.unwrap();

    // Now in implementation phase
    assert_eq!(auto.current_phase(), "implementation");
}

/// Test that rejected plans return to planning phase
#[tokio::test]
async fn test_auto_mode_reject_plan() {
    let auto = AutoMode::with_plan_enforcement();

    let plan = auto.generate_plan("Something risky").await.unwrap();

    // Reject the plan
    auto.reject_plan(&plan).await.ok();

    // Should return to planning phase
    assert_eq!(auto.current_phase(), "planning");
}

/// Test that task execution without approval fails
#[tokio::test]
async fn test_auto_mode_task_without_approval() {
    let auto = AutoMode::with_plan_enforcement();

    // Try to execute task directly without approving a plan
    let result = auto.execute_task("Direct modification").await;

    // Should fail
    assert!(result.is_err() || !result.as_ref().unwrap().success);
}

/// Test TaskResult fields
#[tokio::test]
async fn test_task_result_fields() {
    let auto = AutoMode::with_plan_enforcement();

    let plan = auto.generate_plan("Test task").await.unwrap();
    auto.approve_plan(&plan).await.unwrap();
    let result = auto.execute_plan(&plan).await.unwrap();

    // All fields should be populated
    assert!(result.success);
    assert!(result.cost >= 0.0);
    // In planning phase before approval, would require_approval == true
    // After approval, it's false (approval already granted)
    assert!(!result.requires_approval);
}
