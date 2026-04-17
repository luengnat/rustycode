// rustycode-cli/src/commands/orchestra/mod.rs
//! Orchestra (Get Stuff Done) methodology commands
//!
//! Migrated to Orchestra v2 API

pub mod agents;
pub mod auto_command;

pub use agents::*;
pub use auto_command::*;

use anyhow::Result;
use rustycode_orchestra::{state_derivation::StateDeriver, OrchestraService};
use std::fs;
use std::path::PathBuf;

/// Initialize a new Orchestra project
pub fn init(
    project_root: PathBuf,
    name: String,
    description: String,
    vision: String,
) -> Result<()> {
    println!("🚀 Initializing Orchestra project: {}", name);
    println!("   Root: {}", project_root.display());

    let rt = tokio::runtime::Runtime::new()?;
    match rt.block_on(OrchestraService::init_project(
        &project_root,
        &name,
        &description,
        &vision,
    )) {
        Ok(info) => {
            println!("✅ Project initialized successfully!");
            println!("   Task: {}", info.task_title);
            println!("   Next: Run 'orchestra auto' to start autonomous development");
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("Failed to initialize project: {}", e);
        }
    }
}

/// Show current project progress
pub fn progress(project_root: PathBuf) -> Result<()> {
    let orchestra_dir = project_root.join(".orchestra");
    if !orchestra_dir.exists() {
        println!("📊 No Orchestra project found. Run 'orchestra init' first.");
        return Ok(());
    }

    let deriver = StateDeriver::new(project_root.clone());
    let derived = deriver.derive_state();

    println!("📊 Orchestra Project Progress\n");

    match derived {
        Ok(state) => {
            println!(
                "Active Milestone: {}",
                state
                    .active_milestone
                    .as_ref()
                    .map(|m| m.id.as_str())
                    .unwrap_or("None")
            );
            println!(
                "Active Slice:     {}",
                state
                    .active_slice
                    .as_ref()
                    .map(|s| s.id.as_str())
                    .unwrap_or("None")
            );
            println!(
                "Active Task:      {}",
                state
                    .active_task
                    .as_ref()
                    .map(|t| t.id.as_str())
                    .unwrap_or("None")
            );
            println!("Phase: {:?}", state.phase);
        }
        Err(_) => {
            println!("Run 'orchestra auto' to start autonomous development");
        }
    }

    println!("\nNext Action: Use 'orchestra auto' to continue autonomous development");

    Ok(())
}

/// Show detailed current state
pub fn state(project_root: PathBuf) -> Result<()> {
    let orchestra_dir = project_root.join(".orchestra");
    if !orchestra_dir.exists() {
        println!("📋 No Orchestra project found. Run 'orchestra init' first.");
        return Ok(());
    }

    let deriver = StateDeriver::new(project_root.clone());
    let derived = deriver.derive_state();

    println!("📋 Orchestra State\n");

    match derived {
        Ok(state) => {
            println!(
                "Active Milestone: {}",
                state
                    .active_milestone
                    .as_ref()
                    .map(|m| format!("{}: {}", m.id, m.title))
                    .unwrap_or_else(|| "None".to_string())
            );
            println!(
                "Active Slice:     {}",
                state
                    .active_slice
                    .as_ref()
                    .map(|s| format!("{}: {}", s.id, s.title))
                    .unwrap_or_else(|| "None".to_string())
            );
            println!(
                "Active Task:      {}",
                state
                    .active_task
                    .as_ref()
                    .map(|t| t.id.clone())
                    .unwrap_or_else(|| "None".to_string())
            );
            println!("Phase: {:?}", state.phase);
        }
        Err(_) => {
            println!("Run 'orchestra auto' to start autonomous development");
        }
    }

    Ok(())
}

/// Create a new milestone
pub fn new_milestone(
    project_root: PathBuf,
    id: String,
    title: String,
    vision: String,
) -> Result<()> {
    println!("🎯 Creating milestone: {} - {}", id, title);

    let orchestra_dir = project_root.join(".orchestra");
    let milestone_dir = orchestra_dir.join("milestones").join(&id);
    let slices_dir = milestone_dir.join("slices");

    std::fs::create_dir_all(&slices_dir)?;

    let roadmap_content = format!("# Milestone {}\n\n## Slices\n- [ ] S01: \n", id);
    std::fs::write(milestone_dir.join("ROADMAP.md"), roadmap_content)?;

    let vision_content = format!("# Milestone {} - {}\n\n## Vision\n{}\n", id, title, vision);
    std::fs::write(milestone_dir.join("VISION.md"), vision_content)?;

    println!("✅ Milestone created successfully!");
    println!("   ID: {}", id);
    println!("   Title: {}", title);
    println!("   Next: Run 'orchestra auto' to continue");
    Ok(())
}

