//! Handler for the `omo` CLI subcommand.
//!
//! Delegates to `rustycode_runtime::multi_agent::MultiAgentOrchestrator` for
//! multi-agent code analysis.

use crate::commands::cli_args::OmoCommand;
use anyhow::Result;
use rustycode_runtime::multi_agent::{AgentRole, MultiAgentConfig, MultiAgentOrchestrator};
use std::io::Read;

fn format_role_short_name(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::FactualReviewer => "factual",
        AgentRole::SeniorEngineer => "senior",
        AgentRole::SecurityExpert => "security",
        AgentRole::ConsistencyReviewer => "consistency",
        AgentRole::RedundancyChecker => "redundancy",
        AgentRole::PerformanceAnalyst => "performance",
        AgentRole::TestCoverageAnalyst => "test",
        AgentRole::DocumentationReviewer => "docs",
        #[allow(unreachable_patterns)]
        _ => "other",
    }
}

pub async fn execute(command: OmoCommand) -> Result<()> {
    match command {
        OmoCommand::Analyze {
            file,
            roles,
            parallelism,
            context,
            instructions,
        } => {
            // Read content from file or stdin
            let content = if let Some(ref file_path) = file {
                std::fs::read_to_string(file_path)
                    .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", file_path, e))?
            } else {
                println!("Reading code from stdin (press Ctrl+D when done)...");
                let mut input = String::new();
                std::io::stdin().read_to_string(&mut input)?;
                input
            };

            // Parse roles if specified
            let agent_roles = if let Some(role_names) = roles {
                let mut parsed_roles = Vec::new();
                for role_name in role_names {
                    let role = match role_name.to_lowercase().as_str() {
                        "factual" | "factual-reviewer" => AgentRole::FactualReviewer,
                        "senior" | "senior-engineer" => AgentRole::SeniorEngineer,
                        "security" | "security-expert" => AgentRole::SecurityExpert,
                        "consistency" | "consistency-reviewer" => AgentRole::ConsistencyReviewer,
                        "redundancy" | "redundancy-checker" => AgentRole::RedundancyChecker,
                        "performance" | "performance-analyst" => AgentRole::PerformanceAnalyst,
                        "test" | "test-coverage" => AgentRole::TestCoverageAnalyst,
                        "docs" | "documentation" => AgentRole::DocumentationReviewer,
                        _ => return Err(anyhow::anyhow!("Unknown agent role: {}", role_name)),
                    };
                    parsed_roles.push(role);
                }
                if parsed_roles.is_empty() {
                    AgentRole::all()
                } else {
                    parsed_roles
                }
            } else {
                AgentRole::all()
            };

            // Build configuration
            let config = MultiAgentConfig {
                roles: agent_roles,
                max_parallelism: parallelism,
                context: context.unwrap_or_default(),
                content,
                file_path: file,
                instructions,
            };

            println!(
                "Starting multi-agent analysis with {} agents...",
                config.roles.len()
            );
            println!(
                "Running up to {} agents in parallel...\n",
                config.max_parallelism
            );

            let orchestrator = MultiAgentOrchestrator::from_config(config)?;
            let analysis = orchestrator.analyze().await?;

            println!("{}", MultiAgentOrchestrator::format_analysis(&analysis));
        }
        OmoCommand::ListRoles => {
            println!("Available Agent Roles:\n");
            for role in AgentRole::all() {
                println!("  • {} ({})", role.name(), format_role_short_name(&role));
            }
            println!("\nUse these role names (or abbreviations) with --roles flag.");
        }
    }
    Ok(())
}
