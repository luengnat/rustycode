//! Orchestra (Get Stuff Done) framework commands

use super::CommandContext;
use super::CommandEffect;
use anyhow::Result;
use rustycode_orchestra::{BootstrapInfo, OrchestraService, StateDeriver};

/// Parse the `/orchestra quick` task text, falling back to a sensible default.
pub fn parse_orchestra_quick_task(parts: &[&str]) -> String {
    if parts.len() > 2 {
        let joined = parts[2..].join(" ");
        if joined.trim().is_empty() {
            "Quick improvement".to_string()
        } else {
            joined
        }
    } else {
        "Quick improvement".to_string()
    }
}

/// Shared help text for Orchestra commands.
pub fn orchestra_help_text() -> String {
    "Orchestra (Get Stuff Done) - Autonomous Mode\n\
     \n\
     Usage:\n\
     • /orchestra init - Seed the canonical project structure\n\
     • /orchestra quick <task> - Run a one-off improvement\n\
     • /orchestra auto - Start autonomous execution\n\
     \n\
     Autonomous mode will:\n\
     • Create default project structure if needed\n\
     • Find and execute pending tasks\n\
     • Use LLM for code generation\n\
     • Update state after each task\n\
     • Stop when no more tasks or Ctrl+C pressed\n\
     "
    .to_string()
}

/// Format the success message shown after a Orchestra bootstrap.
pub fn format_orchestra_bootstrap_success(info: &BootstrapInfo) -> String {
    format!(
        "✅ Orchestra project initialized\n   🧩 Task: {}\n   🎯 Goal: {}\n   📄 Plan: {}",
        info.task_title,
        info.task_goal,
        info.task_plan_path.display()
    )
}

/// Format a Orchestra state snapshot for display.
pub fn format_orchestra_state_message(cwd: &std::path::Path) -> Result<String> {
    let state = StateDeriver::new(cwd.to_path_buf()).derive_state()?;
    let milestone_str = state
        .active_milestone
        .as_ref()
        .map(|m| format!("{}: {}", m.id, m.title))
        .unwrap_or_else(|| "—".to_string());
    let slice_str = state
        .active_slice
        .as_ref()
        .map(|s| format!("{}: {}", s.id, s.title))
        .unwrap_or_else(|| "—".to_string());
    let task_str = state
        .active_task
        .as_ref()
        .map(|t| format!("{}: {}", t.id, t.title))
        .unwrap_or_else(|| "—".to_string());

    Ok(format!(
        "📊 Orchestra Project State\n\n\
         Active Milestone:  {}\n\
         Active Slice:      {}\n\
         Active Task:       {}\n\
         Current Phase:     {:?}\n\n\
         Total Milestones:  {}",
        milestone_str,
        slice_str,
        task_str,
        state.phase,
        state.milestones.len()
    ))
}

/// Shared status text for existing Orchestra projects.
pub fn orchestra_project_already_exists_text() -> &'static str {
    "Orchestra project already exists. Use /orchestra auto to continue."
}

/// Shared async start message for Orchestra init.
pub fn orchestra_init_async_message() -> &'static str {
    "🛠️  Seeding the canonical Orchestra project structure..."
}

/// Shared async start message for Orchestra quick.
pub fn orchestra_quick_async_message(task: &str) -> String {
    format!("⚡ Running quick Orchestra task: {}", task)
}

/// Shared completion text for Orchestra quick tasks.
pub fn orchestra_quick_complete_text() -> &'static str {
    "✅ Quick task complete"
}

/// Shared async start message for Orchestra auto.
pub fn orchestra_auto_async_message() -> &'static str {
    "🤖 Starting Orchestra autonomous mode..."
}

/// Shared completion text for Orchestra auto mode.
pub fn orchestra_auto_complete_text() -> &'static str {
    "✅ Orchestra autonomous execution complete"
}

/// Append the current Orchestra state snapshot to a completion message when possible.
pub fn format_orchestra_completion_message(base: &str, cwd: &std::path::Path) -> String {
    match format_orchestra_state_message(cwd) {
        Ok(state) => format!("{}\n\n{}", base, state),
        Err(_) => base.to_string(),
    }
}