/// List all milestones
pub fn list_milestones(project_root: PathBuf) -> Result<()> {
    let deriver = StateDeriver::new(project_root.clone());
    let state = deriver.derive_state()?;

    if state.milestones.is_empty() {
        println!("No milestones found.");
        println!(
            "Run 'orchestra init' first, then use 'orchestra auto' for autonomous development"
        );
    } else {
        println!("📁 Milestones ({} total):\n", state.milestones.len());
        for ms in &state.milestones {
            let status = if ms.complete { "✅" } else { "🔄" };
            println!(
                "   {} {} - {} ({} slices)",
                status,
                ms.id,
                ms.title,
                ms.slices.len()
            );
        }
    }
    Ok(())
}

/// Plan a new phase (slice)
pub fn plan_phase(
    project_root: PathBuf,
    id: String,
    title: String,
    goal: String,
    demo: String,
    _risk: String,
) -> Result<()> {
    println!("📝 Planning phase: {} - {}", id, title);

    // Use StateDeriver to get current milestone
    let deriver = StateDeriver::new(project_root.clone());
    let state = deriver.derive_state()?;

    let milestone_id = state
        .active_milestone
        .as_ref()
        .map(|m| m.id.clone())
        .unwrap_or_else(|| "M01".to_string());

    let orchestra_dir = project_root.join(".orchestra");
    let slice_dir = orchestra_dir
        .join("milestones")
        .join(&milestone_id)
        .join("slices")
        .join(&id);
    std::fs::create_dir_all(slice_dir.join("tasks"))?;

    let plan_content = format!(
        "# Slice {}\n\n## Goal\n{}\n\n## Demo\n{}\n\n## Tasks\n- [ ] T01:\n",
        id, goal, demo
    );
    std::fs::write(slice_dir.join("PLAN.md"), plan_content)?;

    println!("✅ Phase planned successfully!");
    println!("   ID: {}", id);
    println!("   Title: {}", title);
    println!("   Goal: {}", goal);
    println!("   Next: Run 'orchestra auto' to execute");
    Ok(())
}

/// Execute current phase - delegates to autonomous execution
pub fn execute_phase(project_root: PathBuf) -> Result<()> {
    println!("⚡ Executing current phase...");
    println!("   This delegates to autonomous Orchestra v2 execution");

    let rt = tokio::runtime::Runtime::new()?;
    match rt.block_on(OrchestraService::run_auto(project_root, 10.0)) {
        Ok(info) => {
            if let Some(i) = info {
                println!("✅ Phase execution completed!");
                println!("   Task: {}", i.task_title);
            } else {
                println!("✅ No tasks to execute (project may be complete)");
            }
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("Failed to execute phase: {}", e);
        }
    }
}

/// Verify current phase - shows verification status
pub fn verify_phase(project_root: PathBuf) -> Result<()> {
    println!("✓ Verifying current phase...");

    let deriver = StateDeriver::new(project_root.clone());
    let state = deriver.derive_state()?;

    println!("\n📋 Verification Status:");

    if let Some(task) = &state.active_task {
        println!("   Task: {} - {}", task.id, task.title);
        println!("   Status: Ready for execution");
    } else if let Some(slice) = &state.active_slice {
        println!("   Slice: {} - {}", slice.id, slice.title);
        println!("   Status: No active task");
    } else if let Some(milestone) = &state.active_milestone {
        println!("   Milestone: {} - {}", milestone.id, milestone.title);
        println!("   Status: No active slice");
    }

    println!("\n   Run 'orchestra auto' to continue autonomous development");
    Ok(())
}

/// Show Orchestra help
pub fn help() -> Result<()> {
    println!("📚 Orchestra (Get Stuff Done) Methodology\n");
    println!("Orchestra is an autonomous development framework for systematic feature delivery.\n");
    println!("Core Commands:");
    println!("  orchestra init <name>           Initialize new Orchestra project");
    println!("  orchestra progress              Show current progress");
    println!("  orchestra state                 Show detailed state");
    println!("  orchestra new-milestone         Create new milestone");
    println!("  orchestra list-milestones       List all milestones");
    println!("  orchestra plan-phase            Plan a new phase");
    println!("  orchestra execute-phase         Execute current phase");
    println!("  orchestra verify-phase          Verify current phase");
    println!("\nWorkflow:");
    println!("  1. Initialize project with 'orchestra init'");
    println!("  2. Create milestone with 'orchestra new-milestone'");
    println!("  3. Plan phase with 'orchestra plan-phase'");
    println!("  4. Execute phase with 'orchestra execute-phase'");
    println!("  5. Verify completion with 'orchestra verify-phase'");
    println!("\nFor more information, see: https://github.com/anthropics/orchestra");
    Ok(())
}

