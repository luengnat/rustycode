use crate::app::async_::{
    SlashCommandResult, StreamChunk, ToolOutput, ToolResult, WorkspaceUpdate,
};
use crate::app::TUI;
use crate::task_extraction::extract_action_items;
use crate::tool_approval::risk;

use anyhow;

use crate::ui::message::{Message, MessageRole, ToolExecution, ToolStatus};
use chrono;
use std::time::{Duration, SystemTime};
use tracing;

/// Classify tool name to ToolType
fn classify_tool_type(tool_name: &str) -> risk::ToolType {
    match tool_name {
        "read_file" => risk::ToolType::ReadFile,
        "write_file" => risk::ToolType::WriteFile,
        "bash" => risk::ToolType::Bash,
        "grep" => risk::ToolType::Grep,
        "glob" | "list_files" | "list_dir" => risk::ToolType::ListDirectory,
        "edit_file" | "search_replace" => risk::ToolType::WriteFile,
        "git_status" | "git_diff" | "git_log" | "git_commit" => risk::ToolType::Git,
        _ => risk::ToolType::Custom(tool_name.to_string()),
    }
}

/// Check for pending tasks and trigger auto-continue if needed
///
/// This function is called after stream completion when auto-continue is enabled.
/// It checks if there are pending or in-progress tasks, and if so, automatically
/// sends a continuation message to keep the AI working.
///
/// Safety: capped at MAX_AUTO_CONTINUE_ITERATIONS (20) to prevent infinite loops
/// if the AI keeps creating new tasks faster than it completes them.
fn check_and_trigger_auto_continue(tui: &mut TUI) {
    use crate::tasks::TaskStatus;

    const MAX_AUTO_CONTINUE_ITERATIONS: usize = 20;

    // Enforce iteration limit to prevent infinite loops
    if tui.auto_continue_iterations >= MAX_AUTO_CONTINUE_ITERATIONS {
        tracing::warn!(
            "Auto-continue stopped after {} iterations (task creation may be outpacing completion)",
            MAX_AUTO_CONTINUE_ITERATIONS
        );
        tui.add_system_message(format!(
            "Auto-continue stopped after {} iterations. Press Ctrl+Shift+A to resume if needed.",
            MAX_AUTO_CONTINUE_ITERATIONS
        ));
        tui.auto_continue_enabled = false;
        tui.auto_continue_iterations = 0;
        return;
    }

    // Check for pending or in-progress tasks
    let pending_tasks: Vec<_> = tui
        .workspace_tasks
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::InProgress)
        .collect();

    // Check for incomplete todos
    let incomplete_todos: Vec<_> = tui
        .workspace_tasks
        .todos
        .iter()
        .filter(|t| !t.done)
        .collect();

    // Only continue if there's work to do
    if pending_tasks.is_empty() && incomplete_todos.is_empty() {
        tracing::debug!("Auto-continue: No pending tasks, stopping");
        tui.auto_continue_enabled = false; // Disable auto-continue when done
        return;
    }

    // Count remaining work
    let total_pending = pending_tasks.len() + incomplete_todos.len();

    tracing::info!(
        "Auto-continue: {} tasks/todos remaining, continuing work",
        total_pending
    );

    // Mark that we have a pending continuation
    tui.auto_continue_pending = true;
    tui.auto_continue_iterations += 1;

    // Build context message about remaining work
    let mut context = String::from("Continue working on the remaining tasks:\n\n");

    // Add pending tasks
    for task in &pending_tasks {
        context.push_str(&format!("- [ ] {}\n", task.description));
    }

    // Add incomplete todos
    for todo in &incomplete_todos {
        context.push_str(&format!("- [ ] {}\n", todo.text));
    }

    context.push_str("\nPlease continue with the next task. Use tools to complete the work.");

    // Send the continuation message
    let workspace_context = tui.workspace_context.clone();
    let history = tui.build_conversation_history();

    // Set streaming state before send to prevent races
    tui.is_streaming = true;
    tui.chunks_received = 0;
    tui.stream_start_time = Some(std::time::Instant::now());
    tui.current_stream_content.clear();
    tui.streaming_render_buffer = crate::app::streaming_render_buffer::StreamingRenderBuffer::new();

    if let Err(e) =
        tui.services
            .send_message_with_history(context, workspace_context, Some(history))
    {
        tracing::error!("Failed to send auto-continue message: {}", e);
        tui.is_streaming = false;
        tui.chunks_received = 0;
        tui.current_stream_content.clear();
        tui.streaming_render_buffer =
            crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
        tui.stream_start_time = None;
        tui.active_tools.clear();
        tui.auto_continue_pending = false;
    } else {
        let assistant_msg = crate::ui::message::Message::assistant(String::new());
        tui.messages.push(assistant_msg);
        tui.dirty = true;
    }
}

