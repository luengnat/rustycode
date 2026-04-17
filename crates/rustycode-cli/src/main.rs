use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rustycode_cli::prompt::PromptConfig;
use rustycode_protocol::{SessionId, WorkingMode};
use rustycode_runtime::AsyncRuntime;
use std::path::PathBuf;
use std::time::Duration;
use tracing_subscriber::filter::LevelFilter;

mod server;
use commands::cli_args::*;
use commands::harness_cmd;
use commands::history_cmd;
use commands::provider_command::{self as provider_cmd, ProviderCommand};
use commands::skills_cmd;
use rustycode_cli::commands;

#[derive(Debug, Parser)]
#[command(
    name = "rustycode",
    version,
    about = "Rust-native coding agent workspace",
    subcommand_negates_reqs = true,
    args_conflicts_with_subcommands = true
)]
struct Cli {
    /// Task to execute directly (equivalent to `rustycode run <task>`).
    /// Use quotes for multi-word tasks: rustycode "fix the bug"
    #[arg(value_name = "TASK", hide_possible_values = true)]
    task: Option<String>,

    /// Automatically answer yes to all prompts (non-interactive mode)
    #[arg(long, global = true)]
    yes: bool,

    /// Enable or disable colored output
    #[arg(long, global = true, default_value = "auto")]
    color: String,

    /// Output format: human (default) or json
    #[arg(long, global = true, default_value = "human")]
    format: String,

    /// Override the configured LLM model for this invocation
    #[arg(long, global = true)]
    model: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Doctor,
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Context {
        prompt: String,
    },
    Run {
        /// Task prompt to execute
        prompt: String,
        /// Auto mode: execute without TUI, output results and exit
        #[arg(long)]
        auto: bool,
        /// Output format: human (default) or json
        #[arg(long, default_value = "human")]
        format: String,
        /// Working mode (affects prompts and behavior):
        ///   auto       - Automatically detect intent and select mode (default)
        ///   code       - Implementation and feature development
        ///   debug      - Troubleshooting and issue diagnosis
        ///   ask        - Quick questions and information retrieval
        ///   orchestrate - Multi-agent coordination and complex workflows
        ///   plan       - Planning and architecture design
        ///   test       - Test-driven development and testing
        #[arg(long, default_value = "auto", value_name = "MODE")]
        mode: String,
    },
    Tools {
        #[command(subcommand)]
        command: ToolsCommand,
    },
    Sessions {
        #[command(subcommand)]
        command: SessionsCommand,
    },
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
    /// Plan mode: create, list, show, approve, or reject plans.
    Plan {
        #[command(subcommand)]
        command: PlanCommand,
    },
    /// Agent mode: autonomous task execution with LLM reasoning.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Harness mode: long-running agent framework with progress persistence
    Harness {
        #[command(subcommand)]
        command: HarnessCommand,
    },
    /// OMO multi-agent orchestration for comprehensive code analysis.
    Omo {
        #[command(subcommand)]
        command: OmoCommand,
    },
    /// Git worktree management for isolated development.
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommand,
    },
    /// Orchestra (Get Stuff Done) methodology commands.
    Orchestra {
        #[command(subcommand)]
        command: OrchestraCommand,
    },
    /// Provider and model management for LLM selection.
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Conversation history management (list, search, show, export).
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
    /// Skills management (list, run, create, validate).
    Skills {
        #[command(subcommand)]
        command: SkillsCommand,
    },
    /// Team learnings management (view, add, remove project memory).
    Learnings {
        #[command(subcommand)]
        command: LearningsCommand,
    },
    /// Launch the interactive TUI.
    Tui {
        /// Force the configuration wizard to run (even if config exists)
        #[arg(long)]
        reconfigure: bool,
    },
    /// Serve the web UI and API server.
    Serve {
        /// Port to listen on (default: 3000)
        #[arg(short, long, default_value = "3000")]
        port: u16,
        /// Directory to serve static files from
        #[arg(short, long)]
        dir: Option<String>,
    },
    /// Run SWE-bench evaluation (load instances, generate predictions).
    Swebench {
        #[command(flatten)]
        args: commands::cli_args::SweBenchCliArgs,
    },
    /// Benchmark runner for agent evaluation (Terminal Bench compatible).
    Bench {
        #[command(subcommand)]
        command: commands::cli_args::BenchCommand,
    },
}