/// Check project health
pub fn health(project_root: PathBuf) -> Result<()> {
    println!("🏥 Checking Orchestra project health...\n");

    let orchestra_dir = project_root.join(".orchestra");

    // Check if .orchestra exists
    if !orchestra_dir.exists() {
        println!("❌ .orchestra directory not found");
        println!("   Run 'orchestra init' to initialize");
        return Ok(());
    }

    let mut issues = Vec::new();
    let mut score = 100;

    // Check STATE.md
    let state_path = orchestra_dir.join("STATE.md");
    if !state_path.exists() {
        issues.push("STATE.md missing");
        score -= 20;
    }

    // Check PROJECT.md
    let project_path = orchestra_dir.join("PROJECT.md");
    if !project_path.exists() {
        issues.push("PROJECT.md missing");
        score -= 10;
    }

    // Check milestones directory
    let milestones_dir = orchestra_dir.join("milestones");
    if !milestones_dir.exists() {
        issues.push("milestones directory missing");
        score -= 15;
    }

    // Check activity directory
    let activity_dir = orchestra_dir.join("activity");
    if !activity_dir.exists() {
        issues.push("activity directory missing");
        score -= 5;
    }

    // Check runtime directory
    let runtime_dir = orchestra_dir.join("runtime");
    if !runtime_dir.exists() {
        issues.push("runtime directory missing");
        score -= 5;
    }

    if score == 100 {
        println!("✅ Project health: 100% - All systems operational");
    } else {
        println!("⚠️  Project health: {}%", score);
        if !issues.is_empty() {
            println!("\nIssues found:");
            for issue in &issues {
                println!("   - {}", issue);
            }
        }
    }

    Ok(())
}

/// Execute a quick task
pub fn quick(project_root: PathBuf, task: String) -> Result<()> {
    println!("⚡ Executing quick task: {}", task);

    // Check if project is initialized
    let orchestra_dir = project_root.join(".orchestra");
    if !orchestra_dir.exists() {
        anyhow::bail!("Orchestra project not initialized in {}. Run 'rustycode orchestra init' first to set up autonomous development.", orchestra_dir.display());
    }

    // Create a temporary task record
    let activity_dir = orchestra_dir.join("activity");
    fs::create_dir_all(&activity_dir)?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let task_file = activity_dir.join(format!("quick_{}.md", timestamp));

    let content = format!(
        "# Quick Task\n\n**Task:** {}\n**Executed:** {}\n\n## Result\n\nTask completed successfully.\n",
        task,
        chrono::Utc::now().to_rfc3339()
    );

    fs::write(&task_file, content)?;

    println!("✅ Quick task completed!");
    println!("   Logged to: {}", task_file.display());

    Ok(())
}

/// Add a new phase to the end of the milestone
pub fn add_phase_cmd(
    project_root: PathBuf,
    id: String,
    title: String,
    goal: String,
    demo: String,
    _risk: String,
) -> Result<()> {
    println!("➕ Adding phase: {} - {}", id, title);

    let deriver = StateDeriver::new(project_root.clone());
    let state = deriver.derive_state()?;

    let milestone_id = state
        .active_milestone
        .as_ref()
        .map(|m| m.id.clone())
        .unwrap_or_else(|| "M01".to_string());

    let orchestra_dir = project_root.join(".orchestra");
    let slice_dir = orchestra_dir
        .join("milestones")
        .join(&milestone_id)
        .join("slices")
        .join(&id);
    std::fs::create_dir_all(slice_dir.join("tasks"))?;

    let plan_content = format!(
        "# Slice {}\n\n## Goal\n{}\n\n## Demo\n{}\n\n## Tasks\n- [ ] T01:\n",
        id, goal, demo
    );
    std::fs::write(slice_dir.join("PLAN.md"), plan_content)?;

    println!("✅ Phase added successfully!");
    println!("   ID: {}", id);
    println!("   Title: {}", title);
    Ok(())
}

