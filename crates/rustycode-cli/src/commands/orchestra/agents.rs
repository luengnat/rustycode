// rustycode-cli/src/commands/orchestra_agents.rs
//! Orchestra Agent commands for autonomous execution
//!
//! Migrated to Orchestra v2 API
use anyhow::Result;
use rustycode_orchestra::OrchestraService;
use std::path::PathBuf;

/// Plan a phase with AI assistance - delegates to Orchestra v2 auto mode
pub fn plan_phase_agent(
    project_root: PathBuf,
    phase_id: String,
    milestone_id: String,
    title: String,
    goal: String,
    demo: String,
    _risk: String,
) -> Result<()> {
    println!("🤖 Planning phase with AI assistance: {}", title);
    println!("   Orchestra v2 uses autonomous execution - planning is built into execution");

    let orchestra_dir = project_root.join(".orchestra");
    let slice_dir = orchestra_dir
        .join("milestones")
        .join(&milestone_id)
        .join("slices")
        .join(&phase_id);
    std::fs::create_dir_all(slice_dir.join("tasks"))?;

    let plan_content = format!(
        "# Slice {}\n\n## Goal\n{}\n\n## Demo\n{}\n\n## Tasks\n- [ ] T01:\n",
        phase_id, goal, demo
    );
    std::fs::write(slice_dir.join("PLAN.md"), plan_content)?;

    println!("✅ Phase plan created!");
    println!("   Phase: {}", phase_id);
    println!("   Title: {}", title);
    println!("   Goal: {}", goal);
    println!("\n   Run 'orchestra auto' to execute with AI assistance");
    Ok(())
}

/// Execute a phase with autonomous agents
pub fn execute_phase_agent(
    project_root: PathBuf,
    _phase_id: String,
    _milestone_id: String,
) -> Result<()> {
    println!("🤖 Executing phase with autonomous agents");
    println!("   Using Orchestra v2 autonomous execution...\n");

    let rt = tokio::runtime::Runtime::new()?;
    match rt.block_on(OrchestraService::run_auto(project_root, 10.0)) {
        Ok(info) => {
            if let Some(i) = info {
                println!("✅ Phase execution completed!");
                println!("   Task: {}", i.task_title);
                println!("   Goal: {}", i.task_goal);
            } else {
                println!("✅ No tasks to execute");
            }
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("Failed to execute phase: {}", e);
        }
    }
}

/// Verify a phase with AI assistance
pub fn verify_phase_agent(
    project_root: PathBuf,
    phase_id: String,
    milestone_id: String,
) -> Result<()> {
    println!("🔍 Verifying phase with AI assistance: {}", phase_id);

    let slice_plan = project_root
        .join(".orchestra")
        .join("milestones")
        .join(&milestone_id)
        .join("slices")
        .join(&phase_id)
        .join("PLAN.md");

    if slice_plan.exists() {
        println!("\n📋 Verification: Phase {} exists", phase_id);
        println!("   Run 'orchestra auto' to continue execution");
    } else {
        println!("⚠️  Phase {} not found", phase_id);
    }

    Ok(())
}