/// Format Orchestra state for display in status widget
pub fn format_orchestra_state_display(
    milestone: Option<(&str, &str)>,
    slice: Option<(&str, &str)>,
    task: Option<(&str, &str)>,
    phase: &str,
) -> String {
    let milestone_str = milestone
        .map(|(id, title)| format!("{}: {}", id, title))
        .unwrap_or_else(|| "—".to_string());

    let slice_str = slice
        .map(|(id, title)| format!("{}: {}", id, title))
        .unwrap_or_else(|| "—".to_string());

    let task_str = task
        .map(|(id, title)| format!("{}: {}", id, title))
        .unwrap_or_else(|| "—".to_string());

    format!(
        "📊 Orchestra Project Status\n\n\
         Active Milestone:  {}\n\
         Active Slice:      {}\n\
         Active Task:       {}\n\
         Current Phase:     {}",
        milestone_str, slice_str, task_str, phase
    )
}

/// Format a log entry with proper emoji/icon
pub fn format_orchestra_log_entry(message: &str) -> String {
    if message.contains("✅") || message.contains("✓") {
        format!("✅ {}", message.trim_start_matches(['✅', '✓', ' ']))
    } else if message.contains("❌") || message.contains("✗") {
        format!("❌ {}", message.trim_start_matches(['❌', '✗', ' ']))
    } else if ["🤖", "⚙", "🚀", "⏹"].iter().any(|e| message.contains(e)) {
        message.to_string()
    } else {
        format!("  {}", message)
    }
}

