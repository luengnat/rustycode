//! Orchestra v2 command entry points for the main CLI.
//!
//! These functions are thin adapters over the shared Orchestra v2 service so the
//! CLI and TUI stay aligned on project bootstrap and runtime behavior.

use anyhow::Result;
use rustycode_orchestra::orchestra_service::OrchestraService;
use std::path::PathBuf;

/// Execute autonomous development workflow
pub async fn run_auto_mode(project_root: PathBuf, budget: f64, _max_units: u32) -> Result<()> {
    println!("🚀 Orchestra Auto Mode - Autonomous Development");
    println!("   Root: {}", project_root.display());
    println!("   Budget: ${:.2}", budget);
    println!();

    println!("▶️  Starting autonomous execution...");
    println!();

    if let Some(info) = OrchestraService::run_auto(project_root, budget).await? {
        println!("🧭 Bootstrapped project: {}", info.project_name);
        println!("   🧩 Task: {}", info.task_title);
        println!("   🎯 Goal: {}", info.task_goal);
        println!("   📄 Plan: {}", info.task_plan_path.display());
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Execution complete");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

/// Quick task execution (single unit)
pub async fn run_quick_task(
    project_root: PathBuf,
    task_description: String,
    _model: Option<String>,
) -> Result<()> {
    println!("⚡ Orchestra Quick Task");
    println!("   Task: {}", task_description);
    println!("   Root: {}", project_root.display());
    println!();

    println!("▶️  Starting quick task execution...");
    if let Some(info) =
        OrchestraService::run_quick_task(project_root, task_description, 10.0).await?
    {
        println!("🧭 Bootstrapped project: {}", info.project_name);
        println!("   🧩 Task: {}", info.task_title);
        println!("   🎯 Goal: {}", info.task_goal);
        println!("   📄 Plan: {}", info.task_plan_path.display());
    }

    println!();
    println!("✅ Quick task execution complete!");

    Ok(())
}

/// Show project progress
pub async fn show_progress(project_root: PathBuf) -> Result<()> {
    println!("📊 Orchestra Progress Report");
    println!("   Root: {}", project_root.display());
    println!();

    let state_path = project_root.join(".orchestra/STATE.md");
    if state_path.exists() {
        let state_content = tokio::fs::read_to_string(&state_path).await?;
        println!("{}", state_content);
    } else {
        println!("   No STATE.md found. Initialize with 'orchestra init'");
    }

    Ok(())
}

/// Initialize Orchestra project
pub async fn init_project(
    project_root: PathBuf,
    name: String,
    description: String,
    vision: String,
) -> Result<()> {
    println!("🚀 Initializing Orchestra Project");
    println!("   Name: {}", name);
    println!("   Root: {}", project_root.display());
    println!();

    let info = OrchestraService::init_project(&project_root, &name, &description, &vision).await?;

    println!("✅ Project initialized successfully!");
    println!("   📁 Created .orchestra/ directory");
    println!("   📄 Created milestone, slice, and first task");
    println!("   🧩 Task: {}", info.task_title);
    println!("   🎯 Goal: {}", info.task_goal);
    println!("   📄 Plan: {}", info.task_plan_path.display());
    println!();
    println!("Next steps:");
    println!("  1. Run 'orchestra auto' to start autonomous development");
    println!("  2. Or run 'orchestra quick \"...\"' for a one-off improvement");

    Ok(())
}