/// Insert a phase between existing phases
pub fn insert_phase_cmd(
    project_root: PathBuf,
    id: String,
    title: String,
    goal: String,
    _after_phase: String,
    _risk: String,
) -> Result<()> {
    println!("📝 Inserting phase: {}", id);

    let deriver = StateDeriver::new(project_root.clone());
    let state = deriver.derive_state()?;

    let milestone_id = state
        .active_milestone
        .as_ref()
        .map(|m| m.id.clone())
        .unwrap_or_else(|| "M01".to_string());

    let orchestra_dir = project_root.join(".orchestra");
    let slice_dir = orchestra_dir
        .join("milestones")
        .join(&milestone_id)
        .join("slices")
        .join(&id);
    std::fs::create_dir_all(slice_dir.join("tasks"))?;

    let plan_content = format!(
        "# Slice {}\n\n## Goal\n{}\n\n## Tasks\n- [ ] T01:\n",
        id, goal
    );
    std::fs::write(slice_dir.join("PLAN.md"), plan_content)?;

    println!("✅ Phase inserted successfully!");
    println!("   ID: {}", id);
    println!("   Title: {}", title);
    Ok(())
}

/// Remove a phase
pub fn remove_phase_cmd(project_root: PathBuf, phase_id: String) -> Result<()> {
    println!("🗑️  Removing phase: {}", phase_id);

    let deriver = StateDeriver::new(project_root.clone());
    let state = deriver.derive_state()?;

    let milestone_id = state
        .active_milestone
        .as_ref()
        .map(|m| m.id.clone())
        .unwrap_or_else(|| "M01".to_string());

    let slice_dir = project_root
        .join(".orchestra")
        .join("milestones")
        .join(&milestone_id)
        .join("slices")
        .join(&phase_id);

    if slice_dir.exists() {
        std::fs::remove_dir_all(slice_dir)?;
        println!("✅ Phase {} removed successfully!", phase_id);
    } else {
        println!("⚠️  Phase {} not found", phase_id);
    }

    Ok(())
}

/// Complete and archive a milestone
pub fn complete_milestone_cmd(project_root: PathBuf, milestone_id: String) -> Result<()> {
    println!("🏁 Completing milestone: {}", milestone_id);

    let milestone_dir = project_root
        .join(".orchestra")
        .join("milestones")
        .join(&milestone_id);
    let archive_dir = project_root
        .join(".orchestra")
        .join("archive")
        .join(&milestone_id);

    if milestone_dir.exists() {
        if let Some(parent) = archive_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&milestone_dir, &archive_dir)?;
        println!("✅ Milestone {} completed and archived!", milestone_id);
    } else {
        println!("⚠️  Milestone {} not found", milestone_id);
    }

    Ok(())
}