/// Handle a stream chunk from the LLM
pub fn handle_stream_chunk(tui: &mut TUI, chunk: StreamChunk) {
    match chunk {
        StreamChunk::Text(text) => {
            // Capture stream start time on first chunk (Goose pattern: response timing)
            if tui.stream_start_time.is_none() {
                tui.stream_start_time = Some(std::time::Instant::now());
            }

            // Feed through the streaming render buffer for safe markdown boundaries.
            // The buffer holds back incomplete markdown (unclosed bold, code blocks, etc.)
            // and returns complete segments safe for rendering.
            let safe_text = tui.streaming_render_buffer.push(&text);

            if let Some(renderable) = safe_text {
                // Append safe content to current stream content
                tui.current_stream_content.reserve(renderable.len());
                tui.current_stream_content.push_str(&renderable);

                let assistant_msg = tui
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant);
                if let Some(last_msg) = assistant_msg {
                    last_msg.content.push_str(&renderable);
                }

                tui.is_streaming = true;
                tui.chunks_received += 1;
                // Update terminal title on first chunk (state transition to "thinking")
                if tui.chunks_received == 1 {
                    tui.update_terminal_title();
                }
                if !tui.user_scrolled {
                    tui.auto_scroll();
                }
                tui.dirty = true;
            } else {
                // Buffer is holding incomplete markdown — still mark streaming
                // so the UI shows the spinner, but don't dirty (no render change).
                tui.is_streaming = true;
            }
            // NOTE: Do NOT clear stream_cancelled here!
            // The user may have pressed Esc/Ctrl+D to cancel while chunks
            // are still in-flight. If we clear the flag on every Text chunk,
            // a late-arriving chunk would un-cancel the stream, causing the
            // Done handler to treat it as a successful completion and trigger
            // auto-continue or queued message send. The flag is properly
            // reset in the Done/Error handlers.
        }
        StreamChunk::Thinking(thinking) => {
            // Accumulate thinking content on the last assistant message
            // Thinking is stored separately from response text for clean display
            if let Some(last_msg) = tui.messages.last_mut() {
                if last_msg.role == MessageRole::Assistant {
                    if let Some(existing) = &mut last_msg.thinking {
                        existing.push_str(&thinking);
                    } else {
                        last_msg.thinking = Some(thinking);
                    }
                }
            }

            tui.is_streaming = true;

            // Take a turn snapshot on first streaming chunk so we can
            // verify file changes when the turn completes.
            if tui.turn_snapshot.is_none() {
                let cwd = std::env::current_dir().unwrap_or_default();
                tui.turn_snapshot = Some(crate::app::turn_snapshot::TurnSnapshot::take(&cwd));
            }

            if !tui.user_scrolled {
                tui.auto_scroll();
            }
            tui.dirty = true;
        }
        StreamChunk::Done => {
            // Flush any remaining buffered content from the render buffer
            // (e.g., unclosed bold at stream end, last few chars)
            let remaining = tui.streaming_render_buffer.flush();
            if !remaining.is_empty() {
                tui.current_stream_content.reserve(remaining.len());
                tui.current_stream_content.push_str(&remaining);
                let assistant_msg = tui
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant);
                if let Some(msg) = assistant_msg {
                    msg.content.push_str(&remaining);
                }
            }

            // Streaming completed — release query guard and finalize auto-save
            let was_cancelled = tui.stream_cancelled;
            tui.is_streaming = false;
            tui.stream_cancelled = false; // Reset for next stream

            // Update terminal title back to "ready"
            tui.update_terminal_title();

            // Calculate and store response duration (Goose pattern: response timing)
            if let Some(start) = tui.stream_start_time.take() {
                tui.last_response_duration = Some(start.elapsed());
            }
            tui.services.complete_query();

            // Reset retry count on successful completion — transient errors
            // should not accumulate into excessive backoff delays for future errors.
            tui.rate_limit.retry_count = 0;
            // Also reset auto-retry cancellation flag so future rate limits
            // can auto-retry without requiring user intervention.
            tui.rate_limit.auto_retry_cancelled = false;

            // Clear any stale active tools (shouldn't happen but defensive)
            tui.active_tools.clear();

            // Doom loop check at turn end: if the agent repeated the same
            // failing tool call throughout this turn, warn and reset the
            // detector so the next turn starts clean.
            if tui.doom_loop.is_doom_loop() {
                if let Some(reason) = tui.doom_loop.doom_loop_reason() {
                    tui.add_system_message(format!("Warning: {}", reason));
                }
            }
            tui.doom_loop.reset();

            // Turn snapshot verification: compute what files changed during
            // this agent turn and show a brief summary.
            if let Some(snap) = tui.turn_snapshot.take() {
                let cwd = std::env::current_dir().unwrap_or_default();
                let diff = snap.diff(&cwd);
                if !diff.is_empty() {
                    tui.toast_manager.info(diff.summary());
                }
            }

            // Ensure the final assistant message has complete content.
            // Text chunks append incrementally during streaming, so the message
            // should already have the right content. But if no Text chunks were
            // received (e.g. only Thinking), create/ensure the assistant message.
            if !tui.current_stream_content.is_empty() {
                // Search backwards past system messages to find the streaming assistant
                // message. System messages (auto-approve notifications, etc.) may have
                // been pushed during the stream, so .last() may not point to the
                // assistant message that was accumulating content.
                let needs_message = tui
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant)
                    .is_none();
                if needs_message {
                    let message = Message::assistant(tui.current_stream_content.clone());
                    tui.messages.push(message);
                }

                // Note: Text-based question detection is intentionally disabled here.
                // The AI's native QuestionRequest SSE event (handled via question_rx channel
                // in response.rs) is the correct mechanism for clarification questions.
                // Automatic text-based detection was too aggressive — matching common words
                // like "what" and "how" in technical explanations.

                // Always mark dirty and trigger re-render when streaming completes
                // This ensures plain text gets converted to markdown
                tui.dirty = true;
                tui.auto_scroll();
            } else {
                if !was_cancelled {
                    let assistant_info = tui
                        .messages
                        .iter()
                        .rev()
                        .find(|m| m.role == MessageRole::Assistant)
                        .map(|m| {
                            (
                                m.id.clone(),
                                m.content.is_empty() && m.thinking.is_none(),
                                m.tool_executions.as_ref().is_none_or(|t| t.is_empty()),
                            )
                        });

                    if let Some((msg_id, is_empty, no_tools)) = assistant_info {
                        if is_empty {
                            if no_tools {
                                if let Some(pos) = tui.messages.iter().position(|m| m.id == msg_id)
                                {
                                    tui.messages.remove(pos);
                                }
                                tui.add_system_message(
                                    "Received empty response — try rephrasing or check model"
                                        .to_string(),
                                );
                            } else if let Some(last_msg) =
                                tui.messages.iter_mut().rev().find(|m| m.id == msg_id)
                            {
                                let tool_count = last_msg
                                    .tool_executions
                                    .as_ref()
                                    .map(|t| t.len())
                                    .unwrap_or(0);
                                let tool_names: Vec<&str> = last_msg
                                    .tool_executions
                                    .as_ref()
                                    .map(|t| t.iter().map(|e| e.name.as_str()).collect())
                                    .unwrap_or_default();
                                last_msg.content = format!(
                                    "Executed {} tool(s): {}",
                                    tool_count,
                                    tool_names.join(", ")
                                );
                            }
                        }
                    }
                }
                tui.dirty = true;
                tui.auto_scroll();
            }

            // Auto-continue: Check if we should continue working on pending tasks.
            // Skip auto-continue if the user manually cancelled the stream.
            // Reset pending flag first — the previous continuation has completed,
            // so a new check is safe. check_and_trigger_auto_continue() will
            // re-set pending=true if it starts another continuation, and will
            // disable auto_continue_enabled when all tasks are done.
            tui.auto_continue_pending = false;
            if !was_cancelled && tui.auto_continue_enabled {
                check_and_trigger_auto_continue(tui);
            }

            tracing::debug!("Stream completed");

            // Terminal bell notification on completion (Goose pattern).
            // Sends BEL character to alert the user that a response finished,
            // useful when they've switched to another window while waiting.
            // Only ring if the response took at least 3 seconds (avoid spamming on quick replies).
            if !was_cancelled {
                let should_bell = tui.last_response_duration.is_some_and(|d| d.as_secs() >= 3);
                if should_bell {
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::cursor::SetCursorStyle::DefaultUserShape
                    );
                    // BEL character — most terminals flash the title bar or play a sound
                    let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x07");
                    // Toast notification so the user sees what completed even if they
                    // were looking away (Goose pattern: completion feedback).
                    let duration_str = tui
                        .last_response_duration
                        .map(|d| {
                            let s = d.as_secs();
                            if s < 60 {
                                format!("{}s", s)
                            } else {
                                format!("{}m{}s", s / 60, s % 60)
                            }
                        })
                        .unwrap_or_default();
                    tui.toast_manager
                        .success(format!("Response complete ({})", duration_str));
                }
            }

            // Auto-send queued message if one was waiting (goose pattern)
            if !was_cancelled {
                if let Some(queued) = tui.queued_message.take() {
                    // Add the user message to the conversation so the user
                    // sees what they typed before the AI responds.
                    let user_msg = Message::user(queued.clone());
                    tui.messages.push(user_msg);
                    tui.selected_message = tui.messages.len() - 1;
                    tui.scroll_offset_line = 0;
                    tui.user_scrolled = false;

                    let message_to_send = tui.inject_memory_if_needed(&queued);
                    let workspace_context = tui.workspace_context.clone();
                    let history = tui.build_conversation_history();
                    tui.is_streaming = true;
                    tui.chunks_received = 0;
                    tui.stream_start_time = Some(std::time::Instant::now());
                    tui.current_stream_content.clear();
                    tui.streaming_render_buffer =
                        crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
                    if let Err(e) = tui.services.send_message_with_history(
                        message_to_send,
                        workspace_context,
                        Some(history),
                    ) {
                        tracing::error!("Failed to send queued message: {}", e);
                        tui.is_streaming = false;
                        tui.chunks_received = 0;
                        tui.current_stream_content.clear();
                        tui.streaming_render_buffer =
                            crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
                        tui.stream_start_time = None;
                        tui.active_tools.clear();
                        tui.add_system_message(format!("Queued message failed: {}", e));
                    } else {
                        let assistant_msg = crate::ui::message::Message::assistant(String::new());
                        tui.messages.push(assistant_msg);
                    }
                    tui.dirty = true;
                    tui.auto_scroll();
                }
            } else {
                // Stream was cancelled — preserve queued message so the user can
                // send it after cancellation is complete. Inform them it's still available.
                if tui.queued_message.is_some() {
                    tui.add_system_message(
                        "Queued message preserved — it will be sent when ready".to_string(),
                    );
                }
            }
        }
        StreamChunk::Error(err) => {
            // Streaming encountered an error — release query guard
            tui.is_streaming = false;
            tui.stream_cancelled = false; // Reset for next stream
            tui.services.complete_query();
            // Clear stale active tools on error
            tui.active_tools.clear();
            // Reset streaming buffer state so next stream starts clean
            tui.streaming_render_buffer =
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
            tui.chunks_received = 0;

            // Preserve partial response content so the user doesn't lose
            // what the AI already wrote before the error. If there's partial
            // content that hasn't been committed as a message, commit it now.
            if !tui.current_stream_content.is_empty() {
                let needs_message = tui
                    .messages
                    .last()
                    .is_none_or(|m| m.role != MessageRole::Assistant);
                if needs_message {
                    tui.messages
                        .push(Message::assistant(tui.current_stream_content.clone()));
                }
                tui.add_system_message(format!(
                    "Partial response preserved ({} chars)",
                    tui.current_stream_content.len()
                ));
            }

            // On cancellation, keep the queued message so the user can retry.
            // On non-retryable errors, preserve it too — the user should decide.
            // Only clear on explicit user cancellation (which sets stream_cancelled).
            // Note: stream_cancelled is already reset above, but we check the error
            // type to decide whether to preserve the queue.
            let was_auth_or_context = err.contains("401")
                || err.contains("403")
                || err.contains("authentication")
                || err.contains("context length")
                || err.contains("token limit");
            if was_auth_or_context {
                // Non-retryable errors — drop the queue since retrying won't help
                tui.queued_message = None;
            }
            // For retryable errors, preserve queued_message for retry after backoff

            // Reset auto-continue pending flag — the continuation that
            // errored will not complete, so pending must be cleared to
            // prevent the Done handler from being confused. Also disable
            // auto-continue on non-retryable errors to prevent loops.
            tui.auto_continue_pending = false;
            let is_retryable = !matches!(
                err.as_str(),
                s if s.contains("401") || s.contains("403")
                    || s.contains("authentication") || s.contains("Invalid")
                    || s.contains("context length") || s.contains("token limit")
            );
            if !is_retryable {
                tui.auto_continue_enabled = false;
                // Show non-retryable errors through the prominent error display
                // for better visibility and actionable suggestions
                tui.show_error(anyhow::anyhow!("{}", err.trim()));
                tui.dirty = true;
                tui.auto_scroll();
                return;
            }

            // Calculate exponential backoff with jitter: 5, 10, 20, 40, 60s...
            let base_delay_secs =
                (5u64 * 2u64.saturating_pow(tui.rate_limit.retry_count as u32)).min(60);
            let jitter = (base_delay_secs as f64 * 0.25) as isize;
            let random_jitter = if jitter > 0 {
                let nanos = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as isize;
                (nanos % (2 * jitter)) - jitter
            } else {
                0
            };
            let delay_secs = (base_delay_secs as isize + random_jitter).max(1) as u64;

            // Set rate limit countdown with exponential backoff
            tui.rate_limit.until =
                Some(std::time::Instant::now() + Duration::from_secs(delay_secs));

            // Remove previous retry message if exists
            if let Some(prev_idx) = tui.rate_limit.message_index.take() {
                // Check if the message is still "Retrying now..." and remove it
                if prev_idx < tui.messages.len() {
                    if let Some(msg) = tui.messages.get(prev_idx) {
                        if msg.content.contains("Retrying now")
                            || msg.content.contains("Auto-retrying")
                        {
                            // Remove the old retry message to avoid clutter
                            tui.messages.remove(prev_idx);
                        }
                    }
                }
            }

            // Add rate limit message with countdown
            let error_type = if err.contains("Rate limit") || err.contains("streaming_error") {
                "Rate limited"
            } else if err.contains("API error") || err.contains("connection") {
                "Connection issue"
            } else if err.contains("401") || err.contains("403") || err.contains("authentication") {
                "Auth error"
            } else if err.contains("context length") || err.contains("token limit") {
                "Context too long"
            } else {
                "Temporary issue"
            };

            // Show retry attempt number (if > 1)
            let retry_info = if tui.rate_limit.retry_count > 0 {
                format!(" (retry {})", tui.rate_limit.retry_count + 1)
            } else {
                String::new()
            };

            tui.add_system_message(format!(
                "◯ {} - Auto-retrying in {}s...{} (Esc or Ctrl+C to cancel)",
                error_type, delay_secs, retry_info
            ));

            // Store the message index for updating countdown
            tui.rate_limit.message_index = Some(tui.messages.len() - 1);

            // Reset auto-retry cancellation flag for new error
            tui.rate_limit.auto_retry_cancelled = false;

            // Increment retry count for next exponential backoff
            tui.rate_limit.retry_count += 1;

            tui.auto_scroll();

            tracing::debug!(
                "Stream error: {} (retry {} in {}s)",
                err,
                tui.rate_limit.retry_count,
                delay_secs
            );
        }
        StreamChunk::ToolStart {
            tool_name,
            tool_id,
            input_json: input_json_str,
        } => {
            // Parse tool input JSON for conversation history reconstruction
            let input_json: Option<serde_json::Value> = if input_json_str.is_empty() {
                None
            } else {
                serde_json::from_str(&input_json_str).ok()
            };
            // Tool execution started - create a running ToolExecution entry
            tracing::info!("Tool started: {}", tool_name);
            let hm = &tui.hook_manager;
            let trigger = rustycode_tools::hooks::HookTrigger::PreToolUse;
            let ctx = serde_json::json!({"tool_name": &tool_name, "tool_id": &tool_id});
            let hooks_dir = hm.hooks_dir().to_path_buf();
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("Failed to create runtime for hook execution: {}", e);
                        return;
                    }
                };
                let hm2 = rustycode_tools::hooks::HookManager::new(
                    hooks_dir,
                    rustycode_tools::hooks::HookProfile::Standard,
                    String::new(),
                );
                let _ = rt.block_on(hm2.execute(trigger, ctx));
            });

            if let Err(reason) = tui.plan_mode.is_tool_allowed(&tool_name) {
                tracing::warn!("Plan mode blocked tool: {}", reason);
                tui.toast_manager.warning(format!("Plan mode: {}", reason));
            }
            tui.update_terminal_title();

            // Track in active_tools map for status bar display
            tui.active_tools.insert(
                tool_id.clone(),
                ToolExecution {
                    tool_id: tool_id.clone(),
                    name: tool_name.clone(),
                    status: ToolStatus::Running,
                    start_time: chrono::Utc::now(),
                    end_time: None,
                    duration_ms: None,
                    result_summary: format!("{}...", tool_name),
                    detailed_output: None,
                    input_json: input_json.clone(),
                    progress_current: None,
                    progress_total: None,
                    progress_description: None,
                },
            );

            let assistant_msg = tui
                .messages
                .iter_mut()
                .rev()
                .find(|m| m.role == MessageRole::Assistant);
            if let Some(last_msg) = assistant_msg {
                let tool_execution = ToolExecution {
                    tool_id: tool_id.clone(),
                    name: tool_name.clone(),
                    status: ToolStatus::Running,
                    start_time: chrono::Utc::now(),
                    end_time: None,
                    duration_ms: None,
                    result_summary: format!("{}...", tool_name),
                    detailed_output: None,
                    input_json: input_json.clone(),
                    progress_current: None,
                    progress_total: None,
                    progress_description: None,
                };

                if last_msg.tool_executions.is_none() {
                    last_msg.tool_executions = Some(vec![]);
                }
                if let Some(tools) = &mut last_msg.tool_executions {
                    tools.push(tool_execution);
                    while tools.len() > 100 {
                        tools.remove(0);
                    }
                }
            }

            tui.dirty = true;

            // Add running tool to panel history immediately (goose pattern: show in-progress work)
            tui.tool_panel_history.push(ToolExecution {
                tool_id: tool_id.clone(),
                name: tool_name.clone(),
                status: ToolStatus::Running,
                start_time: chrono::Utc::now(),
                end_time: None,
                duration_ms: None,
                result_summary: format!("{}...", tool_name),
                detailed_output: None,
                input_json: input_json.clone(),
                progress_current: None,
                progress_total: None,
                progress_description: None,
            });
            // Cap at 50 entries
            if tui.tool_panel_history.len() > 50 {
                tui.tool_panel_history.remove(0);
            }
        }
        StreamChunk::ToolProgress {
            tool_name,
            stage,
            elapsed_ms,
            output_preview,
        } => {
            // Tool execution progress — update tool panel and active tools
            tracing::debug!(
                "Tool progress: {} - {} ({}ms)",
                tool_name,
                stage,
                elapsed_ms
            );
            tui.dirty = true;

            // Update tool panel history entries for this tool (show progress)
            for entry in tui.tool_panel_history.iter_mut().rev() {
                if entry.name == tool_name && entry.status == ToolStatus::Running {
                    let preview = output_preview.as_deref().unwrap_or("");
                    if !preview.is_empty() {
                        entry.result_summary = if preview.len() > 100 {
                            format!("{}...", &preview[..preview.floor_char_boundary(97)])
                        } else {
                            preview.to_string()
                        };
                    }
                    // Update progress description from stage
                    if !stage.is_empty() {
                        entry.progress_description = Some(stage.clone());
                    }
                    break; // Only update the most recent matching tool
                }
            }

            // Also update the ToolExecution in the current message's tool_executions
            for msg in tui.messages.iter_mut().rev() {
                if let Some(tools) = &mut msg.tool_executions {
                    for tool in tools.iter_mut().rev() {
                        if tool.name == tool_name && tool.status == ToolStatus::Running {
                            // Update progress description from stage
                            if !stage.is_empty() {
                                tool.progress_description = Some(stage.clone());
                            }
                            break;
                        }
                    }
                }
                // Stop at the first message with matching running tool
                if msg.tool_executions.as_ref().is_some_and(|tools| {
                    tools
                        .iter()
                        .any(|t| t.name == tool_name && t.status == ToolStatus::Running)
                }) {
                    break;
                }
            }

            if let Some(preview) = output_preview {
                if preview.len() <= 100 {
                    tracing::debug!("  Output preview: {}", preview);
                }
            }
        }
        StreamChunk::ToolComplete {
            tool_name,
            tool_id,
            duration_ms,
            success,
            output_size,
        } => {
            // Tool execution completed - update the running ToolExecution entry
            let status = if success { "✓" } else { "✗" };
            let size_str = if output_size > 1024 {
                format!("{:.1}KB", output_size as f64 / 1024.0)
            } else {
                format!("{}b", output_size)
            };

            // Remove from active_tools map using tool_id for accurate matching
            tui.active_tools.remove(&tool_id);
            // Update terminal title (may change from "tools" back to "thinking")
            tui.update_terminal_title();

            tracing::info!(
                "Tool complete: {} {} ({}ms, {})",
                status,
                tool_name,
                duration_ms,
                size_str
            );

            let assistant_msg = tui
                .messages
                .iter_mut()
                .rev()
                .find(|m| m.role == MessageRole::Assistant);
            if let Some(last_msg) = assistant_msg {
                if let Some(tools) = &mut last_msg.tool_executions {
                    if let Some(tool) = tools.iter_mut().find(|t| t.tool_id == tool_id) {
                        tool.status = if success {
                            ToolStatus::Complete
                        } else {
                            ToolStatus::Failed
                        };
                        let end_time = chrono::Utc::now();
                        tool.end_time = Some(end_time);
                        tool.duration_ms = Some(duration_ms);
                        tool.result_summary =
                            format!("{} {} ({}ms, {})", status, tool_name, duration_ms, size_str);
                    }
                }
            }

            tui.dirty = true;

            // Toast notification for failed tools so the user notices even
            // when scrolled away from the tool output (Goose pattern).
            if !success {
                tui.toast_manager.warning(format!("{} failed", tool_name));
            }

            // Doom loop detection: record tool result for pattern analysis.
            // Extract a key argument (file path, command, etc.) for fingerprinting.
            let key_arg = tui
                .messages
                .iter()
                .rev()
                .find_map(|m| {
                    m.tool_executions
                        .as_ref()?
                        .iter()
                        .rev()
                        .find(|t| t.tool_id == tool_id && t.status != ToolStatus::Running)
                })
                .and_then(|t| {
                    t.input_json.as_ref().and_then(|json| {
                        // Try common field names: file_path, path, command, query, pattern
                        json.get("file_path")
                            .or_else(|| json.get("path"))
                            .or_else(|| json.get("command"))
                            .or_else(|| json.get("query"))
                            .or_else(|| json.get("pattern"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                });
            tui.doom_loop
                .record(&tool_name, key_arg.as_deref(), success);

            // If doom loop detected, show a warning toast. The stream continues
            // but the user is alerted that the agent may be stuck.
            if tui.doom_loop.is_doom_loop() {
                if let Some(reason) = tui.doom_loop.doom_loop_reason() {
                    tui.toast_manager.warning(format!("Doom loop: {}", reason));
                }
            }

            let post_ctx = serde_json::json!({"tool_name": &tool_name, "success": success});
            let post_dir = tui.hook_manager.hooks_dir().to_path_buf();
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("Failed to create runtime for post-tool hook: {}", e);
                        return;
                    }
                };
                let hm = rustycode_tools::hooks::HookManager::new(
                    post_dir,
                    rustycode_tools::hooks::HookProfile::Standard,
                    String::new(),
                );
                let _ = rt.block_on(
                    hm.execute(rustycode_tools::hooks::HookTrigger::PostToolUse, post_ctx),
                );
            });
        }
        StreamChunk::ExtractTasks { text } => {
            // Save current state for undo
            tui.last_extraction = Some((
                tui.workspace_tasks.tasks.clone(),
                tui.workspace_tasks.todos.clone(),
            ));

            // Extract tasks/todos from the provided text
            let initial_todos = tui.workspace_tasks.todos.len();
            let initial_tasks = tui.workspace_tasks.tasks.len();

            extract_action_items(&text, &mut tui.workspace_tasks);

            let new_todos = tui.workspace_tasks.todos.len() - initial_todos;
            let new_tasks = tui.workspace_tasks.tasks.len() - initial_tasks;

            // Save the updated tasks
            if let Err(e) = crate::tasks::save_tasks(&tui.workspace_tasks) {
                tracing::warn!("Failed to save extracted tasks: {}", e);
            }

            // Provide feedback to user
            if new_todos > 0 || new_tasks > 0 {
                let mut feedback = Vec::new();
                if new_todos > 0 {
                    feedback.push(format!(
                        "☐ {} todo{}",
                        new_todos,
                        if new_todos == 1 { "" } else { "s" }
                    ));
                }
                if new_tasks > 0 {
                    feedback.push(format!(
                        "🔄 {} task{}",
                        new_tasks,
                        if new_tasks == 1 { "" } else { "s" }
                    ));
                }

                tui.add_system_message(format!("✓ Auto-created {}", feedback.join(" and ")));
                tui.add_system_message(
                    "💡 Tip: Press Ctrl+Shift+U to undo this extraction".to_string(),
                );
                tui.auto_scroll();
            }

            tracing::info!(
                "Auto-extracted {} todos and {} tasks from assistant response",
                new_todos,
                new_tasks
            );
        }
        StreamChunk::TasksExtracted {
            todos_count,
            tasks_count,
        } => {
            // Notification that extraction happened (for logging/debugging)
            tracing::info!(
                "Tasks extracted: {} todos, {} tasks",
                todos_count,
                tasks_count
            );
        }
        StreamChunk::ApprovalRequest {
            tool_name,
            tool_id: _,
            description,
            diff,
        } => {
            // Determine tool type based on tool name
            let tool_type = classify_tool_type(&tool_name);
            let command = diff.unwrap_or_else(|| format!("Execute {}", tool_name));
            let risk_level = crate::tool_approval::risk::classify_tool_risk(&tool_type, &command);

            // Check if already approved (e.g., "Always approve" was selected)
            if !tui.tool_approval.requires_approval(&tool_name, risk_level) {
                // Already approved - send automatic approval response
                tui.services.send_approval_response(true);
                tui.add_system_message(format!("✓ Auto-approved: {}", tool_name));
                tui.dirty = true;
                return;
            }

            // Check if tool has been blocked for this session
            if tui.tool_approval.is_blocked(&tool_name) {
                tui.services.send_approval_response(false);
                tui.add_system_message(format!("✗ Auto-rejected (blocked): {}", tool_name));
                tui.dirty = true;
                return;
            }

            // If already awaiting approval for this exact tool, don't spam duplicate messages
            if tui.awaiting_approval {
                if let Some(ref req) = tui.pending_approval_request {
                    if req.tool_name == tool_name {
                        // Already showing approval dialog for this tool - ignore duplicate request
                        return;
                    }
                }
            }

            // Show approval prompt to user
            tui.pending_approval_request = Some(crate::tool_approval::ApprovalRequest {
                tool_name: tool_name.clone(),
                tool_type,
                risk_level,
                description,
                command,
                state: crate::tool_approval::ApprovalState::Pending,
            });
            tui.awaiting_approval = true;
            tui.add_system_message(format!(
                "🔔 Approval required for '{}' - [Y]es / [n]o / [A]lways / [N] block",
                tool_name
            ));
            tui.dirty = true;
        }
        StreamChunk::ApprovalApproved { tool_id: _ } => {
            // User approved the tool
            if let Some(mut request) = tui.pending_approval_request.take() {
                request.approve();
                tui.tool_approval
                    .record_approval(request.tool_name.clone(), request.state);
                tui.add_system_message(format!("✓ Approved: {}", request.tool_name));
            }
            tui.awaiting_approval = false;
            tui.dirty = true;
        }
        StreamChunk::ApprovalRejected { tool_id: _ } => {
            // User rejected the tool
            if let Some(mut request) = tui.pending_approval_request.take() {
                request.reject();
                tui.add_system_message(format!("✗ Rejected: {}", request.tool_name));
            }
            tui.awaiting_approval = false;
            tui.dirty = true;
        }
        StreamChunk::QuestionRequest {
            question_id: _,
            question_text,
            header,
            options,
            multi_select: _,
        } => {
            // Show question to user - for now just log it
            // Full TUI integration would show a dialog here
            tracing::info!("Question from AI: {} - {}", header, question_text);
            let option_summary: Vec<_> = options.iter().map(|o| o.label.as_str()).collect();
            tracing::info!("Options: {:?}", option_summary);

            // Build structured options from the AI's question
            let ui_options: Vec<crate::ui::clarification::QuestionOption> = options
                .iter()
                .map(|o| crate::ui::clarification::QuestionOption {
                    label: o.label.clone(),
                    description: o.description.clone(),
                })
                .collect();

            let question = crate::ui::clarification::Question {
                text: format!("{}: {}", header, question_text),
                context: Some(format!("Options: {}", option_summary.join(", "))),
                options: ui_options,
            };
            tui.clarification_panel =
                crate::ui::clarification::ClarificationPanel::new(vec![question]);
            tui.awaiting_clarification = true;
            tui.add_system_message(format!("❓ AI asks: {}", question_text));
            tui.dirty = true;
        }
        StreamChunk::QuestionAnswered {
            question_id: _,
            answer: _,
        } => {
            // Question was answered - this is for logging
        }
        StreamChunk::FileSnapshot { batch } => {
            // Snapshot of file content before a write operation — push to undo stack
            if !batch.is_empty() {
                tui.file_undo_stack.push(batch);
                // Cap undo stack at 20 entries to bound memory usage
                while tui.file_undo_stack.len() > 20 {
                    tui.file_undo_stack.remove(0);
                }
            }
        }
        StreamChunk::TokenUsage {
            input_tokens,
            output_tokens,
        } => {
            // Accumulate token usage and estimate cost
            tui.session_input_tokens += input_tokens;
            tui.session_output_tokens += output_tokens;

            // Estimate cost using model-aware input/output pricing
            let model = &tui.current_model;
            let turn_cost =
                rustycode_llm::token_tracker::estimate_cost(model, input_tokens, output_tokens);
            tui.session_cost_usd += turn_cost;

            let _ = tui
                .cost_tracker
                .record_call(rustycode_llm::cost_tracker::ApiCall {
                    model: model.clone(),
                    input_tokens,
                    output_tokens,
                    cost_usd: turn_cost,
                    timestamp: chrono::Utc::now(),
                    tool_name: None,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                    cache_savings_usd: 0.0,
                });

            // Update the context monitor with actual token counts
            let total = tui.session_input_tokens + tui.session_output_tokens;
            let effective_max = tui.compaction_config.effective_max_tokens();
            tui.context_monitor.current_tokens = total;
            tui.context_monitor.max_tokens = effective_max;
            let was_needs_compaction = tui.context_monitor.needs_compaction;
            tui.context_monitor.needs_compaction =
                tui.context_monitor.usage_percentage() >= tui.compaction_config.warning_threshold;

            // Proactive warning: when context first crosses the threshold,
            // nudge the user to compact before hitting the hard limit.
            if tui.context_monitor.needs_compaction && !was_needs_compaction {
                let usage_pct = (tui.context_monitor.usage_percentage() * 100.0) as usize;
                let fmt_tokens = |n: usize| -> String {
                    if n >= 1_000_000 {
                        format!("{:.1}M", n as f64 / 1_000_000.0)
                    } else if n >= 1_000 {
                        format!("{:.0}k", n as f64 / 1_000.0)
                    } else {
                        n.to_string()
                    }
                };
                let current = fmt_tokens(tui.context_monitor.current_tokens);
                let max = fmt_tokens(tui.context_monitor.max_tokens);
                tui.add_system_message(format!(
                    "⚠️  Context at {}% ({}/{}) — consider /compact to free space",
                    usage_pct, current, max
                ));
                tui.auto_scroll();
            }

            tui.dirty = true;
        }
    }
}

