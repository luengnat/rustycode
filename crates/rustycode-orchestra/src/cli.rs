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
            let b = budget.unwrap_or(100.0);

            if budget.is_some() {
                println!("Budget: ${:.2}", b);
            }

            if let Some(info) = OrchestraService::run_auto(project_root, b).await? {
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
        OrchestraCommand::Progress => {
            println!("Orchestra Progress is not yet implemented.");
            println!("This command will show project progress across milestones and slices.");
            Ok(())
        }
        OrchestraCommand::PlanPhase { phase_id } => {
            println!("Orchestra PlanPhase ({}) is not yet implemented.", phase_id);
            println!("This command will generate a multi-wave plan for a specific phase.");
            Ok(())
        }
        OrchestraCommand::ExecutePhase { phase_id, auto } => {
            println!(
                "Orchestra ExecutePhase ({}) is not yet implemented (auto: {}).",
                phase_id, auto
            );
            println!("This command will execute all waves in a planned phase.");
            Ok(())
        }
        OrchestraCommand::Debug { issue } => {
            println!(
                "Orchestra Debug is not yet implemented (issue: {:?}).",
                issue
            );
            println!(
                "This command will start an interactive debug session to resolve specific issues."
            );
            Ok(())
        }
        OrchestraCommand::AddTodo { description } => {
            println!(
                "Orchestra AddTodo is not yet implemented (description: {:?}).",
                description
            );
            Ok(())
        }
        OrchestraCommand::CheckTodos { area } => {
            println!(
                "Orchestra CheckTodos is not yet implemented (area: {:?}).",
                area
            );
            Ok(())
        }
        OrchestraCommand::Help => {
            println!("Orchestra v2 - Get Stuff Done Methodology Framework");
            println!();
            println!("Available commands:");
            println!("  init <name> <description> <vision>  Initialize a new project");
            println!("  auto [--budget <cost>]              Run autonomous mode");
            println!("  quick <task>                        Execute a quick task");
            println!("  progress                            Show project progress");
            println!("  plan-phase <id>                     Create a new phase plan");
            println!("  execute-phase <id> [--auto]         Execute a phase");
            println!("  debug [issue]                       Start/resume debug session");
            println!("  add-todo [desc]                     Add a todo");
            println!("  check-todos [area]                  Check todos");
            Ok(())
        }
    }
}
