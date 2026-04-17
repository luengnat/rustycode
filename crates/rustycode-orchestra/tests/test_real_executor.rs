use rustycode_llm::MockProvider;
use rustycode_orchestra::{bootstrap_default_project, Orchestra2Executor};
use std::sync::Arc;

#[tokio::test]
async fn test_orchestra2_executor_creation() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let project_root = temp_dir.path().to_path_buf();

    bootstrap_default_project(&project_root).await.unwrap();

    let provider = Arc::new(MockProvider::from_text("mock executor response"));
    let executor =
        Orchestra2Executor::new(project_root.clone(), provider, "mock".to_string(), 10.0);

    let _ = executor;

    assert!(project_root.join(".orchestra/STATE.md").exists());
    assert!(project_root
        .join(".orchestra/milestones/M01/ROADMAP.md")
        .exists());
}