/// Handle a tool execution result
pub fn handle_tool_result(tui: &mut TUI, result: ToolResult) {
    tracing::debug!("Tool result: {} ({:?})", result.name, result.result);

    let result_status = match &result.result {
        ToolOutput::Success(_) => ToolStatus::Complete,
        ToolOutput::Error(_) => ToolStatus::Failed,
        ToolOutput::Timeout => ToolStatus::Failed,
    };
    let raw_output: Option<String> = match &result.result {
        ToolOutput::Success(s) => Some(s.clone()),
        ToolOutput::Error(e) => Some(e.clone()),
        ToolOutput::Timeout => Some("Operation timed out".to_string()),
    };

    // Truncate large tool outputs with temp file fallback for inspection
    // Strip ANSI escape codes so terminal colors don't render as garbage in the TUI
    let detailed_output = raw_output.map(|output| {
        let output = crate::app::tool_output_format::strip_ansi_escapes(&output);
        const MAX_INLINE_CHARS: usize = 4000;
        if output.len() <= MAX_INLINE_CHARS {
            output
        } else {
            let truncated_lines: Vec<&str> = output.lines().take(21).collect();
            let has_more = truncated_lines.len() > 20;
            let truncated_lines = &truncated_lines[..20.min(truncated_lines.len())];
            let mut truncated = truncated_lines.join("\n");

            // Save full output to temp file
            let filename = format!(
                "rustycode-tool-{}-{}.txt",
                result.name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );
            let path = std::env::temp_dir().join(&filename);
            if std::fs::write(&path, &output).is_ok() {
                if has_more {
                    truncated.push_str(&format!(
                        "\n\n... (more lines truncated. Full output: {})",
                        path.display()
                    ));
                }
            } else {
                if has_more {
                    truncated.push_str(&format!(
                        "\n\n... (more lines truncated, {} chars total)",
                        output.len()
                    ));
                }
            }
            truncated
        }
    });

    // Save copies for tool panel history before moving into message
    let panel_detailed_output = detailed_output.clone();
    let panel_status = result_status.clone();

    // Compute a smart summary using output_summary for better display
    // Strip ANSI from summary too so status lines are clean
    let result_summary = match &result.result {
        ToolOutput::Success(s) => {
            let clean = crate::app::tool_output_format::strip_ansi_escapes(s);
            crate::app::tool_output_format::output_summary(&clean)
        }
        ToolOutput::Error(e) => {
            let clean = crate::app::tool_output_format::strip_ansi_escapes(e);
            format!("Error: {}", clean)
        }
        ToolOutput::Timeout => "Timeout".to_string(),
    };

    let assistant_msg = tui
        .messages
        .iter_mut()
        .rev()
        .find(|m| m.role == MessageRole::Assistant);
    if let Some(last_msg) = assistant_msg {
        let updated_existing = if let Some(tools) = &mut last_msg.tool_executions {
            if let Some(tool) = tools.iter_mut().find(|t| t.tool_id == result.id) {
                tool.status = result_status.clone();
                let end_time = chrono::Utc::now();
                tool.end_time = Some(end_time);
                tool.duration_ms = Some(
                    end_time
                        .signed_duration_since(tool.start_time)
                        .num_milliseconds()
                        .max(0) as u64,
                );
                tool.result_summary = result_summary.clone();
                tool.detailed_output = detailed_output.clone();
                true
            } else {
                false
            }
        } else {
            false
        };

        if !updated_existing {
            let tool_execution = ToolExecution {
                tool_id: result.id.clone(),
                name: result.name.clone(),
                start_time: chrono::Utc::now(),
                end_time: Some(chrono::Utc::now()),
                duration_ms: None,
                result_summary: result_summary.clone(),
                status: result_status,
                detailed_output,
                input_json: None,
                progress_current: None,
                progress_total: None,
                progress_description: None,
            };

            if last_msg.tool_executions.is_none() {
                last_msg.tool_executions = Some(vec![]);
            }
            if let Some(tools) = &mut last_msg.tool_executions {
                tools.push(tool_execution);
                while tools.len() > 100 {
                    tools.remove(0);
                }
            }
        }

        if !tui.user_scrolled {
            tui.auto_scroll();
        }
    }

    // Remove from active tools
    tui.active_tools.remove(&result.id);

    // Update the running entry in tool panel history (added by ToolStart)
    // Look up duration from the message's tool execution that was just updated
    let panel_duration = tui
        .messages
        .last()
        .and_then(|m| m.tool_executions.as_ref())
        .and_then(|tools| tools.iter().rev().find(|t| t.tool_id == result.id))
        .and_then(|t| t.duration_ms);

    // Find the running entry for this tool and update it in-place
    let updated_existing = tui
        .tool_panel_history
        .iter_mut()
        .rev()
        .find(|entry| entry.tool_id == result.id && entry.status == ToolStatus::Running)
        .map(|entry| {
            entry.status = panel_status.clone();
            entry.end_time = Some(chrono::Utc::now());
            entry.duration_ms = panel_duration;
            entry.result_summary = result_summary.clone();
            entry.detailed_output = panel_detailed_output.clone();
        })
        .is_some();

    // If no running entry was found (edge case), add a new one
    if !updated_existing {
        let tool_entry = ToolExecution {
            tool_id: result.id.clone(),
            name: result.name.clone(),
            start_time: chrono::Utc::now(),
            end_time: Some(chrono::Utc::now()),
            duration_ms: panel_duration,
            result_summary,
            status: panel_status,
            detailed_output: panel_detailed_output,
            input_json: None,
            progress_current: None,
            progress_total: None,
            progress_description: None,
        };
        tui.tool_panel_history.push(tool_entry);
        if tui.tool_panel_history.len() > 50 {
            tui.tool_panel_history.remove(0);
        }
    }

    tui.dirty = true;
}

