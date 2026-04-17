// rustycode-orchestra/src/cli.rs
//! CLI integration for Orchestra v2

use crate::orchestra_service::OrchestraService;

/// CLI commands for Orchestra v2
#[non_exhaustive]
pub enum OrchestraCommand {
    /// Initialize a new project
    Init {
        name: String,
        description: String,
        vision: String,
    },

    /// Run autonomous mode
    Auto { budget: Option<f64> },

    /// Show project progress
    Progress,

    /// Create a new phase plan
    PlanPhase { phase_id: String },

    /// Execute a phase
    ExecutePhase { phase_id: String, auto: bool },

    /// Execute a quick task
    Quick { task: String, auto: bool },

    /// Start or resume a debug session
    Debug { issue: Option<String> },

    /// Add a todo
    AddTodo { description: Option<String> },

    /// Check todos
    CheckTodos { area: Option<String> },

    /// Show help
    Help,
}

/// Execute a Orchestra command
pub async fn execute_command(cmd: OrchestraCommand) -> anyhow::Result<()> {
    match cmd {
        OrchestraCommand::Init {
            name,
            description,
            vision,
        } => {
            let project_root = std::env::current_dir()?;
            let info =
                OrchestraService::init_project(&project_root, &name, &description, &vision).await?;
            println!("Project initialized successfully");
            println!("Created .orchestra/ directory");
            println!("Created milestone, slice, and first task");
            println!("Task: {}", info.task_title);
            println!("Goal: {}", info.task_goal);
            println!("Plan: {}", info.task_plan_path.display());
            Ok(())
        }
        OrchestraCommand::Auto { budget } => {
            let project_root = std::env::current_dir()?;

            if let Some(b) = budget {
                println!("Budget: ${:.2}", b);
            }

            if let Some(info) =
                OrchestraService::run_auto(project_root, budget.unwrap_or(100.0)).await?
            {
                println!("Bootstrapped project: {}", info.project_name);
                println!("Task: {}", info.task_title);
                println!("Goal: {}", info.task_goal);
                println!("Plan: {}", info.task_plan_path.display());
            }
            Ok(())
        }
        OrchestraCommand::Quick { task, auto: _ } => {
            let project_root = std::env::current_dir()?;
            if let Some(info) = OrchestraService::run_quick_task(project_root, task, 10.0).await? {
                println!("Bootstrapped project: {}", info.project_name);
                println!("Task: {}", info.task_title);
                println!("Goal: {}", info.task_goal);
                println!("Plan: {}", info.task_plan_path.display());
            }
            Ok(())
        }
        _ => {
            println!("Command not yet implemented");
            Ok(())
        }
    }
}