fn main() -> Result<()> {
    // Build tokio runtime with optimized configuration for CPU-bound workloads
    // Uses number of CPU cores for maximum parallelism in tool execution
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .thread_name_fn(|| {
            static ATOMIC_ID: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            let id = ATOMIC_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            format!("main-runtime-worker-{}", id)
        })
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build tokio runtime: {}", e))?;
    rt.block_on(async_main())
}

async fn async_main() -> Result<()> {
    // Redirect tracing output to log file instead of stderr to avoid screen pollution
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let log_dir = PathBuf::from(home).join(".rustycode");
    let log_path = log_dir.join("debug.log");
    let _ = std::fs::create_dir_all(&log_dir);

    // Try to open log file; fall back to stderr if unavailable (non-fatal)
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path);

    match log_file {
        Ok(file) => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive(LevelFilter::INFO.into()),
                )
                .with_target(false)
                .without_time()
                .with_writer(std::sync::Mutex::new(file))
                .init();
        }
        Err(_) => {
            // Fallback: write to stderr (better than crashing)
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive(LevelFilter::WARN.into()),
                )
                .with_target(false)
                .without_time()
                .init();
        }
    }

    let cli = Cli::parse();

    // Resolve effective command:
    //   - explicit subcommand → use it
    //   - positional TASK arg → treat as Run
    //   - nothing → default to Tui
    // Note: stdin reading removed - piping requires different handling via shell
    let command = if let Some(cmd) = cli.command {
        cmd
    } else if let Some(ref task) = cli.task {
        // Check if task looks like a misspelled subcommand
        if let Some(suggestion) = suggest_similar_subcommand(task) {
            eprintln!(
                "Note: '{}' is not a subcommand. Did you mean '{}'?",
                task, suggestion
            );
            eprintln!("Starting task in agent mode...\n");
        }
        Command::Run {
            prompt: task.clone(),
            auto: false,
            format: cli.format.clone(),
            mode: "auto".to_string(),
        }
    } else {
        Command::Tui { reconfigure: false }
    };

    let cwd = std::env::current_dir()?;

    // Apply model override via env var (read by LLM provider config loader)
    if let Some(ref model) = cli.model {
        std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", model);
    }

    // Configure colored output
    match cli.color.as_str() {
        "always" => colored::control::set_override(true),
        "never" => colored::control::set_override(false),
        "auto" => {
            // Let colored crate detect automatically
            colored::control::unset_override();
        }
        _ => {
            eprintln!(
                "Invalid color option: {}. Use 'always', 'never', or 'auto'.",
                cli.color
            );
            std::process::exit(1);
        }
    }

    // Configure global prompt settings
    if cli.yes {
        PromptConfig::set_global_yes_enabled(true);
    }

    // TUI takes over the terminal — must run on a fresh thread to avoid nesting tokio runtimes
    if let Command::Tui { ref reconfigure } = command {
        let reconfigure = *reconfigure;
        return std::thread::spawn(move || {
            rustycode_tui::run(cwd, reconfigure, false).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to start TUI: {}\nHint: run this command in an interactive terminal (not piped/redirected).",
                    e
                )
            })
        })
            .join()
            .map_err(|_| anyhow::anyhow!("TUI thread panicked"))?;
    }

    // Handle Orchestra commands separately to avoid runtime loading issues
    if let Command::Orchestra {
        command: orchestra_cmd,
    } = command
    {
        let format = cli.format.clone();
        return handle_orchestra_command(cwd, orchestra_cmd, format).await;
    }

    let runtime = AsyncRuntime::load(&cwd).await?;
    match command {
        Command::Tui { .. } => {
            // Already handled above
            unreachable!();
        }
        Command::Orchestra { .. } => {
            // Already handled above via handle_orchestra_command
            unreachable!();
        }
        Command::Serve { port, dir } => crate::server::serve_web(port, dir).await?,

        Command::Swebench { args } => {
            use commands::swebench_command::{run_swebench, SweBenchArgs};
            let swebench_args = SweBenchArgs {
                instances: args.instances,
                output: args.output,
                budget: args.budget,
                parallel: args.parallel,
                instance_ids: args.instance_ids,
                format: args.format,
            };
            run_swebench(swebench_args).await?;
        }

        Command::Doctor => {
            let report = runtime.doctor(&cwd).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Config {
            command: ConfigCommand::Show,
        } => {
            println!("{}", serde_json::to_string_pretty(runtime.config())?);
        }
        Command::Config {
            command: ConfigCommand::Get { key },
        } => {
            let config = runtime.config();
            let value = match key.as_str() {
                "model" => serde_json::json!(config.model).to_string(),
                "provider" => serde_json::json!(config.providers).to_string(),
                "log_level" => serde_json::json!(config.advanced.log_level).to_string(),
                "telemetry_enabled" => {
                    serde_json::json!(config.advanced.telemetry_enabled).to_string()
                }
                "cache_enabled" => serde_json::json!(config.advanced.cache_enabled).to_string(),
                _ => {
                    eprintln!(
                        "Unknown config key: {}. Run 'rustycode config show' to see all keys.",
                        key
                    );
                    std::process::exit(1);
                }
            };
            println!("{}", value);
        }
        Command::Config {
            command: ConfigCommand::Set { key, value },
        } => {
            eprintln!("Note: Configuration is stored in ~/.rustycode/config.toml");
            eprintln!("Edit the file directly to change settings.");
            eprintln!();
            eprintln!(
                "To set '{}' to '{}', add/update this in your config:",
                key, value
            );
            eprintln!("[{}]", key);
            eprintln!("{} = \"{}\"", key, value);
        }
        Command::Context { prompt } => {
            let report = runtime.run(&cwd, &prompt).await?;
            println!("{}", serde_json::to_string_pretty(&report.context_plan)?);
        }
        Command::Run {
            prompt,
            auto,
            format,
            mode,
        } => {
            // Parse working mode — "auto" lets IntentGate classify the prompt
            let working_mode_opt = if mode == "auto" {
                None // Let IntentGate decide based on prompt content
            } else {
                Some(mode.parse::<WorkingMode>().unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }))
            };

            if auto {
                let mode_display = working_mode_opt
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "auto (IntentGate)".to_string());
                println!(
                    "🤖 Auto mode ({}) executing task non-interactively...\n",
                    mode_display
                );
            }

            // Build context first to create session
            let report = runtime.run(&cwd, &prompt).await?;
            let session_id = &report.session.id;

            if auto {
                println!("✓ Session created: {}\n", session_id);
                println!("⏳ Executing agent...\n");
            }

            // Then execute the agent loop
            use rustycode_llm::{create_provider_with_config, load_provider_config_from_env};

            // Load provider configuration
            let (provider_type, model_name, v2_config) =
                load_provider_config_from_env().context("Failed to load LLM provider config")?;

            let provider = create_provider_with_config(&provider_type, &model_name, v2_config)
                .context("Failed to create LLM provider")?;

            if auto {
                use rustycode_core::ultrawork::{
                    build_iteration_prompt, ProgressSnapshot, MAX_ITERATIONS,
                };

                // Capture snapshot before iterations (works with or without git)
                let snapshot = ProgressSnapshot::take(&cwd);

                // Carry forward conversation messages across retries so the agent
                // doesn't re-explore the same files. On the first retry, we pass
                // the prior messages so the agent can continue from where it left off.
                let mut prior_messages: Option<Vec<rustycode_llm::ChatMessage>> = None;

                for iteration in 1..=MAX_ITERATIONS {
                    let task_prompt = build_iteration_prompt(&prompt, iteration);

                    if iteration > 1 {
                        eprintln!("\n🔄 Retry {}/{}...", iteration, MAX_ITERATIONS);
                    }

                    match runtime
                        .run_headless_with_prior_messages(
                            &*provider,
                            &model_name,
                            &task_prompt,
                            &cwd,
                            iteration,
                            prior_messages.take(),
                        )
                        .await
                    {
                        Ok(task_result) => {
                            tracing::debug!(
                                iteration,
                                made_writes = task_result.made_writes,
                                total_tool_calls = task_result.total_tool_calls,
                                final_text_len = task_result.final_text.len(),
                                "Iteration completed"
                            );
                            // Progress detection: require BOTH sufficient work AND filesystem
                            // changes. "Sufficient work" means 8+ tool calls (the agent explored,
                            // edited, built, and verified). This prevents false positives where
                            // the agent clones a repo, reads the README, and declares success —
                            // the filesystem changed but no real work was done.
                            //
                            // We also require at least one write/edit tool call. An agent that only
                            // reads and explores files hasn't made real progress, even if git clone
                            // created new files on disk.
                            let min_tool_calls: usize = 3;
                            let has_sufficient_work =
                                task_result.total_tool_calls >= min_tool_calls;
                            let has_made_writes = task_result.made_writes;
                            let has_fs_progress = snapshot.has_progress(&cwd);
                            let has_real_progress =
                                has_sufficient_work && has_made_writes && has_fs_progress;

                            // If agent made writes but never verified, don't declare success.
                            // This catches the pattern where the agent edits a file and stops
                            // without running tests/builds/imports to confirm the changes work.
                            let unverified_edits =
                                has_made_writes && !task_result.verified_after_last_edit;

                            eprintln!(
                                "  [progress] work={} writes={} fs_progress={} verified={} unverified={}",
                                has_sufficient_work, has_made_writes, has_fs_progress,
                                task_result.verified_after_last_edit, unverified_edits
                            );

                            tracing::debug!(
                                has_sufficient_work,
                                has_progress = snapshot.has_progress(&cwd),
                                has_real_progress,
                                total_tool_calls = task_result.total_tool_calls,
                                "Progress check"
                            );

                            if has_real_progress && !unverified_edits {
                                println!("\n✅ Task completed successfully");
                                if !task_result.final_text.is_empty() {
                                    println!("\n{}", task_result.final_text);
                                }
                                if format == "json" {
                                    println!(
                                        "{}",
                                        serde_json::json!({
                                            "status": "success",
                                            "session_id": session_id.to_string(),
                                            "task": prompt,
                                            "iterations": iteration
                                        })
                                    );
                                }
                                break;
                            } else if iteration < MAX_ITERATIONS {
                                // Save messages for carry-forward to next iteration.
                                // This preserves exploration context so the agent can
                                // continue from where it left off instead of starting fresh.
                                prior_messages = Some(task_result.messages);
                                if unverified_edits {
                                    eprintln!("  (agent edited files but never verified, retrying to force verification...)");
                                } else if !task_result.made_writes {
                                    eprintln!("  (agent made no write/edit calls, retrying with context...)");
                                } else {
                                    eprintln!("  (no file changes yet, retrying with context...)");
                                }
                                continue;
                            } else {
                                // Show the agent's response even if we couldn't detect changes
                                println!(
                                    "\n⚠️  Could not detect file changes, but agent responded:"
                                );
                                if !task_result.final_text.is_empty() {
                                    println!("{}", task_result.final_text);
                                }
                                if format == "json" {
                                    println!(
                                        "{}",
                                        serde_json::json!({
                                            "status": "completed_no_changes_detected",
                                            "session_id": session_id.to_string(),
                                            "task": prompt,
                                            "iterations": MAX_ITERATIONS
                                        })
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            if iteration < MAX_ITERATIONS {
                                eprintln!("⚠️  Iteration {} failed: {}. Retrying...", iteration, e);
                                continue;
                            }
                            // Use return Err() instead of std::process::exit(1)
                            // so that output buffers are flushed and the error
                            // message is visible in logs (tee'd output).
                            let err_msg =
                                format!("Task failed after {} iterations: {}", iteration, e);
                            eprintln!("\n❌ {}", err_msg);
                            if format == "json" {
                                eprintln!(
                                    "{}",
                                    serde_json::json!({
                                        "status": "error",
                                        "error": e.to_string(),
                                        "session_id": session_id.to_string(),
                                        "task": prompt,
                                        "iterations": iteration
                                    })
                                );
                            }
                            return Err(anyhow::anyhow!("{}", err_msg));
                        }
                    }
                }
            } else {
                // Interactive mode: single-shot execution via the unified headless runtime
                match runtime
                    .run_headless(&*provider, &model_name, &prompt, &cwd, 1)
                    .await
                {
                    Ok(task_result) => {
                        if task_result.made_writes {
                            println!("✅ Task completed successfully");
                        }
                        if !task_result.final_text.is_empty() {
                            println!("{}", task_result.final_text);
                        }
                        if format == "json" {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "status": "success",
                                    "session_id": session_id.to_string(),
                                    "task": prompt,
                                    "iterations": 1
                                })
                            );
                        }
                    }
                    Err(e) => {
                        if format == "json" {
                            eprintln!(
                                "{}",
                                serde_json::json!({
                                    "status": "error",
                                    "error": e.to_string(),
                                    "session_id": session_id.to_string(),
                                    "task": prompt,
                                    "iterations": 1
                                })
                            );
                        }
                        return Err(e);
                    }
                }
            }
        }
        Command::Tools {
            command: ToolsCommand::List,
        } => {
            let tools = runtime.tool_list();
            let width = tools.iter().map(|tool| tool.name.len()).max().unwrap_or(0);
            for tool in &tools {
                println!("{:<width$}  {}", tool.name, tool.description, width = width);
            }
        }
        Command::Tools {
            command: ToolsCommand::Call { name, params },
        } => {
            let arguments: serde_json::Value = serde_json::from_str(&params)
                .map_err(|e| anyhow::anyhow!("--params must be valid JSON: {e}"))?;
            let report = runtime.run_tool(&cwd, name, arguments).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Sessions {
            command: SessionsCommand::List { limit },
        } => {
            let sessions = runtime.recent_sessions(limit).await?;
            println!("{}", serde_json::to_string_pretty(&sessions)?);
        }
        Command::Sessions {
            command: SessionsCommand::Show { id },
        } => {
            let session_id = SessionId::parse(&id)?;
            let events = runtime.session_events(&session_id).await?;
            println!("{}", serde_json::to_string_pretty(&events)?);
        }
        Command::Events {
            command:
                EventsCommand::Watch {
                    pattern,
                    limit,
                    timeout_ms,
                    run,
                    tool,
                    plan,
                    approve_session,
                    reject_session,
                    params,
                },
        } => {
            watch_events(
                &runtime,
                &pattern,
                limit,
                timeout_ms,
                run,
                tool,
                plan,
                approve_session,
                reject_session,
                &params,
            )
            .await?;
        }
        Command::Plan { command } => commands::plan_cmd::execute(&runtime, &cwd, command).await?,
        Command::Agent { command } => commands::agent_cmd::execute(&runtime, &cwd, command).await?,
        Command::Harness { command } => harness_cmd::execute(&cwd, command).await?,
        Command::Omo { command } => commands::omo_cmd::execute(command).await?,
        Command::Worktree { command } => commands::worktree_cmd::execute(&cwd, command).await?,
        Command::Provider { command } => {
            provider_cmd::execute(command).await?;
        }
        Command::History { command } => {
            history_cmd::execute(command)?;
        }
        Command::Skills { command } => {
            skills_cmd::execute(command)?;
        }
        Command::Learnings { command } => {
            execute_learnings_command(&cwd, command)?;
        }
        Command::Bench { command } => {
            commands::bench_cmd::execute(command)?;
        }
        #[allow(unreachable_patterns)]
        _ => {
            anyhow::bail!("Unknown command. Run 'rustycode --help' for available commands.");
        }
    }
    Ok(())
}

