// Autonomous Mode canonical executor smoke check.
//
// This example bootstraps a temporary Orchestra project and constructs the
// canonical Orchestra2Executor with a mock provider so the flow stays offline.

use rustycode_llm::MockProvider;
use rustycode_orchestra::{bootstrap_default_project, Orchestra2Executor};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Autonomous Mode canonical executor smoke check");
    println!("====================================\n");

    let project_root = std::env::temp_dir().join(format!(
        "rustycode-orchestra-example-{}",
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&project_root).await?;

    bootstrap_default_project(&project_root).await?;

    let provider = Arc::new(MockProvider::from_text("mock execution"));
    let executor =
        Orchestra2Executor::new(project_root.clone(), provider, "mock".to_string(), 10.0);

    let _ = executor;

    println!("✅ Bootstrapped project: {}", project_root.display());
    println!("✅ Constructed canonical Orchestra2Executor");
    println!("\nTo run a real task, use `orchestra init` followed by `orchestra auto`.");

    Ok(())
}
