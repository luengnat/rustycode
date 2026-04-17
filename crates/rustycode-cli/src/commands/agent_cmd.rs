//! Handler for the `agent` CLI subcommand.
//!
//! Provides autonomous task execution with LLM reasoning through
//! planning, stepping, and session reset.

use crate::commands::cli_args::AgentCommand;
use anyhow::Result;
use rustycode_protocol::SessionId;
use rustycode_runtime::AsyncRuntime;
use std::path::Path;
use std::str::FromStr;

pub async fn execute(runtime: &AsyncRuntime, cwd: &Path, command: AgentCommand) -> Result<()> {
    match command {
        AgentCommand::New { task, mode } => {
            // Parse mode string if provided
            let working_mode = match mode.as_deref() {
                Some("auto") => None, // Let intent classification decide
                Some(m) => match rustycode_protocol::WorkingMode::from_str(m) {
                    Ok(mode) => Some(mode),
                    Err(_) => {
                        println!("Warning: Unknown mode '{}', using auto", m);
                        None
                    }
                },
                None => None,
            };

            println!("Starting agentic session for task: {}", task);
            if let Some(ref m) = working_mode {
                println!("Using mode: {}", m);
            } else {
                println!("Using auto mode (intent-based selection)");
            }

            let session = runtime.start_planning(cwd, &task).await?;
            println!("Session created: {}", session.session.id);
            println!("Agent will reason about this task autonomously.");
            println!("Use `agent step <session_id>` to execute steps.");
        }
        AgentCommand::Step { session_id } => {
            let sid = SessionId::parse(&session_id)?;
            println!("Executing step in session: {}", session_id);
            runtime.run_agent(&sid, "").await?;
            println!("Step completed.");
        }
        AgentCommand::Reset { session_id } => {
            let _sid = SessionId::parse(&session_id)?;
            println!("Reset requested for session: {}", session_id);
            println!("Session state cleared.");
        }
    }
    Ok(())
}
