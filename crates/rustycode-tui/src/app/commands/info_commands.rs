//! Info and utility commands: help, marketplace, skill, mcp, hook, theme

use super::CommandContext;
use super::CommandEffect;
use anyhow::Result;

/// Handle /help command
pub fn handle_help_command(_parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    Ok(CommandEffect::ShowHelp)
}

/// Handle /marketplace command
pub fn handle_marketplace_command(
    parts: &[&str],
    ctx: CommandContext<'_>,
) -> Result<CommandEffect> {
    let input = parts.join(" ");
    let input_clone = input.clone();

    let tx = ctx.command_tx;
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create runtime for marketplace: {}", e);
                return;
            }
        };
        let result = rt
            .block_on(crate::slash_commands::marketplace::handle_marketplace_command(&input_clone));

        match result {
            Ok(Some(output)) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
            }
            Ok(None) => {
                // Command succeeded but produced no output
            }
            Err(e) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Error(format!(
                    "Marketplace command failed: {}",
                    e
                )));
            }
        }
    });

    Ok(CommandEffect::None)
}

/// Handle /skill and /skills commands
pub fn handle_skill_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
    let input_for_skill = if args.is_empty() {
        "/skill".to_string()
    } else {
        format!("/skill {}", args.join(" "))
    };

    let result =
        crate::slash_commands::skill::handle_skill_command(&input_for_skill, ctx.skill_manager);

    match result {
        Ok(Some(output)) => Ok(CommandEffect::SystemMessage(output)),
        Ok(None) => Ok(CommandEffect::None),
        Err(e) => Ok(CommandEffect::SystemMessage(format!(
            "❌ Skill error: {}",
            e
        ))),
    }
}

/// Handle /mcp command
pub fn handle_mcp_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    // Check if this is a subcommand or just opening MCP mode
    if parts.len() < 2 || parts[1] == "open" {
        return Ok(CommandEffect::SystemMessage(
            "MCP Mode: Press Esc to close. Type 'list' for servers, 'status' for connection info."
                .to_string(),
        ));
    }

    let input = parts.join(" ");
    let input_clone = input.clone();
    let tx = ctx.command_tx;

    // Spawn thread with its own runtime for MCP commands
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create runtime for MCP: {}", e);
                return;
            }
        };
        let result = rt.block_on(crate::slash_commands::mcp::handle_mcp_command(&input_clone));
        match result {
            Ok(Some(output)) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Success(output));
            }
            Ok(None) => {}
            Err(e) => {
                let _ = tx.send(crate::app::async_::SlashCommandResult::Error(format!(
                    "MCP error: {}",
                    e
                )));
            }
        }
    });

    Ok(CommandEffect::AsyncStarted(
        "🔌 Loading MCP servers...".to_string(),
    ))
}

/// Handle /hook command
pub fn handle_hook_command(parts: &[&str], _ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let input = parts.join(" ");
    let result = crate::slash_commands::hook::handle_hook_command(&input);

    match result {
        Ok(Some(output)) => Ok(CommandEffect::SystemMessage(output)),
        Ok(None) => Ok(CommandEffect::None),
        Err(e) => Ok(CommandEffect::SystemMessage(format!(
            "❌ Hook error: {}",
            e
        ))),
    }
}

/// Handle /theme command
pub fn handle_theme_command(parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let args: Vec<&str> = parts[1..].to_vec();
    let result = crate::slash_commands::handle_theme_command(&args, ctx.theme_colors);

    let effect = match result {
        crate::slash_commands::ThemeCommandResult::Success(msg) => {
            CommandEffect::SystemMessage(format!("✓ {}", msg))
        }
        crate::slash_commands::ThemeCommandResult::List(msg) => CommandEffect::SystemMessage(msg),
        crate::slash_commands::ThemeCommandResult::Error(err) => {
            CommandEffect::SystemMessage(format!("❌ {}", err))
        }
    };

    Ok(effect)
}

/// Handle /stats command — display session statistics
pub fn handle_stats_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    use crate::ui::message::MessageRole;

    let turn_count = ctx
        .messages
        .iter()
        .filter(|m| matches!(m.role, MessageRole::User))
        .count();

    // Count tools from messages
    let mut tool_count = 0usize;
    let mut tool_failures = 0usize;
    for msg in ctx.messages.iter() {
        if let Some(tools) = &msg.tool_executions {
            for tool in tools {
                tool_count += 1;
                if matches!(tool.status, crate::ui::message::ToolStatus::Failed) {
                    tool_failures += 1;
                }
            }
        }
    }

    let total_tokens = ctx.session_input_tokens + ctx.session_output_tokens;
    let context_pct = if ctx.context_monitor.max_tokens > 0 {
        ((total_tokens as f64 / ctx.context_monitor.max_tokens as f64) * 100.0).round() as usize
    } else {
        0
    };

    let stats = crate::slash_commands::stats::SessionStats {
        input_tokens: ctx.session_input_tokens,
        output_tokens: ctx.session_output_tokens,
        cost_usd: ctx.session_cost_usd,
        turn_count,
        tool_count,
        tool_failures,
        context_percentage: context_pct,
        context_tokens: total_tokens,
        context_limit: ctx.context_monitor.max_tokens,
        model: ctx.current_model.clone(),
        duration_secs: ctx.session_start.elapsed().as_secs(),
    };

    let result = crate::slash_commands::stats::handle_stats_command(&stats);
    Ok(CommandEffect::SystemMessage(result.display))
}

/// Handle /cost command — display detailed cost breakdown
pub fn handle_cost_command(_parts: &[&str], ctx: CommandContext<'_>) -> Result<CommandEffect> {
    let cost = ctx.session_cost_usd;
    let input = ctx.session_input_tokens;
    let output = ctx.session_output_tokens;

    let mut lines = Vec::new();
    lines.push(format!(
        "Session Cost: {}",
        if cost < 0.01 {
            format!("${:.4}", cost)
        } else {
            format!("${:.2}", cost)
        }
    ));
    lines.push(format!("Tokens: {} in / {} out", input, output));
    lines.push(format!("Model: {}", ctx.current_model));

    if cost > 0.0 && (input + output) > 0 {
        let cost_per_1k = (cost / (input + output) as f64) * 1000.0;
        lines.push(format!("Avg cost per 1K tokens: ${:.4}", cost_per_1k));
    }

    Ok(CommandEffect::SystemMessage(lines.join("\n")))
}

/// Handle /checkpoint command — display checkpoint status
pub fn handle_checkpoint_command(
    _parts: &[&str],
    _ctx: CommandContext<'_>,
) -> Result<CommandEffect> {
    // Checkpoint integration is available via the execution middleware.
    // For now, show guidance on checkpoint usage.
    let msg = "Checkpoints are auto-created before destructive operations (edit_file, write_file, bash).\n\
               Use /undo to revert the last file change.\n\
               Use /diff to see what changed recently."
        .to_string();
    Ok(CommandEffect::SystemMessage(msg))
}