/// Handle /orchestra commands for Orchestra framework integration
pub fn handle_orchestra_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    if parts.len() < 2 {
        return Ok(CommandEffect::SystemMessage(orchestra_help_text()));
    }

    let subcmd = parts[1];
    match subcmd {
        "state" => {
            let cwd = ctx.cwd.to_path_buf();
            let command_tx = ctx.command_tx.clone();
            std::thread::spawn(move || {
                let result = format_orchestra_state_message(&cwd);
                match result {
                    Ok(output) => {
                        let _ = command_tx
                            .send(crate::app::async_::SlashCommandResult::Success(output));
                    }
                    Err(e) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                            format!("Failed to get Orchestra state: {}", e),
                        ));
                    }
                }
            });
            Ok(CommandEffect::AsyncStarted(
                "📖 Fetching Orchestra project state...".to_string(),
            ))
        }
        "init" => {
            let cwd = ctx.cwd.to_path_buf();
            let command_tx = ctx.command_tx.clone();
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("Failed to create runtime for Orchestra init: {}", e);
                        return;
                    }
                };
                let result = rt.block_on(OrchestraService::bootstrap_default_if_needed(&cwd));
                match result {
                    Ok(Some(info)) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Success(
                            format_orchestra_completion_message(
                                &format_orchestra_bootstrap_success(&info),
                                &cwd,
                            ),
                        ));
                    }
                    Ok(None) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Success(
                            format_orchestra_completion_message(
                                orchestra_project_already_exists_text(),
                                &cwd,
                            ),
                        ));
                    }
                    Err(e) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                            format!("Orchestra init failed: {}", e),
                        ));
                    }
                }
            });
            Ok(CommandEffect::AsyncStarted(
                orchestra_init_async_message().to_string(),
            ))
        }
        "quick" => {
            let task = parse_orchestra_quick_task(parts);
            let task_for_spawn = task.clone();
            let cwd = ctx.cwd.to_path_buf();
            let cwd_for_completion = cwd.clone();
            let command_tx = ctx.command_tx.clone();
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("Failed to create runtime for Orchestra quick: {}", e);
                        return;
                    }
                };
                let result =
                    rt.block_on(OrchestraService::run_quick_task(cwd, task_for_spawn, 10.0));
                match result {
                    Ok(Some(info)) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Success(
                            format_orchestra_bootstrap_success(&info),
                        ));
                    }
                    Ok(None) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Success(
                            format_orchestra_completion_message(
                                orchestra_quick_complete_text(),
                                &cwd_for_completion,
                            ),
                        ));
                    }
                    Err(e) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                            format!("Orchestra quick failed: {}", e),
                        ));
                    }
                }
            });
            Ok(CommandEffect::AsyncStarted(orchestra_quick_async_message(
                &task,
            )))
        }
        "auto" => {
            let cwd = ctx.cwd.to_path_buf();
            let cwd_for_completion = cwd.clone();
            let command_tx = ctx.command_tx.clone();
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("Failed to create runtime for Orchestra auto: {}", e);
                        return;
                    }
                };
                let result = rt.block_on(OrchestraService::run_auto(cwd, 100.0));
                match result {
                    Ok(Some(info)) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Success(
                            format_orchestra_bootstrap_success(&info),
                        ));
                    }
                    Ok(None) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Success(
                            format_orchestra_completion_message(
                                orchestra_auto_complete_text(),
                                &cwd_for_completion,
                            ),
                        ));
                    }
                    Err(e) => {
                        let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                            format!("Orchestra auto failed: {}", e),
                        ));
                    }
                }
            });
            Ok(CommandEffect::AsyncStarted(
                orchestra_auto_async_message().to_string(),
            ))
        }
        _ => Ok(CommandEffect::SystemMessage(format!(
            "Unknown Orchestra command: {}\nUse /orchestra for help.",
            subcmd
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_orchestra::BootstrapInfo;
    use tempfile::TempDir;

    #[test]
    fn parse_orchestra_quick_task_defaults_when_missing() {
        assert_eq!(
            parse_orchestra_quick_task(&["/orchestra", "quick"]),
            "Quick improvement"
        );
    }

    #[test]
    fn parse_orchestra_quick_task_defaults_when_blank() {
        assert_eq!(
            parse_orchestra_quick_task(&["/orchestra", "quick", "   ", ""]),
            "Quick improvement"
        );
    }

    #[test]
    fn parse_orchestra_quick_task_preserves_task_text() {
        assert_eq!(
            parse_orchestra_quick_task(&["/orchestra", "quick", "Fix", "the", "login", "flow"]),
            "Fix the login flow"
        );
    }

    #[test]
    fn orchestra_help_text_mentions_core_commands() {
        let help = orchestra_help_text();
        assert!(help.contains("/orchestra init"));
        assert!(help.contains("/orchestra quick"));
        assert!(help.contains("/orchestra auto"));
    }

    #[test]
    fn format_orchestra_bootstrap_success_shows_seed_details() {
        let info = BootstrapInfo {
            project_root: std::path::PathBuf::from("/tmp/project"),
            project_name: "project".to_string(),
            description: "desc".to_string(),
            vision: "vision".to_string(),
            task_title: "Fix login".to_string(),
            task_goal: "Initial quick improvement".to_string(),
            milestone_id: "M01".to_string(),
            slice_id: "S01".to_string(),
            task_id: "T01".to_string(),
            roadmap_path: std::path::PathBuf::from(
                "/tmp/project/.orchestra/milestones/M01/ROADMAP.md",
            ),
            slice_plan_path: std::path::PathBuf::from(
                "/tmp/project/.orchestra/milestones/M01/slices/S01/PLAN.md",
            ),
            task_plan_path: std::path::PathBuf::from(
                "/tmp/project/.orchestra/milestones/M01/slices/S01/tasks/T01/T01-PLAN.md",
            ),
            state_path: std::path::PathBuf::from("/tmp/project/.orchestra/STATE.md"),
        };

        let message = format_orchestra_bootstrap_success(&info);
        assert!(message.contains("Fix login"));
        assert!(message.contains("Initial quick improvement"));
        assert!(message.contains("T01-PLAN.md"));
    }

    #[test]
    fn format_orchestra_state_message_shows_state_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let message = format_orchestra_state_message(temp_dir.path()).unwrap();

        assert!(message.contains("Orchestra Project State"));
        assert!(message.contains("Total Milestones"));
    }

    #[test]
    fn format_orchestra_completion_message_appends_state_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let message = format_orchestra_completion_message("Done", temp_dir.path());

        assert!(message.contains("Done"));
        assert!(message.contains("Orchestra Project State"));
    }
}
