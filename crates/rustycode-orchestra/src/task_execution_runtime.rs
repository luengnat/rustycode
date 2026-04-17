//! Runtime helpers for execute-task units.
//!
//! This module owns the task execution conversation loop, tool dispatch,
//! and verification retry flow so `orchestra_executor.rs` can stay focused on
//! higher-level orchestration.

use anyhow::{anyhow, Context};
use rustycode_llm::{ChatMessage, CompletionRequest};
use rustycode_tools::{ReadFileTool, Tool, ToolContext, WriteFileTool};
use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::context_budget::{budget_usage_percent, compute_budgets};
use crate::conversation_runtime::{
    append_assistant_and_tool_results, messages_to_text, stream_text_round, stream_tool_round,
    tool_schemas, PendingToolCall,
};
use crate::crash_recovery::{ActivityEvent, ActivityType};
use crate::post_unit_runtime::{write_task_summary, ExecutionStats};
use crate::session_context::{build_session_context, format_prior_summaries};
use crate::skill_discovery::{format_skills_markdown, DiscoveredSkill};
use crate::task_control_runtime::{evaluate_task_attempt, TaskControlDecision};
use crate::timeout::{TimeoutAction, UnitTimeoutState};
use crate::token_counter::{estimate_tokens_for_provider, parse_token_provider};
use crate::tool_tracking::{mark_tool_end, mark_tool_start};
use crate::verification_retry_state::{
    clear_pending_verification_retry, new_pending_verification_retry,
    save_pending_verification_retry, touch_pending_verification_retry,
};
use crate::Orchestra2Executor;

impl Orchestra2Executor {
    /// Execute a single unit with LLM
    pub(crate) async fn execute_unit(
        &self,
        unit_id: &str,
        task_plan: &str,
        timeout_state: &mut UnitTimeoutState,
        discovered_skills: &[DiscoveredSkill],
    ) -> anyhow::Result<ExecutionStats> {
        let state = self.state_deriver.derive_state()?;

        let (milestone_id, slice_id) =
            if let (Some(ref m), Some(ref s)) = (&state.active_milestone, &state.active_slice) {
                (m.id.clone(), s.id.clone())
            } else {
                return Err(anyhow!("No active milestone or slice for task {}", unit_id));
            };

        let session_ctx =
            build_session_context(&self.project_root, &milestone_id, &slice_id, unit_id)?;

        let skills_section = if !discovered_skills.is_empty() {
            format_skills_markdown(discovered_skills)
        } else {
            String::new()
        };

        let system_prompt = format!(
            "You are executing Orchestra auto-mode.\n\n\
            ## UNIT: Execute Task {} (\"{}\") — Slice {} (\"{}\"), Milestone {}\n\n\
            ## Working Directory\n\n\
            Your working directory is `{}`. All file reads, writes, and shell commands MUST operate relative to this directory. Do NOT `cd` to any other directory.\n\n\
            A researcher explored the codebase and a planner decomposed the work — you are the executor. The task plan below is your authoritative contract. It contains the specific files, steps, and verification you need. Don't re-research or re-plan — build what the plan says, verify it works, and document what happened.\n\n\
            ## Slice Plan Excerpt (Goal & Verification)\n\n\
            {}\n\n\
            ## Task Plan (Your Contract)\n\n\
            {}\n\n\
            ## Prior Task Summaries in This Slice\n\n\
            {}\n\n\
            {}\n\n\
            Then:\n\
            1. Execute the steps in the task plan\n\
            2. Build the real thing (no stubs or mocks)\n\
            3. Verify it works with concrete checks\n\
            4. Write a task summary when done\n\
            5. Mark {} as done in the slice plan\n\n\
            All work stays in your working directory: `{}`.\n\n\
            **You MUST mark {} as done AND write a task summary before finishing.**\n\n\
            When done, say: \"Task {} complete.\"",
            unit_id,
            session_ctx.task_title,
            session_ctx.slice_id,
            session_ctx.slice_title,
            session_ctx.milestone_id,
            self.project_root.display(),
            session_ctx.slice_excerpt,
            task_plan,
            format_prior_summaries(&session_ctx.prior_summaries),
            skills_section,
            unit_id,
            self.project_root.display(),
            unit_id,
            unit_id
        );

        let messages = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(format!("Execute task: {}", unit_id)),
        ];