fn execute_learnings_command(cwd: &std::path::Path, command: LearningsCommand) -> Result<()> {
    use rustycode_core::team::team_learnings::{LearningCategory, TeamLearnings};

    let mut learnings = TeamLearnings::load(cwd)?;

    match command {
        LearningsCommand::Show | LearningsCommand::List => {
            println!("{}", learnings.get_all());
        }
        LearningsCommand::Add { category, content } => {
            let cat = match category.as_str() {
                "user-preference" => LearningCategory::UserPreference,
                "codebase-quirk" => LearningCategory::CodebaseQuirk,
                "what-worked" => LearningCategory::WhatWorked,
                "what-failed" => LearningCategory::WhatFailed,
                _ => {
                    eprintln!("Invalid category: {}. Use: user-preference, codebase-quirk, what-worked, what-failed", category);
                    std::process::exit(1);
                }
            };
            learnings.record(cat, content, None);
            learnings.save()?;
            println!("✓ Learning recorded");
        }
        LearningsCommand::Remove { category, content } => {
            let cat = match category.as_str() {
                "user-preference" => LearningCategory::UserPreference,
                "codebase-quirk" => LearningCategory::CodebaseQuirk,
                "what-worked" => LearningCategory::WhatWorked,
                "what-failed" => LearningCategory::WhatFailed,
                _ => {
                    eprintln!("Invalid category: {}. Use: user-preference, codebase-quirk, what-worked, what-failed", category);
                    std::process::exit(1);
                }
            };
            if learnings.remove(&cat, &content) {
                println!("✓ Learning removed");
            } else {
                eprintln!("No matching learning found");
            }
        }
        LearningsCommand::Clear { yes } => {
            if yes {
                learnings.clear();
                learnings.save()?;
                println!("✓ All learnings cleared");
            } else {
                eprintln!("⚠️  This will delete all learnings. Re-run with --yes to confirm.");
            }
        }
        #[allow(unreachable_patterns)]
        _ => {
            eprintln!("Unknown learnings command. Run --help for usage.");
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn watch_events(
    runtime: &AsyncRuntime,
    pattern: &str,
    limit: usize,
    timeout_ms: u64,
    run: Option<String>,
    tool: Option<String>,
    plan: Option<String>,
    approve_session: Option<String>,
    reject_session: Option<String>,
    params: &str,
) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let (_subscription_id, mut rx) = runtime.subscribe_events(pattern).await?;

    if let Some(prompt) = run {
        let _ = runtime.run(&cwd, &prompt).await?;
    }

    if let Some(tool_name) = tool {
        let arguments: serde_json::Value = serde_json::from_str(params)
            .map_err(|e| anyhow::anyhow!("--params must be valid JSON: {e}"))?;
        let _ = runtime.run_tool(&cwd, tool_name, arguments).await?;
    }

    if let Some(task) = plan {
        let _ = runtime.start_planning(&cwd, &task).await?;
    }

    if let Some(session_id) = approve_session {
        let session_id = SessionId::parse(&session_id)?;
        runtime.approve_plan(&session_id, &cwd).await?;
    }

    if let Some(session_id) = reject_session {
        let session_id = SessionId::parse(&session_id)?;
        runtime.reject_plan(&session_id, &cwd).await?;
    }

    for _ in 0..limit {
        let event = tokio::time::timeout(Duration::from_millis(timeout_ms), rx.recv()).await;
        match event {
            Ok(Ok(event)) => {
                let payload = rustycode_bus::Event::serialize(event.as_ref());
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "event_type": event.event_type(),
                        "payload": payload,
                    }))?
                );
            }
            Ok(Err(error)) => {
                return Err(anyhow::anyhow!("event subscription closed: {error}"));
            }
            Err(_) => break,
        }
    }
    Ok(())
}