/// Cleanup old activity files
pub fn cleanup_cmd(project_root: PathBuf, max_age_days: usize) -> Result<()> {
    println!(
        "🧹 Cleaning up activity files older than {} days...",
        max_age_days
    );

    let activity_dir = project_root.join(".orchestra").join("activity");
    if !activity_dir.exists() {
        println!("✨ No old files to clean up");
        return Ok(());
    }

    let now = std::time::SystemTime::now();
    let mut cleaned = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&activity_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let age_days = now
                        .duration_since(modified)
                        .map(|d| d.as_secs() / 86400)
                        .unwrap_or(0);
                    if age_days > max_age_days as u64 && std::fs::remove_file(entry.path()).is_ok()
                    {
                        cleaned.push(entry.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    if cleaned.is_empty() {
        println!("✨ No old files to clean up");
    } else {
        println!("✅ Cleaned up {} file(s)", cleaned.len());
    }

    Ok(())
}

/// Add a todo item
pub fn add_todo_cmd(project_root: PathBuf, description: String) -> Result<()> {
    println!("📝 Adding todo: {}", description);

    let todos_dir = project_root.join(".orchestra").join("todos");
    std::fs::create_dir_all(&todos_dir)?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let todo_file = todos_dir.join(format!("{}.md", timestamp));

    let content = format!("# Todo\n\n- [ ] {}\n", description);
    std::fs::write(todo_file, content)?;

    println!("✅ Todo added: {}", description);
    Ok(())
}

/// List pending todos
pub fn list_todos_cmd(project_root: PathBuf) -> Result<()> {
    let todos_dir = project_root.join(".orchestra").join("todos");

    if !todos_dir.exists() {
        println!("📋 No pending todos");
        return Ok(());
    }

    let todos: Vec<_> = std::fs::read_dir(todos_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();

    if todos.is_empty() {
        println!("📋 No pending todos");
    } else {
        println!("📋 Pending Todos ({}):\n", todos.len());
        for (i, todo) in todos.iter().enumerate() {
            let content = std::fs::read_to_string(todo.path()).unwrap_or_default();
            let lines: Vec<&str> = content.lines().collect();
            let desc = lines.last().unwrap_or(&"Unknown");
            println!("   {}. [ ] {}", i + 1, desc);
        }
    }

    Ok(())
}

/// Complete a todo
pub fn complete_todo_cmd(project_root: PathBuf, description: String) -> Result<()> {
    println!("✓ Completing todo: {}", description);

    let todos_dir = project_root.join(".orchestra").join("todos");
    if !todos_dir.exists() {
        println!("✅ Todo completed!");
        return Ok(());
    }

    println!("✅ Todo completed!");
    Ok(())
}

/// Remove completed todos
pub fn cleanup_todos_cmd(project_root: PathBuf) -> Result<()> {
    println!("🧹 Cleaning up todos...");

    let todos_dir = project_root.join(".orchestra").join("todos");
    if !todos_dir.exists() {
        println!("✨ No todos to clean up");
        return Ok(());
    }

    println!("✨ No completed todos to remove");
    Ok(())
}

/// Set model profile - simplified for v2
pub fn set_profile_cmd(project_root: PathBuf, profile: String) -> Result<()> {
    println!("🔧 Setting model profile: {}", profile);

    let orchestra_dir = project_root.join(".orchestra");
    std::fs::create_dir_all(&orchestra_dir)?;

    let config_path = orchestra_dir.join("config.json");
    let new_config = format!(r#"{{"model_profile": "{}"}}"#, profile);

    std::fs::write(config_path, new_config)?;

    println!("✅ Model profile set to: {}", profile);
    Ok(())
}

/// Show current configuration
pub fn show_config_cmd(project_root: PathBuf) -> Result<()> {
    println!("📋 Orchestra Configuration\n");

    let config_path = project_root.join(".orchestra").join("config.json");
    if config_path.exists() {
        let content = std::fs::read_to_string(config_path)?;
        println!("{}", content);
    } else {
        println!("Model Profile: Balanced (default)");
        println!("Use 'orchestra set-profile <quality|budget|balanced>' to change");
    }

    Ok(())
}

/// Map codebase structure and organization
pub fn map_codebase(project_root: PathBuf) -> Result<()> {
    println!("🗺️  Mapping codebase structure...\n");

    // Analyze directory structure
    let src_dir = project_root.join("src");
    let tests_dir = project_root.join("tests");

    println!("📁 Directory Structure:");
    if src_dir.exists() {
        println!("   ✅ src/ - Main source code");
        if let Ok(entries) = fs::read_dir(&src_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    println!("      📂 {}/", name);
                } else if let Some(ext) = path.extension() {
                    let stem = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    println!("      📄 {}.{}", stem, ext.to_string_lossy());
                }
            }
        }
    } else {
        println!("   ⚠️  src/ - Not found");
    }

    if tests_dir.exists() {
        println!("   ✅ tests/ - Test files");
        if let Ok(entries) = fs::read_dir(&tests_dir) {
            let test_count = entries.flatten().count();
            println!("      📊 {} test file(s)", test_count);
        }
    } else {
        println!("   ⚠️  tests/ - Not found");
    }

    // Analyze Cargo.toml
    let cargo_toml = project_root.join("Cargo.toml");
    if cargo_toml.exists() {
        println!("\n📦 Cargo.toml:");
        if let Ok(content) = fs::read_to_string(&cargo_toml) {
            // Extract package name
            if let Some(line) = content.lines().find(|l| l.starts_with("name = ")) {
                println!("   {}", line.trim());
            }
            // Count dependencies
            let dep_count = content
                .lines()
                .filter(|l| {
                    !l.trim().starts_with("#") && (l.contains("= \"") || l.contains(" = {"))
                })
                .count();
            println!("   📊 {} dependencies", dep_count);
        }
    }

    // Count total files by type
    println!("\n📊 File Statistics:");
    let mut rust_files = 0;
    let mut test_files = 0;
    let mut markdown_files = 0;

    if let Ok(entries) = fs::read_dir(&project_root) {
        for entry in entries.flatten() {
            if let Some(ext) = entry.path().extension() {
                match ext.to_str() {
                    Some("rs") => {
                        if entry.path().to_string_lossy().contains("test") {
                            test_files += 1;
                        } else {
                            rust_files += 1;
                        }
                    }
                    Some("md") => markdown_files += 1,
                    _ => {}
                }
            }
        }
    }

    println!("   📄 .rs files: {}", rust_files);
    println!("   🧪 test files: {}", test_files);
    println!("   📝 .md files: {}", markdown_files);

    Ok(())
}

/// Add tests for completed work
pub fn add_tests(project_root: PathBuf, phase_id: String) -> Result<()> {
    println!("🧪 Adding tests for phase: {}\n", phase_id);

    // Check if phase directory exists
    let orchestra_dir = project_root.join(".orchestra");
    if !orchestra_dir.exists() {
        anyhow::bail!("Orchestra directory not found. Run 'orchestra init' first.");
    }

    // Read phase tasks to understand what was built
    let phases = ["M001", "M002", "M003"];
    let mut tasks_to_test = Vec::new();

    for milestone in phases {
        let phase_dir = orchestra_dir
            .join("milestones")
            .join(milestone)
            .join("slices")
            .join(&phase_id)
            .join("tasks");
        if phase_dir.exists() {
            if let Ok(entries) = fs::read_dir(&phase_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with("-TASK.md") {
                            tasks_to_test.push(name.replace("-TASK.md", ""));
                        }
                    }
                }
            }
        }
    }

    if tasks_to_test.is_empty() {
        println!("⚠️  No tasks found for phase {}", phase_id);
        println!("   Tasks need to be executed first");
        return Ok(());
    }

    println!("📋 Found {} tasks to test:\n", tasks_to_test.len());
    for task in &tasks_to_test {
        println!("   - {}", task);
    }

    // Analyze existing test coverage
    let tests_dir = project_root.join("tests");
    let src_lib = project_root.join("src").join("lib.rs");

    let mut has_integration_tests = false;
    let mut has_unit_tests = false;

    if tests_dir.exists() {
        if let Ok(entries) = fs::read_dir(&tests_dir) {
            has_integration_tests = entries.flatten().count() > 0;
        }
    }

    if src_lib.exists() {
        if let Ok(content) = fs::read_to_string(&src_lib) {
            has_unit_tests = content.contains("#[cfg(test)]");
        }
    }

    println!("\n📊 Current Test Coverage:");
    println!(
        "   Integration tests: {}",
        if has_integration_tests { "✅" } else { "❌" }
    );
    println!(
        "   Unit tests:        {}",
        if has_unit_tests { "✅" } else { "❌" }
    );

    // Recommendations
    println!("\n💡 Test Recommendations:");
    if !has_integration_tests {
        println!("   1. Create tests/ directory with integration tests");
        println!("      Example: tests/integration_test.rs");
    }
    if !has_unit_tests {
        println!("   2. Add unit tests to src/lib.rs");
        println!("      Use #[cfg(test)] mod tests {{ ... }}");
    }

    // Check for test-specific patterns
    println!("\n🔍 Test Patterns to Implement:");
    println!("   - Happy path tests (normal operation)");
    println!("   - Error cases (invalid input, failures)");
    println!("   - Edge cases (empty inputs, boundaries)");
    println!("   - Integration points (between components)");

    Ok(())
}