        self.execute_conversation(messages, unit_id, &milestone_id, &slice_id, timeout_state)
            .await
    }

    /// Execute multi-turn conversation with tool use
    pub(crate) async fn execute_conversation(
        &self,
        mut messages: Vec<ChatMessage>,
        unit_id: &str,
        _milestone_id: &str,
        _slice_id: &str,
        timeout_state: &mut UnitTimeoutState,
    ) -> anyhow::Result<ExecutionStats> {
        let max_turns = 20;
        let mut turn = 0;

        let budgets = compute_budgets(self.model_context_window);
        info!(
            "📊 Context budget: {} inline, {} summary, {} verification",
            budgets.inline_context_budget_chars,
            budgets.summary_budget_chars,
            budgets.verification_budget_chars
        );

        let mut tokens_used_this_session: usize = 0;
        let provider = parse_token_provider(&self.get_model());
        let mut tokens_in_total: u32 = 0;
        let mut tokens_out_total: u32 = 0;
        let tools_schema = tool_schemas(&self.tool_registry);
        info!("🔧 Registered {} tools for LLM", tools_schema.len());

        loop {
            turn += 1;
            if turn > max_turns {
                warn!("Reached max turns ({}), stopping", max_turns);
                break;
            }

            match self.timeout_supervisor.check_timeouts(timeout_state) {
                TimeoutAction::Continue => {}
                TimeoutAction::Warn(msg) => warn!("⏱️  Timeout warning: {}", msg),
                TimeoutAction::Retry(msg) => {
                    warn!("⏱️  Idle timeout, retrying: {}", msg);
                    return Err(anyhow!("Unit idle timed out: {}", msg));
                }
                TimeoutAction::Kill(msg) => {
                    error!("⏱️  Hard timeout: {}", msg);
                    return Err(anyhow!("Unit hard timed out: {}", msg));
                }
            }

            self.timeout_supervisor.record_progress(timeout_state);

            let usage_percent =
                budget_usage_percent(tokens_used_this_session, self.model_context_window);
            if usage_percent >= budgets.continue_threshold_percent as f64 {
                warn!(
                    "⚠️  Context usage at {:.0}% ({} / {} tokens) - approaching limit",
                    usage_percent, tokens_used_this_session, self.model_context_window
                );
            }

            if usage_percent >= 100.0 {
                return Err(anyhow!(
                    "Context budget exceeded: {} tokens used",
                    tokens_used_this_session
                ));
            }

            let estimated_input_tokens =
                estimate_tokens_for_provider(&messages_to_text(&messages), provider);
            tokens_in_total = tokens_in_total.saturating_add(estimated_input_tokens as u32);
            tokens_used_this_session =
                tokens_used_this_session.saturating_add(estimated_input_tokens);

            let mut request = CompletionRequest::new(self.get_model(), messages.clone())
                .with_max_tokens(4096)
                .with_temperature(0.1)
                .with_streaming(true);

            let (assistant_response, pending_tool_calls) = if turn == 1 {
                request = request.with_tools(tools_schema.clone());
                info!("📋 Added {} tools to request", tools_schema.len());
                stream_tool_round(&self.provider, request).await?
            } else {
                (
                    stream_text_round(&self.provider, request).await?,
                    Vec::new(),
                )
            };

            let assistant_output_tokens =
                estimate_tokens_for_provider(&assistant_response, provider);
            tokens_out_total = tokens_out_total.saturating_add(assistant_output_tokens as u32);
            tokens_used_this_session =
                tokens_used_this_session.saturating_add(assistant_output_tokens);

            let tool_results = if pending_tool_calls.is_empty() {
                debug!("\n✅ Execution complete - no more tools");
                if turn == 1 {
                    break;
                }
                Vec::new()
            } else {
                self.execute_pending_tool_calls(unit_id, pending_tool_calls, Some(timeout_state))
                    .await?
            };

            if tool_results.is_empty() {
                if turn != 1 {
                    break;
                }
            } else {
                for (_, result) in &tool_results {
                    let result_tokens = estimate_tokens_for_provider(result, provider);
                    tokens_used_this_session =
                        tokens_used_this_session.saturating_add(result_tokens);
                }
                append_assistant_and_tool_results(&mut messages, assistant_response, tool_results);
            }
        }

        Ok(ExecutionStats {
            tokens_in: tokens_in_total,
            tokens_out: tokens_out_total,
            total_tokens: tokens_in_total.saturating_add(tokens_out_total),
        })
    }

    pub(crate) async fn run_task_verification_with_retries(
        &self,
        state: &crate::state_derivation::OrchestraState,
        unit_id: &str,
        task_plan: &str,
        task_plan_fingerprint: &str,
        aggregate_stats: &mut ExecutionStats,
        timeout_state: &mut UnitTimeoutState,
        discovered_skills: &[DiscoveredSkill],
    ) -> anyhow::Result<crate::post_unit_runtime::TaskExecutionOutcome> {
        const MAX_RETRIES: u32 = 2;

        let task = state
            .active_task
            .as_ref()
            .ok_or_else(|| anyhow!("No active task for verification flow"))?;
        let mut current_plan = task_plan.to_string();
        let mut attempt: u32 = 0;

        loop {
            match evaluate_task_attempt(
                &self.project_root,
                unit_id,
                &current_plan,
                &task.path,
                attempt,
                MAX_RETRIES,
            )? {
                TaskControlDecision::Passed => {
                    if let Err(e) = clear_pending_verification_retry(&self.project_root, unit_id) {
                        warn!(
                            "Failed to clear pending verification retry for {}: {}",
                            unit_id, e
                        );
                    }
                    let summary = format!(
                        "Task completed successfully after {} verification attempt(s).",
                        attempt + 1
                    );
                    if let Err(e) = write_task_summary(&self.project_root, unit_id, &summary).await
                    {
                        warn!("Failed to write summary: {}", e);
                    }
                    return Ok(crate::post_unit_runtime::TaskExecutionOutcome { passed: true });
                }
                TaskControlDecision::Failed => {
                    if let Err(e) = clear_pending_verification_retry(&self.project_root, unit_id) {
                        warn!(
                            "Failed to clear pending verification retry for {}: {}",
                            unit_id, e
                        );
                    }
                    return Ok(crate::post_unit_runtime::TaskExecutionOutcome { passed: false });
                }
                TaskControlDecision::Retry {
                    next_plan,
                    failure_context,
                    attempt: prior_attempt,
                } => {
                    let next_attempt = prior_attempt + 1;
                    attempt = next_attempt;
                    current_plan = next_plan;

                    if let Err(e) = save_pending_verification_retry(&self.project_root, &{
                        let mut retry = new_pending_verification_retry(
                            unit_id.to_string(),
                            task_plan_fingerprint.to_string(),
                            failure_context,
                            next_attempt,
                            MAX_RETRIES,
                        );
                        touch_pending_verification_retry(&mut retry);
                        retry
                    }) {
                        warn!(
                            "Failed to persist pending verification retry for {}: {}",
                            unit_id, e
                        );
                    }

                    let retry_stats = self
                        .execute_unit(unit_id, &current_plan, timeout_state, discovered_skills)
                        .await?;
                    aggregate_stats.add(&retry_stats);
                }
            }
        }
    }

    pub(crate) async fn execute_pending_tool_calls(
        &self,
        activity_unit_id: &str,
        pending_tool_calls: Vec<PendingToolCall>,
        timeout_state: Option<&mut UnitTimeoutState>,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let mut tool_results = Vec::new();
        let mut timeout_state = timeout_state;

        for tool_call in pending_tool_calls {
            debug!("\n🔧 Executing tool: {}", tool_call.name);
            let tool_call_id = format!("{}:{}", activity_unit_id, tool_call.id);
            mark_tool_start(tool_call_id.clone(), true);

            if let Some(state) = timeout_state.as_deref_mut() {
                self.timeout_supervisor.record_progress(state);
            }

            let result = match self
                .execute_tool(&tool_call.name, &tool_call.input_json)
                .await
            {
                Ok(result) => result,
                Err(err) => {
                    mark_tool_end(tool_call_id);
                    return Err(err);
                }
            };

            debug!("✅ Tool result: {}", result);
            mark_tool_end(tool_call_id);

            if let Some(state) = timeout_state.as_deref_mut() {
                self.timeout_supervisor.record_progress(state);
            }

            self.activity_log
                .log(ActivityEvent {
                    timestamp: chrono::Utc::now(),
                    unit_id: activity_unit_id.to_string(),
                    event_type: ActivityType::ToolUse,
                    detail: serde_json::json!({
                        "tool": tool_call.name,
                        "input": tool_call.input_json,
                    }),
                })
                .await?;

            self.activity_log
                .log(ActivityEvent {
                    timestamp: chrono::Utc::now(),
                    unit_id: activity_unit_id.to_string(),
                    event_type: ActivityType::ToolResult,
                    detail: serde_json::json!({
                        "tool": tool_call.name,
                        "result": result.chars().take(1000).collect::<String>(),
                    }),
                })
                .await?;

            tool_results.push((tool_call.id, result));
        }

        Ok(tool_results)
    }

    async fn execute_tool(&self, name: &str, input: &str) -> anyhow::Result<String> {
        debug!("Executing tool {} with input: {}", name, input);
        let params: serde_json::Value = serde_json::from_str(input)
            .with_context(|| format!("Invalid JSON for tool {}: '{}'", name, input))?;
        self.execute_tool_call(name, &params).await
    }

    async fn execute_tool_call(
        &self,
        name: &str,
        params: &serde_json::Value,
    ) -> anyhow::Result<String> {
        let project_root = self.project_root.clone();
        let name = name.to_string();
        let name_for_error = name.clone();
        let params = params.clone();

        const TOOL_TIMEOUT_SECS: u64 = 120;

        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(TOOL_TIMEOUT_SECS),
            tokio::task::spawn_blocking(move || {
                let tool_ctx = ToolContext::new(&project_root);

                let tool_result = match name.as_str() {
                    "read_file" => ReadFileTool.execute(params, &tool_ctx)?,
                    "write_file" => WriteFileTool.execute(params, &tool_ctx)?,
                    "bash" | "run_shell" => {
                        let command = params
                            .get("command")
                            .and_then(Value::as_str)
                            .ok_or_else(|| anyhow!("missing string parameter 'command'"))?;

                        let output = std::process::Command::new("bash")
                            .arg("-c")
                            .arg(command)
                            .current_dir(&tool_ctx.cwd)
                            .output()
                            .context("Failed to execute bash command")?;

                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        let exit_code = output.status.code().unwrap_or(-1);

                        rustycode_tools::ToolOutput {
                            text: format!(
                                "exit: {}\nstdout:\n{}\nstderr:\n{}",
                                exit_code, stdout, stderr
                            ),
                            structured: None,
                        }
                    }
                    _ => {
                        return Ok(format!("Unknown tool: {}", name));
                    }
                };

                Ok::<_, anyhow::Error>(tool_result.text)
            }),
        )
        .await
        .map_err(|_| {
            anyhow!(
                "Tool '{}' timed out after {}s",
                name_for_error,
                TOOL_TIMEOUT_SECS
            )
        })?
        .context("Failed to join tool execution thread")??;

        Ok(result)
    }
}