/// Suggest similar subcommand names when a task looks like it could be a subcommand
fn suggest_similar_subcommand(task: &str) -> Option<String> {
    const SUBCOMMANDS: &[&str] = &[
        "doctor",
        "config",
        "context",
        "run",
        "tools",
        "sessions",
        "events",
        "plan",
        "agent",
        "harness",
        "omo",
        "worktree",
        "orchestra",
        "provider",
        "history",
        "skills",
        "learnings",
        "tui",
        "serve",
        "swebench",
        "bench",
    ];

    // Check if task looks like a subcommand (lowercase, no spaces, no special chars)
    let looks_like_command = task
        .chars()
        .all(|c| c.is_lowercase() || c == '-' || c == '_')
        && !task.contains(' ')
        && task.len() >= 2
        && task.len() <= 20;

    if !looks_like_command {
        return None;
    }

    // Find closest match using simple Levenshtein distance
    let task_lower = task.to_lowercase();
    let mut best_match: Option<(&str, usize)> = None;

    for &subcmd in SUBCOMMANDS {
        let distance = levenshtein_distance(&task_lower, subcmd);
        // Only suggest if within 3 edits and not exact match
        if distance <= 3 && distance > 0 {
            match best_match {
                None => best_match = Some((subcmd, distance)),
                Some((_, d)) if distance < d => best_match = Some((subcmd, distance)),
                _ => {}
            }
        }
    }

    best_match.map(|(subcmd, _)| subcmd.to_string())
}