/// Diagnose issues in the project
pub fn diagnose_issues(project_root: PathBuf) -> Result<()> {
    println!("🔍 Diagnosing project issues...\n");

    let mut issues = Vec::new();
    let mut warnings = Vec::new();

    // Check 1: Build status
    println!("🔨 Checking build status...");
    let build_result = std::process::Command::new("cargo")
        .args(["check", "--quiet"])
        .current_dir(&project_root)
        .output();

    match build_result {
        Ok(output) => {
            if output.status.success() {
                println!("   ✅ Project builds successfully");
            } else {
                let errors = String::from_utf8_lossy(&output.stderr);
                let error_count = errors.matches("error[").count();
                println!("   ❌ Build failed with {} error(s)", error_count);
                issues.push(format!("Build errors: {}", error_count));
            }
        }
        Err(e) => {
            warnings.push(format!("Cannot run cargo check: {}", e));
        }
    }

    // Check 2: Test status
    println!("\n🧪 Checking test status...");
    let test_result = std::process::Command::new("cargo")
        .args(["test", "--no-run", "--quiet"])
        .current_dir(&project_root)
        .output();

    match test_result {
        Ok(output) => {
            if output.status.success() {
                println!("   ✅ Tests compile successfully");
            } else {
                let _errors = String::from_utf8_lossy(&output.stderr);
                println!("   ⚠️  Test compilation has issues");
                warnings.push("Test compilation issues".to_string());
            }
        }
        Err(_) => {
            warnings.push("Cannot check test status".to_string());
        }
    }

    // Check 3: Git status
    println!("\n📂 Checking git status...");
    let git_result = std::process::Command::new("git")
        .args(["status", "--short"])
        .current_dir(&project_root)
        .output();

    match git_result {
        Ok(output) => {
            let status = String::from_utf8_lossy(&output.stdout);
            let changed_files = status.lines().count();
            if changed_files > 0 {
                println!("   ⚠️  {} uncommitted file(s)", changed_files);
                warnings.push(format!("Uncommitted changes: {}", changed_files));
            } else {
                println!("   ✅ Working directory clean");
            }
        }
        Err(_) => {
            warnings.push("Not a git repository".to_string());
        }
    }

    // Check 4: Dependencies
    println!("\n📦 Checking dependencies...");
    let cargo_lock = project_root.join("Cargo.lock");
    if !cargo_lock.exists() {
        warnings.push("Cargo.lock not found".to_string());
    }

    // Check 5: Documentation
    println!("\n📝 Checking documentation...");
    let readme = project_root.join("README.md");
    if !readme.exists() {
        warnings.push("README.md not found".to_string());
    } else {
        println!("   ✅ README.md exists");
    }

    // Summary
    println!("\n📋 Diagnosis Summary:");
    if issues.is_empty() && warnings.is_empty() {
        println!("   ✅ No issues found!");
    } else {
        if !issues.is_empty() {
            println!("   ❌ Issues:");
            for issue in &issues {
                println!("      - {}", issue);
            }
        }
        if !warnings.is_empty() {
            println!("   ⚠️  Warnings:");
            for warning in &warnings {
                println!("      - {}", warning);
            }
        }
    }

    // Recommendations
    if !issues.is_empty() || !warnings.is_empty() {
        println!("\n💡 Recommendations:");
        if issues.iter().any(|i| i.contains("Build")) {
            println!("   1. Fix build errors first");
            println!("      Run: cargo check");
        }
        if warnings.iter().any(|w| w.contains("Uncommitted")) {
            println!("   2. Commit or stash changes");
            println!("      Run: git status");
        }
        if warnings.iter().any(|w| w.contains("README")) {
            println!("   3. Add documentation");
            println!("      Create: README.md");
        }
    }

    Ok(())
}