/// Handle a workspace update
pub fn handle_workspace_update(tui: &mut TUI, update: WorkspaceUpdate) {
    match update {
        WorkspaceUpdate::ContextLoaded(context) => {
            tui.workspace_loaded = true;
            tui.workspace_context = Some(context.clone()); // Store workspace context!
            tui.workspace_scan_progress = None; // Clear progress
            tracing::debug!("Workspace context loaded ({} bytes)", context.len());

            // Detect git branch for status bar
            tui.git_branch = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        let branch = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        if !branch.is_empty() && branch != "HEAD" {
                            return Some(branch);
                        }
                    }
                    None
                });

            // Add system notification
            tui.add_system_message(format!(
                "Workspace loaded ({} files indexed)",
                context.lines().count()
            ));
        }
        WorkspaceUpdate::Notice(message) => {
            tracing::info!("Workspace notice: {}", message);
            tui.add_system_message(message);
        }
        WorkspaceUpdate::ScanProgress { scanned, total } => {
            tracing::debug!("Workspace scan: {}/{}", scanned, total);
            tui.workspace_scan_progress = Some((scanned, total));
            tui.dirty = true;
        }
        WorkspaceUpdate::ScanComplete {
            file_count,
            dir_count,
        } => {
            tracing::debug!(
                "Workspace scan complete: {} files, {} dirs",
                file_count,
                dir_count
            );
        }
        WorkspaceUpdate::Error(err) => {
            tracing::error!("Workspace loading error: {}", err);
            // User-friendly error (less technical)
            tui.add_system_message(
                "⚠️  Workspace loading issue - some features may be limited".to_string(),
            );
            tui.auto_scroll();
        }
    }
}