/// Simple Levenshtein distance implementation
#[allow(clippy::needless_range_loop)]
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    } else if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = std::cmp::min(
                std::cmp::min(matrix[i - 1][j] + 1, matrix[i][j - 1] + 1),
                matrix[i - 1][j - 1] + cost,
            );
        }
    }

    matrix[a_len][b_len]
}

/// Handle Orchestra commands separately to avoid runtime loading issues
async fn handle_orchestra_command(
    cwd: std::path::PathBuf,
    orchestra_cmd: OrchestraCommand,
    format: String,
) -> Result<()> {
    use rustycode_cli::commands::orchestra::*;

    match orchestra_cmd {
        OrchestraCommand::Init {
            name,
            description,
            vision,
        } => {
            #[cfg(feature = "orchestra-v2")]
            {
                commands::orchestra::auto_command::init_project(cwd, name, description, vision)
                    .await?;
            }
            #[cfg(not(feature = "orchestra-v2"))]
            {
                init(cwd, name, description, vision)?;
            }
        }
        OrchestraCommand::Progress => {
            progress(cwd)?;
        }
        OrchestraCommand::State => {
            state(cwd)?;
        }
        OrchestraCommand::NewMilestone { id, title, vision } => {
            new_milestone(cwd, id, title, vision)?;
        }
        OrchestraCommand::ListMilestones => {
            list_milestones(cwd)?;
        }
        OrchestraCommand::PlanPhase {
            id,
            title,
            goal,
            demo,
            risk,
        } => {
            plan_phase(cwd, id, title, goal, demo, risk)?;
        }
        OrchestraCommand::ExecutePhase => {
            execute_phase(cwd)?;
        }
        OrchestraCommand::VerifyPhase => {
            verify_phase(cwd)?;
        }
        OrchestraCommand::Docs => {
            help()?;
        }
        OrchestraCommand::Health => {
            health(cwd)?;
        }
        OrchestraCommand::Quick { task } => {
            #[cfg(feature = "orchestra-v2")]
            {
                commands::orchestra::auto_command::run_quick_task(cwd, task, None).await?;
            }
            #[cfg(not(feature = "orchestra-v2"))]
            {
                quick(cwd, task)?;
            }
        }
        #[cfg(feature = "orchestra-v2")]
        OrchestraCommand::Auto { budget, max_units } => {
            commands::orchestra::auto_command::run_auto_mode(cwd, budget, max_units).await?;
        }
        #[cfg(not(feature = "orchestra-v2"))]
        OrchestraCommand::Auto { .. } => {
            anyhow::bail!("The 'auto' command requires the 'orchestra-v2' feature. Enable it with: --features orchestra-v2");
        }
        OrchestraCommand::AddPhase {
            id,
            title,
            goal,
            demo,
            risk,
        } => {
            add_phase_cmd(cwd, id, title, goal, demo, risk)?;
        }
        OrchestraCommand::InsertPhase {
            id,
            title,
            goal,
            after_phase,
            risk,
        } => {
            insert_phase_cmd(cwd, id, title, goal, after_phase, risk)?;
        }
        OrchestraCommand::RemovePhase { id } => {
            remove_phase_cmd(cwd, id)?;
        }
        OrchestraCommand::CompleteMilestone { id } => {
            complete_milestone_cmd(cwd, id)?;
        }
        OrchestraCommand::Cleanup { max_age_days } => {
            cleanup_cmd(cwd, max_age_days)?;
        }
        OrchestraCommand::AddTodo { description } => {
            add_todo_cmd(cwd, description)?;
        }
        OrchestraCommand::ListTodos => {
            list_todos_cmd(cwd)?;
        }
        OrchestraCommand::CompleteTodo { description } => {
            complete_todo_cmd(cwd, description)?;
        }
        OrchestraCommand::CleanupTodos => {
            cleanup_todos_cmd(cwd)?;
        }
        OrchestraCommand::SetProfile { profile } => {
            set_profile_cmd(cwd, profile)?;
        }
        OrchestraCommand::ShowConfig => {
            show_config_cmd(cwd)?;
        }
        OrchestraCommand::AgentPlan {
            id,
            milestone,
            title,
            goal,
            demo,
            risk,
        } => {
            use rustycode_cli::commands::orchestra::agents::*;
            plan_phase_agent(cwd, id, milestone, title, goal, demo, risk)?;
        }
        OrchestraCommand::AgentExecute { id, milestone } => {
            use rustycode_cli::commands::orchestra::agents::*;
            execute_phase_agent(cwd, id, milestone)?;
        }
        OrchestraCommand::AgentVerify { id, milestone } => {
            use rustycode_cli::commands::orchestra::agents::*;
            verify_phase_agent(cwd, id, milestone)?;
        }
        OrchestraCommand::MapCodebase => {
            use rustycode_cli::commands::orchestra::*;
            map_codebase(cwd)?;
        }
        OrchestraCommand::AddTests { id } => {
            use rustycode_cli::commands::orchestra::*;
            add_tests(cwd, id)?;
        }
        OrchestraCommand::DiagnoseIssues => {
            use rustycode_cli::commands::orchestra::*;
            diagnose_issues(cwd)?;
        }
        OrchestraCommand::ResearchPhase { id, topic } => {
            use rustycode_cli::commands::orchestra::*;
            research_phase(cwd, id, topic)?;
        }
        OrchestraCommand::PauseWork { note } => {
            use rustycode_cli::commands::orchestra::*;
            pause_work(cwd, note)?;
        }
        OrchestraCommand::ResumeWork => {
            use rustycode_cli::commands::orchestra::*;
            resume_work(cwd)?;
        }
        OrchestraCommand::DiscussPhase { id } => {
            use rustycode_cli::commands::orchestra::*;
            discuss_phase(cwd, id)?;
        }
        OrchestraCommand::NewProjectEnhanced {
            name,
            description,
            vision,
            interactive,
        } => {
            use rustycode_cli::commands::orchestra::*;
            new_project_enhanced(cwd, name, description, vision, interactive)?;
        }
        OrchestraCommand::PlanMilestoneGaps { id } => {
            use rustycode_cli::commands::orchestra::*;
            plan_milestone_gaps(cwd, id)?;
        }
        OrchestraCommand::Suggest => {
            use rustycode_cli::commands::orchestra::*;
            suggest_workflows(cwd)?;
        }
        OrchestraCommand::Visualize => {
            use rustycode_cli::commands::orchestra::*;
            visualize_progress(cwd)?;
        }
        OrchestraCommand::Chain {
            name,
            args,
            interactive,
            dry_run,
            verbose,
        } => {
            use rustycode_cli::commands::orchestra::*;
            let output_format = OutputFormat::parse_format(&format);
            if interactive {
                execute_chain_interactive(cwd, name.clone(), args.clone(), dry_run, verbose)?;
            } else {
                execute_chain(
                    cwd,
                    name.clone(),
                    args.clone(),
                    dry_run,
                    verbose,
                    output_format,
                )?;
            }
        }
        OrchestraCommand::ListChains => {
            use rustycode_cli::commands::orchestra::*;
            list_chains(cwd)?;
        }
        OrchestraCommand::CreateChain { name } => {
            use rustycode_cli::commands::orchestra::*;
            create_chain_template(cwd, name)?;
        }
        OrchestraCommand::ListChainTemplates => {
            use rustycode_cli::commands::orchestra::*;
            list_chain_templates()?;
        }
        OrchestraCommand::CreateChainFromTemplate {
            template,
            name,
            vars,
        } => {
            use rustycode_cli::commands::orchestra::*;
            // Parse key=value pairs
            let parsed_vars: Result<Vec<(String, String)>, String> =
                vars.iter().map(|s| parse_key_value(s)).collect();
            let parsed_vars =
                parsed_vars.map_err(|e| anyhow::anyhow!("Invalid variable format: {}", e))?;
            create_chain_from_template(cwd, &template, name, parsed_vars)?;
        }
        OrchestraCommand::ValidateChain { name } => {
            use rustycode_cli::commands::orchestra::*;
            validate_chain_command(cwd, name)?;
        }
        OrchestraCommand::ExportChain {
            name,
            format,
            output,
        } => {
            use rustycode_cli::commands::orchestra::*;
            export_chain_command(cwd, name, format, output)?;
        }
        OrchestraCommand::ChainStats { name } => {
            use rustycode_cli::commands::orchestra::*;
            chain_stats_command(cwd, name)?;
        }
        OrchestraCommand::ResetChainStats { name } => {
            use rustycode_cli::commands::orchestra::*;
            reset_chain_stats_command(cwd, name)?;
        }
        OrchestraCommand::CompareChains { chain1, chain2 } => {
            use rustycode_cli::commands::orchestra::*;
            compare_chains_command(cwd, chain1, chain2)?;
        }
        OrchestraCommand::BulkExportChains { format, output_dir } => {
            use rustycode_cli::commands::orchestra::*;
            bulk_export_chains_command(cwd, format, output_dir)?;
        }
        OrchestraCommand::BulkValidateChains => {
            use rustycode_cli::commands::orchestra::*;
            bulk_validate_chains_command(cwd)?;
        }
        #[allow(unreachable_patterns)]
        _ => {
            anyhow::bail!("Unknown Orchestra command. Run --help for usage.");
        }
    }
    Ok(())
}