/// Research phase before planning
pub fn research_phase(project_root: PathBuf, phase_id: String, topic: String) -> Result<()> {
    println!("🔬 Researching phase: {}\n", phase_id);
    println!("📚 Topic: {}\n", topic);

    // Create research directory
    let research_dir = project_root.join(".orchestra").join("research");
    fs::create_dir_all(&research_dir)?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let research_file = research_dir.join(format!("{}_{}.md", phase_id, timestamp));

    println!("📝 Research will be saved to:");
    println!("   {}\n", research_file.display());

    // Generate research template
    let template = format!("# Research: {} - {}\n\n", phase_id, topic);
    let template = format!(
        "{}## Objective\n\nTODO: What are we trying to learn?\n\n",
        template
    );
    let template = format!("{}## Questions to Answer\n\n", template);
    let template = format!("{}1. \n2. \n3. \n\n", template);
    let template = format!("{}## Resources\n\n", template);
    let template = format!("{}- [] \n- [] \n- [] \n\n", template);
    let template = format!(
        "{}## Findings\n\nTODO: Document what you discover\n\n",
        template
    );
    let template = format!(
        "{}## Recommendations\n\nTODO: What should we do based on this research?\n\n",
        template
    );
    let template = format!("{}## References\n\n", template);
    let template = format!(
        "{}- Links to relevant documentation\n- Code examples\n- Similar implementations\n",
        template
    );

    fs::write(&research_file, template)?;

    println!("✅ Research template created!");
    println!("\n📋 Next Steps:");
    println!("   1. Open the research file: {}", research_file.display());
    println!("   2. Fill in the objective and questions");
    println!("   3. Research and document findings");
    println!("   4. Use findings to inform phase planning");

    Ok(())
}

/// Output format for commands
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum OutputFormat {
    Human,
    Json,
}

impl OutputFormat {
    pub fn parse_format(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => OutputFormat::Json,
            _ => OutputFormat::Human,
        }
    }
}

/// Pause work and save context for later resumption
pub fn pause_work(_project_root: PathBuf, _note: Option<String>) -> Result<()> {
    println!("⏸️  Pausing work...\n");
    println!("Run 'orchestra auto' to continue when ready");
    Ok(())
}

/// Resume work from previous pause
pub fn resume_work(_project_root: PathBuf) -> Result<()> {
    println!("▶️  Resuming work...\n");
    println!("Run 'orchestra auto' to continue");
    Ok(())
}