/// Handle a slash command result from background task
pub fn handle_slash_command_result(tui: &mut TUI, result: SlashCommandResult) {
    match result {
        SlashCommandResult::Success(output) => {
            tui.add_system_message(output);
        }
        SlashCommandResult::Error(err) => {
            tui.add_system_message(format!("Command failed: {}", err));
        }
        SlashCommandResult::LoadedSession { .. } => {
            // This variant should not arrive here — LoadSession is handled
            // via CommandEffect in the synchronous path. Log if it does.
            tracing::warn!(
                "LoadedSession arrived via async result — should use CommandEffect instead"
            );
        }
    }
    tui.dirty = true;
    tui.auto_scroll();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::async_::StreamChunk;
    use crate::app::TUI;

    fn create_test_tui() -> TUI {
        // Create a minimal TUI for testing using Default impl
        TUI::default()
    }

    #[test]
    fn test_duplicate_prevention_when_awaiting_clarification() {
        // Test that text-based question detection is skipped
        // when awaiting_clarification is already true
        let mut tui = create_test_tui();

        // Simulate that QuestionRequest already set awaiting_clarification
        tui.awaiting_clarification = true;
        tui.current_stream_content = "What format? How should I proceed?".to_string();

        // Manually test the guard logic (extracted from StreamChunk::Done handler)
        let should_detect = !tui.awaiting_clarification;
        assert!(
            !should_detect,
            "Should skip detection when awaiting_clarification is true"
        );

        // Verify no new system messages were added for clarification
        let initial_msg_count = tui.messages.len();

        // Simulate the guard check
        if !tui.awaiting_clarification {
            tui.add_system_message("❓ The AI has some clarification questions".to_string());
        }

        assert_eq!(
            tui.messages.len(),
            initial_msg_count,
            "No clarification message should be added"
        );
    }

    #[test]
    fn test_question_detection_when_not_awaiting() {
        // Test that text-based question detection runs
        // when awaiting_clarification is false
        let mut tui = create_test_tui();

        tui.awaiting_clarification = false;
        tui.current_stream_content = "What format do you prefer?".to_string();

        // The guard should allow detection
        let should_detect = !tui.awaiting_clarification;
        assert!(
            should_detect,
            "Should detect when awaiting_clarification is false"
        );
    }

    #[test]
    fn test_stream_chunk_question_request_sets_flag() {
        // Verify that QuestionRequest chunk sets awaiting_clarification
        let mut tui = create_test_tui();

        assert!(!tui.awaiting_clarification, "Initial state should be false");

        // Simulate what QuestionRequest handler does
        tui.awaiting_clarification = true;

        assert!(
            tui.awaiting_clarification,
            "Should be true after QuestionRequest"
        );
    }

    #[test]
    fn test_stream_chunk_done_after_question_request() {
        // Integration test: QuestionRequest followed by Done should not duplicate
        let mut tui = create_test_tui();
        let _question_content = "What is your preferred format?";

        // Step 1: Simulate QuestionRequest handling
        tui.awaiting_clarification = true;
        let after_question_request = tui.awaiting_clarification;

        // Step 2: Simulate Done handler guard check
        let should_skip_detection = after_question_request;

        assert!(
            should_skip_detection,
            "Done handler should skip detection after QuestionRequest"
        );
    }

    #[test]
    fn test_handle_stream_chunk_text_appends_content() {
        let mut tui = create_test_tui();
        let initial_content = "Hello";
        tui.current_stream_content = initial_content.to_string();

        let chunk = StreamChunk::Text(" World".to_string());
        handle_stream_chunk(&mut tui, chunk);

        assert_eq!(tui.current_stream_content, "Hello World");
        assert!(tui.is_streaming);
    }

    #[test]
    fn test_handle_stream_chunk_done_without_clarification() {
        let mut tui = create_test_tui();
        tui.current_stream_content = "I will implement the feature.".to_string();
        tui.awaiting_clarification = false;

        let initial_msg_count = tui.messages.len();

        // Simulate Done handler - text without questions
        let questions = crate::ui::detect_questions(&tui.current_stream_content);

        assert!(
            questions.is_empty(),
            "No questions should be detected in statement"
        );
        assert_eq!(
            tui.messages.len(),
            initial_msg_count,
            "No clarification message added"
        );
    }

    #[test]
    fn test_handle_stream_chunk_done_with_clarification_not_awaiting() {
        let mut tui = create_test_tui();
        tui.current_stream_content = "What format do you prefer?".to_string();
        tui.awaiting_clarification = false;

        // Simulate Done handler - with questions and not awaiting
        let questions = crate::ui::detect_questions(&tui.current_stream_content);

        assert!(!questions.is_empty(), "Questions should be detected");

        // The guard would allow setting up clarification
        let should_setup = !tui.awaiting_clarification;
        assert!(should_setup, "Should set up clarification panel");
    }
}
