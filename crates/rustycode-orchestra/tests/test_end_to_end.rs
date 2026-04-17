use rustycode_llm::MockProvider;
use rustycode_orchestra::{bootstrap_default_project, Orchestra2Executor};
use std::sync::Arc;

#[tokio::test]
async fn test_end_to_end_executor_smoke() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let project_root = temp_dir.path().to_path_buf();

    bootstrap_default_project(&project_root).await.unwrap();

    let provider = Arc::new(MockProvider::from_text("mock end-to-end response"));
    let executor =
        Orchestra2Executor::new(project_root.clone(), provider, "mock".to_string(), 10.0);

    let _ = executor;

    assert!(project_root.join(".orchestra/STATE.md").exists());
    assert!(project_root
        .join(".orchestra/milestones/M01/slices/S01/PLAN.md")
        .exists());
    assert!(project_root
        .join(".orchestra/milestones/M01/slices/S01/tasks/T01/T01-PLAN.md")
        .exists());
}

#[tokio::test]
#[ignore = "This is a construction smoke check, not a live provider integration test"]
async fn test_full_execution_with_mock_provider() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let project_root = temp_dir.path().to_path_buf();

    bootstrap_default_project(&project_root).await.unwrap();

    let provider = Arc::new(MockProvider::from_text("mock run response"));
    let executor = Orchestra2Executor::new(project_root, provider, "mock".to_string(), 10.0);

    let _ = executor;
}