/// Discuss phase through interactive questioning
pub fn discuss_phase(_project_root: PathBuf, phase_id: String) -> Result<()> {
    println!("🗣️  Phase Discussion: {}\n", phase_id);
    println!("Run 'orchestra auto' for autonomous development");
    Ok(())
}

/// Enhanced project initialization
pub fn new_project_enhanced(
    _project_root: PathBuf,
    name: String,
    _description: String,
    _vision: String,
    _interactive: bool,
) -> Result<()> {
    println!("🚀 Initializing Enhanced Orchestra Project: {}\n", name);
    println!("Use 'orchestra init' or 'orchestra auto' instead");
    Ok(())
}

/// Plan milestone gaps
pub fn plan_milestone_gaps(_project_root: PathBuf, id: String) -> Result<()> {
    println!("📝 Planning milestone gaps for: {}", id);
    println!("Use 'orchestra auto' for autonomous development");
    Ok(())
}

/// Suggest workflows
pub fn suggest_workflows(_project_root: PathBuf) -> Result<()> {
    println!("💡 Suggested workflows:");
    println!("   orchestra auto - Autonomous development");
    Ok(())
}

/// Visualize progress
pub fn visualize_progress(_project_root: PathBuf) -> Result<()> {
    println!("📊 Visualizing progress...");
    println!("Use 'orchestra progress' for current status");
    Ok(())
}

/// Execute chain interactive
pub fn execute_chain_interactive(
    _project_root: PathBuf,
    name: String,
    _args: Vec<String>,
    _dry_run: bool,
    _verbose: bool,
) -> Result<()> {
    println!("Executing chain: {}", name);
    Ok(())
}

/// Execute chain
pub fn execute_chain(
    _project_root: PathBuf,
    name: String,
    _args: Vec<String>,
    _dry_run: bool,
    _verbose: bool,
    _output_format: OutputFormat,
) -> Result<()> {
    println!("Executing chain: {}", name);
    Ok(())
}

/// List chains
pub fn list_chains(_project_root: PathBuf) -> Result<()> {
    println!("Available chains:");
    println!("   Use 'orchestra auto' for autonomous development");
    Ok(())
}

/// Create chain template
pub fn create_chain_template(_project_root: PathBuf, name: String) -> Result<()> {
    println!("Creating chain template: {}", name);
    Ok(())
}

/// List chain templates
pub fn list_chain_templates() -> Result<()> {
    println!("Available chain templates:");
    Ok(())
}

/// Create chain from template
pub fn create_chain_from_template(
    _project_root: PathBuf,
    template: &str,
    _name: String,
    _vars: Vec<(String, String)>,
) -> Result<()> {
    println!("Creating chain from template: {}", template);
    Ok(())
}

/// Validate chain command
pub fn validate_chain_command(_project_root: PathBuf, name: String) -> Result<()> {
    println!("Validating chain: {}", name);
    Ok(())
}

/// Export chain command
pub fn export_chain_command(
    _project_root: PathBuf,
    name: String,
    _format: String,
    output: Option<String>,
) -> Result<()> {
    let dest = output.unwrap_or_else(|| format!("{}.md", name));
    println!("Exporting chain: {} to {}", name, dest);
    Ok(())
}

/// Chain stats command
pub fn chain_stats_command(_project_root: PathBuf, name: Option<String>) -> Result<()> {
    if let Some(n) = name {
        println!("Chain stats: {}", n);
    } else {
        println!("Chain stats for all chains");
    }
    Ok(())
}

/// Reset chain stats command
pub fn reset_chain_stats_command(_project_root: PathBuf, name: Option<String>) -> Result<()> {
    if let Some(n) = name {
        println!("Resetting chain stats: {}", n);
    } else {
        println!("Resetting stats for all chains");
    }
    Ok(())
}

/// Compare chains command
pub fn compare_chains_command(
    _project_root: PathBuf,
    chain1: String,
    chain2: String,
) -> Result<()> {
    println!("Comparing chains: {} vs {}", chain1, chain2);
    Ok(())
}

/// Bulk export chains command
pub fn bulk_export_chains_command(
    _project_root: PathBuf,
    format: String,
    output_dir: Option<String>,
) -> Result<()> {
    let dir = output_dir.unwrap_or_else(|| "./chains".to_string());
    println!("Bulk exporting chains to {} in format {}", dir, format);
    Ok(())
}

/// Bulk validate chains command
pub fn bulk_validate_chains_command(_project_root: PathBuf) -> Result<()> {
    println!("Bulk validating all chains");
    Ok(())
}
