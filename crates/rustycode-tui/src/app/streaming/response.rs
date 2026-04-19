//! Main LLM response streaming function
//!
//! This module contains the core `stream_llm_response` function that handles
//! the full conversation lifecycle including tool use detection, execution, and continuation.

use anyhow::{Context, Result};
use futures::StreamExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use crate::app::async_::StreamChunk;

/// RAII guard that guarantees `StreamChunk::Done` is sent when dropped.
///
/// The TUI relies on receiving `Done` to release the streaming guard and clear
/// `is_streaming`. Without this, early returns via `?` would leave the TUI in a
/// permanently stuck state. Double-sending `Done` is safe — the receiver ignores extras.
#[allow(dead_code)]
struct DoneGuard {
    stream_tx: SyncSender<StreamChunk>,
}

#[allow(dead_code)]
impl DoneGuard {
    fn new(stream_tx: SyncSender<StreamChunk>) -> Self {
        Self { stream_tx }
    }
}

impl Drop for DoneGuard {
    fn drop(&mut self) {
        let _ = self.stream_tx.send(StreamChunk::Done);
    }
}
use crate::task_extraction::extract_todos_from_tool_result;
use crate::{ErrorTracker, FileReadCache};
use rustycode_config::api_key_env_name;
use rustycode_llm::provider_v2::{ChatMessage, CompletionRequest, MessageRole};
use rustycode_protocol::{ContentBlock, MessageContent};
use secrecy::ExposeSecret;

use super::events::handle_sse_event;
use super::tool_detection::{
    handle_content_block_start, handle_message_delta, handle_partial_json,
};
use super::tool_execution::{execute_tool, snapshot_files_for_undo};
use super::{parse_tool_parameters, ActiveToolUse, ToolExecutionResult, ToolUseAction};

/// Configuration for streaming LLM responses
///
/// Builder pattern to handle the many parameters needed for `stream_llm_response`.
pub struct StreamConfig {
    pub content: String,
    pub cwd: std::path::PathBuf,
    pub stream_tx: SyncSender<StreamChunk>,
    pub workspace_context: Option<String>,
    pub stop_signal: Option<Arc<AtomicBool>>,
    pub tools_schema: Option<Vec<serde_json::Value>>,
    pub approval_rx: Option<std::sync::mpsc::Receiver<bool>>,
    pub question_rx: Option<std::sync::mpsc::Receiver<String>>,
    pub agent_mode: Option<crate::agent_mode::AgentMode>,
    pub file_read_cache: Option<Arc<StdMutex<FileReadCache>>>,
    pub error_tracker: Option<Arc<StdMutex<ErrorTracker>>>,
    pub todo_state: Option<rustycode_tools::TodoState>,
    pub conversation_history: Option<Vec<ChatMessage>>,
    pub tool_registry: Option<Arc<rustycode_tools::ToolRegistry>>,
}

impl StreamConfig {
    /// Create a new config with required parameters
    pub fn new(content: &str, cwd: &Path, stream_tx: SyncSender<StreamChunk>) -> Self {
        Self {
            content: content.to_string(),
            cwd: cwd.to_path_buf(),
            stream_tx,
            workspace_context: None,
            stop_signal: None,
            tools_schema: None,
            approval_rx: None,
            question_rx: None,
            agent_mode: None,
            file_read_cache: None,
            error_tracker: None,
            todo_state: None,
            conversation_history: None,
            tool_registry: None,
        }
    }

    pub fn workspace_context_opt(mut self, ctx: Option<String>) -> Self {
        self.workspace_context = ctx;
        self
    }

    pub fn stop_signal_opt(mut self, signal: Option<Arc<AtomicBool>>) -> Self {
        self.stop_signal = signal;
        self
    }

    pub fn tools_schema_opt(mut self, schema: Option<Vec<serde_json::Value>>) -> Self {
        self.tools_schema = schema;
        self
    }

    pub fn approval_rx_opt(mut self, rx: Option<std::sync::mpsc::Receiver<bool>>) -> Self {
        self.approval_rx = rx;
        self
    }

    pub fn question_rx_opt(mut self, rx: Option<std::sync::mpsc::Receiver<String>>) -> Self {
        self.question_rx = rx;
        self
    }

    pub fn agent_mode_opt(mut self, mode: Option<crate::agent_mode::AgentMode>) -> Self {
        self.agent_mode = mode;
        self
    }

    pub fn file_read_cache_opt(mut self, cache: Option<Arc<StdMutex<FileReadCache>>>) -> Self {
        self.file_read_cache = cache;
        self
    }

    pub fn error_tracker_opt(mut self, tracker: Option<Arc<StdMutex<ErrorTracker>>>) -> Self {
        self.error_tracker = tracker;
        self
    }

    pub fn todo_state_opt(mut self, state: Option<rustycode_tools::TodoState>) -> Self {
        self.todo_state = state;
        self
    }

    pub fn conversation_history_opt(mut self, history: Option<Vec<ChatMessage>>) -> Self {
        self.conversation_history = history;
        self
    }

    pub fn tool_registry_opt(
        mut self,
        registry: Option<Arc<rustycode_tools::ToolRegistry>>,
    ) -> Self {
        self.tool_registry = registry;
        self
    }
}

/// Fix conversation structure before sending to LLM.
///
/// Ensures messages alternate properly and removes problematic patterns
/// that would cause API errors with providers like Anthropic/Claude.
///
/// Important: preserves tool_use/tool_result ordering. Anthropic requires
/// assistant messages with tool_use blocks to be immediately followed by
/// user messages with tool_result content.
fn fix_conversation_messages(messages: &mut Vec<ChatMessage>) {
    use rustycode_llm::provider_v2::MessageRole;

    // Remove leading non-system/non-user messages
    while messages
        .first()
        .is_some_and(|m| !matches!(m.role, MessageRole::System | MessageRole::User))
    {
        messages.remove(0);
    }

    // Remove leading orphaned tool_result messages whose parent assistant+tool_use
    // was lost (e.g., from message cap/pruning). These are user-role messages with
    // Simple content containing "type":"tool_result" JSON. Without their parent
    // assistant message, the API will reject them.
    while let Some(msg) = messages.first() {
        if msg.role != MessageRole::User {
            break;
        }
        let text = msg.content.as_text();
        if text.contains("\"type\":\"tool_result\"") {
            tracing::debug!("fix_conversation_messages: dropping orphaned tool_result");
            messages.remove(0);
        } else {
            break;
        }
    }

    // Remove trailing assistant messages only if they DON'T contain tool_use.
    // Tool-use assistant messages must be kept because they're followed by
    // tool_result user messages in the same turn.
    while messages.last().is_some_and(|m| {
        if m.role != MessageRole::Assistant {
            return false;
        }
        // Keep assistant messages that have tool_use blocks (Blocks content)
        !matches!(&m.content, MessageContent::Blocks(blocks) if blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { .. })))
    }) {
        messages.pop();
    }

    // Merge consecutive same-role messages (except system and tool-related).
    // Tool_use/tool_result messages must NOT be merged — they have specific
    // ordering requirements from the API.
    let mut i = 1;
    while i < messages.len() {
        // Don't merge if either message involves tool blocks
        let prev_has_tools = matches!(&messages[i - 1].content, MessageContent::Blocks(blocks) if blocks.iter().any(|b| b.is_tool_use()));
        let curr_has_tools = matches!(&messages[i].content, MessageContent::Blocks(blocks) if blocks.iter().any(|b| b.is_tool_use()));

        if prev_has_tools || curr_has_tools {
            i += 1;
            continue;
        }

        if messages[i].role == messages[i - 1].role
            && !matches!(messages[i].role, MessageRole::System)
        {
            // Merge content
            let merged_content = match (&messages[i - 1].content, &messages[i].content) {
                (MessageContent::Simple(a), MessageContent::Simple(b)) => {
                    MessageContent::Simple(format!("{}\n{}", a, b))
                }
                _ => messages[i].content.clone(),
            };
            messages[i - 1].content = merged_content;
            messages.remove(i);
        } else {
            i += 1;
        }
    }

    // Ensure we have at least a user message
    if messages.is_empty() {
        messages.push(ChatMessage::user("(conversation continued)".to_string()));
    }
}

/// Stream an LLM response and send chunks to the TUI
///
/// This is the main entry point for streaming LLM conversations with tool support.
/// It handles the full lifecycle: loading provider config, streaming responses,
/// detecting and executing tool calls, and continuing conversations with tool results.
///
/// # Arguments
///
/// * `config` - Stream configuration containing all parameters
///
/// # Returns
///
/// Returns `Ok(())` on successful completion, or an error if setup fails.
/// Note that stream errors are sent via the channel rather than returned.
#[allow(clippy::too_many_arguments)]
pub async fn stream_llm_response(config: StreamConfig) -> Result<()> {
    let StreamConfig {
        content,
        cwd,
        stream_tx,
        workspace_context,
        stop_signal,
        tools_schema,
        approval_rx,
        question_rx,
        agent_mode,
        file_read_cache,
        error_tracker,
        todo_state,
        conversation_history,
        tool_registry,
    } = config;

    let _done_guard = DoneGuard::new(stream_tx.clone());

    // Load provider type and model from config
    let (provider_type, model, v2_config) =
        rustycode_llm::load_provider_config_from_env().context("Failed to load provider config")?;

    tracing::debug!("Using provider: {} with model: {}", provider_type, model);
    tracing::debug!("API Key configured: {}", v2_config.api_key.is_some());
    tracing::debug!("Base URL: {:?}", v2_config.base_url);

    // Validate API key (skip for providers that don't require one)
    let needs_api_key = !matches!(
        provider_type.to_lowercase().as_str(),
        "ollama" | "local" | "lmstudio" | "litert-lm" | "litert_lm" | "litert"
    );
    if needs_api_key && v2_config.api_key.is_none() {
        let _ = stream_tx.send(StreamChunk::Error(
            format!("No API key configured for provider '{}'. Please set the {} environment variable or add it to your config.json.",
                provider_type,
                api_key_env_name(&provider_type))
        ));
        // Must send Done so the TUI releases the query guard and clears is_streaming.
        // Without this, the guard stays active and blocks all future messages.
        let _ = stream_tx.send(StreamChunk::Done);
        return Ok(());
    }

    // Validate Anthropic API key format (accept both old "sk-ant-" and new format)
    if provider_type.to_lowercase() == "anthropic" || provider_type.to_lowercase() == "claude" {
        let api_key = match v2_config.api_key.as_ref() {
            Some(key) => key,
            None => {
                let _ = stream_tx.send(StreamChunk::Error(
                    "No API key configured. Please set up your API key in settings.".to_string(),
                ));
                let _ = stream_tx.send(StreamChunk::Done);
                return Ok(());
            }
        };
        let key_str = api_key.expose_secret();
        if key_str.len() < 20 {
            let _ = stream_tx.send(StreamChunk::Error(
                format!("Invalid Anthropic API key format. API key appears too short ({} chars). Expected at least 20 characters.",
                    key_str.len())
            ));
            let _ = stream_tx.send(StreamChunk::Done);
            return Ok(());
        }
    }

    // Create provider
    let provider = if v2_config.api_key.is_some() {
        rustycode_llm::create_provider_with_config(&provider_type, &model, v2_config).context(
            format!(
                "Failed to create provider {} with model {}",
                provider_type, model
            ),
        )?
    } else {
        rustycode_llm::create_provider_v2(&provider_type, &model).context(format!(
            "Failed to create provider {} with model {}",
            provider_type, model
        ))?
    };

    // Build initial messages array with optional workspace context
    let mut messages = Vec::new();

    // Build system message with workspace context, coding guidance, and agent mode
    let workspace_section = if let Some(context) = workspace_context {
        format!("## Project\n{}", context)
    } else {
        "No workspace context available.".to_string()
    };

    let mut system_parts = vec![
        "You are RustyCode, an AI coding assistant. Be concise, decisive, and correct.\n\
        \n\
        - Read files before modifying them\n\
        - Make targeted changes, not broad refactors\n\
        - Run tests to verify your changes\n\
        - Reference code as file_path:line_number\n\
        - Use parallel tool calls when operations are independent"
            .to_string(),
        workspace_section,
        format!(
            "Platform: {} | Date: {}",
            std::env::consts::OS,
            chrono::Utc::now().format("%Y-%m-%d")
        ),
        "Planning mode policy:\n\
        - If a requested action is blocked by planning mode, say you are stalled, name the blocker briefly, and ask the user to switch to implementation mode with /plan.\n\
        - If a required instruction file is missing or empty, say so explicitly and stop.\n\
        - If planning appears complete, say you are ready to switch to implementation mode and wait for the user's confirmation.\n\
        - Do not silently stop after a blocker; explain the next step.".to_string(),
    ];

    if let Some(mode) = agent_mode {
        system_parts.push(mode.system_prompt_suffix().to_string());
    }

    // Load custom system prompt additions (Goose pattern: RUSTYCODE_SYSTEM_PROMPT_FILE)
    // Supports both a file path and inline text via RUSTYCODE_SYSTEM_PROMPT
    if let Ok(custom_prompt) = std::env::var("RUSTYCODE_SYSTEM_PROMPT") {
        if !custom_prompt.is_empty() {
            system_parts.push(custom_prompt);
        }
    } else if let Ok(prompt_file) = std::env::var("RUSTYCODE_SYSTEM_PROMPT_FILE") {
        if !prompt_file.is_empty() {
            match std::fs::read_to_string(&prompt_file) {
                Ok(content) if !content.trim().is_empty() => {
                    system_parts.push(content);
                    tracing::info!("Loaded custom system prompt from {}", prompt_file);
                }
                Ok(_) => {} // Empty file, skip
                Err(e) => {
                    tracing::warn!("Failed to read system prompt file {}: {}", prompt_file, e);
                }
            }
        }
    }
    // Also load .rustycode_system_prompt from project root (Goose hints pattern)
    if let Some(cwd_str) = cwd.to_str() {
        let project_prompt = std::path::Path::new(cwd_str).join(".rustycode_system_prompt");
        if project_prompt.exists() {
            if let Ok(content) = std::fs::read_to_string(&project_prompt) {
                if !content.trim().is_empty() {
                    system_parts.push(content);
                }
            }
        }
        // Also load AGENTS.md (Goose/Claw pattern for project-level AI instructions)
        let agents_md = std::path::Path::new(cwd_str).join("AGENTS.md");
        if agents_md.exists() {
            if let Ok(content) = std::fs::read_to_string(&agents_md) {
                if !content.trim().is_empty() {
                    system_parts.push(format!("## Project Instructions (AGENTS.md)\n{}", content));
                }
            }
        }
    }

    let system_message = system_parts.join("\n\n");

    messages.push(ChatMessage::system(system_message));

    // Include conversation history for multi-turn context
    let mut history_included_user_msg = false;
    if let Some(history) = conversation_history {
        // Skip system messages from history (we have our own)
        for msg in history {
            if !matches!(msg.role, MessageRole::System) {
                messages.push(msg);
            }
        }
        // Check if the last message in history is already this user message
        // (happens when caller pushes user msg to messages before building history)
        // For image messages, as_str() returns text + "[Image]" placeholders,
        // so we check if the text content starts with our message text.
        history_included_user_msg = messages.last().is_some_and(|m| {
            if m.role != MessageRole::User {
                return false;
            }
            let msg_text = m.content.as_str();
            // Exact match (text-only messages)
            if msg_text == content {
                return true;
            }
            // Image messages: text starts with content, followed by "[Image]" blocks
            msg_text.starts_with(&content[..])
        });
    }

    // Add the current user message only if not already in history
    if !history_included_user_msg {
        messages.push(ChatMessage::user(content.to_string()));
    }

    // Conversation limits to prevent unbounded memory growth
    const MAX_MESSAGES: usize = 50;
    const MAX_CONVERSATION_BYTES: usize = 10 * 1024 * 1024; // 10MB
    const MAX_TOOL_TURNS: usize = 50; // Prevent infinite tool-use loops

    // Conversation-level truncation for tool results.
    // Individual tools truncate at 30-80 lines / 10-50KB, but MCP tools, custom tools,
    // or critical content bypasses may produce larger output. This safety net ensures
    // no single tool result exceeds a reasonable size for the LLM context window.
    const TOOL_RESULT_MAX_BYTES: usize = 25 * 1024; // 25KB per result (generous but bounded)
    let truncate_for_conversation = |content: String| -> String {
        if content.len() <= TOOL_RESULT_MAX_BYTES {
            return content;
        }
        // Find a safe char boundary at or before the byte limit
        let bytes = content.as_bytes();
        let mut end = TOOL_RESULT_MAX_BYTES;
        // Walk back to find a clean line break (also ensures UTF-8 safety)
        while end > 0 && bytes[end] != b'\n' {
            end -= 1;
        }
        if end == 0 {
            // No line break found — find nearest char boundary at the limit
            end = TOOL_RESULT_MAX_BYTES;
            while end > 0 && !content.is_char_boundary(end) {
                end -= 1;
            }
        }
        // end is guaranteed to be a valid char boundary (either at \n or at char_boundary)
        let kept = &content[..end];
        let omitted_bytes = content.len() - end;
        format!(
            "{}\n\n[... {} bytes omitted — output truncated for context window]",
            kept.trim_end(),
            omitted_bytes
        )
    };

    // Helper function to prune messages when limits are exceeded
    let prune_messages = |msgs: &mut Vec<ChatMessage>| {
        if msgs.len() <= MAX_MESSAGES {
            return;
        }

        let total_size: usize = msgs.iter().map(|m| m.content.as_text().len()).sum();

        if total_size > MAX_CONVERSATION_BYTES || msgs.len() > MAX_MESSAGES {
            let system_messages: Vec<_> = msgs
                .iter()
                .filter(|m| matches!(m.role, rustycode_llm::MessageRole::System))
                .cloned()
                .collect();

            let keep_count = MAX_MESSAGES.saturating_sub(system_messages.len());
            let start_idx = msgs.len().saturating_sub(keep_count);
            let original_len = msgs.len();

            let mut recent_messages = msgs.split_off(start_idx);

            // Drop leading orphaned tool_result messages whose parent
            // assistant+tool_use was cut by the split.
            while let Some(msg) = recent_messages.first() {
                if msg.role != MessageRole::User {
                    break;
                }
                if msg.content.as_text().contains("\"type\":\"tool_result\"") {
                    tracing::debug!(
                        "Prune: dropping orphaned tool_result (parent tool_use was cut)"
                    );
                    recent_messages.remove(0);
                } else {
                    break;
                }
            }

            tracing::info!(
                "Pruned conversation from {} to {} messages",
                original_len,
                recent_messages.len() + system_messages.len()
            );

            let mut final_messages = system_messages;
            final_messages.append(&mut recent_messages);
            *msgs = final_messages;
        }
    };

    // Conversation continuation loop
    let mut turn_count: usize = 0;
    loop {
        turn_count += 1;
        if turn_count > MAX_TOOL_TURNS {
            tracing::warn!("Reached max tool turns ({}), breaking loop", MAX_TOOL_TURNS);
            let _ = stream_tx.send(StreamChunk::Error(format!(
                "Reached maximum tool-use turns ({}). Stopping to prevent infinite loop.",
                MAX_TOOL_TURNS
            )));
            break;
        }

        // Fix conversation structure before sending to LLM
        fix_conversation_messages(&mut messages);

        // Create request with current messages
        let mut request = CompletionRequest::new(model.clone(), messages.clone())
            .with_streaming(true)
            .with_max_tokens(8192)
            .with_temperature(0.1);

        // Add tools schema if provided (only on first request)
        if let Some(ref tools) = tools_schema {
            request = request.with_tools(tools.clone());
            tracing::info!("Including {} tool definitions in request", tools.len());
        } else {
            tracing::warn!("No tools schema provided - tool use will not be available!");
        }

        // Stream the response
        let mut stream = provider.complete_stream(request).await.map_err(|e| {
            let msg = format!("{}", e);
            if provider_type.to_lowercase() == "ollama" {
                anyhow::anyhow!(
                    "Cannot connect to Ollama. Is it running? Start with: ollama serve\nDetails: {}",
                    msg
                )
            } else if msg.contains("connection refused") || msg.contains("Connection refused") {
                anyhow::anyhow!(
                    "Connection refused by provider '{}'. Check that the service is running and the base URL is correct.\nDetails: {}",
                    provider_type, msg
                )
            } else {
                anyhow::anyhow!("Failed to start stream: {}", msg)
            }
        })?;

        // Track active tool use accumulation
        let mut active_tool: Option<ActiveToolUse> = None;
        let mut in_tool_use = false;
        let mut tool_executions: Vec<ToolExecutionResult> = Vec::new();
        let mut assistant_response = String::new();
        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut thinking_content = String::new();
        let mut thinking_signature = String::new();
        let mut stop_action = ToolUseAction::None;

        // Process stream events with timeout to prevent indefinite hangs
        loop {
            // Wrap each stream.next() call in a 120s timeout
            let chunk_result = match tokio::time::timeout(
                std::time::Duration::from_secs(120),
                stream.next(),
            )
            .await
            {
                Ok(Some(result)) => result,
                Ok(None) => break, // stream ended normally
                Err(_) => {
                    // Timeout — no chunk received in 120s
                    tracing::warn!("Stream timed out after 120s with no data");
                    let _ = stream_tx.send(StreamChunk::Error(
                        "Stream timed out (120s without data). The provider may be overloaded."
                            .into(),
                    ));
                    let _ = stream_tx.send(StreamChunk::Done);
                    break;
                }
            };
            // Check for cancellation signal
            if stop_signal
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
            {
                let _ = stream_tx.send(StreamChunk::Done);
                return Ok(());
            }

            match chunk_result {
                Ok(event) => {
                    use rustycode_llm::SSEEvent;
                    match &event {
                        SSEEvent::Text { text }
                        | SSEEvent::ContentBlockDelta {
                            delta: rustycode_llm::ContentDelta::Text { text },
                            ..
                        } => {
                            if !in_tool_use {
                                assistant_response.push_str(text);
                            }
                            let _ = crate::app::streaming::events::handle_text_event(
                                text.clone(),
                                &mut in_tool_use,
                                &stream_tx,
                            );
                        }
                        SSEEvent::ThinkingDelta { thinking }
                        | SSEEvent::ContentBlockDelta {
                            delta: rustycode_llm::ContentDelta::Thinking { thinking },
                            ..
                        } => {
                            thinking_content.push_str(thinking);
                            let _ = crate::app::streaming::events::handle_thinking_event(
                                thinking.clone(),
                                &in_tool_use,
                                &stream_tx,
                            );
                        }
                        SSEEvent::SignatureDelta { signature } => {
                            thinking_signature.push_str(signature);
                        }
                        SSEEvent::ContentBlockStart { content_block, .. } => {
                            handle_content_block_start(
                                content_block.clone(),
                                &mut in_tool_use,
                                &mut active_tool,
                            );
                        }
                        SSEEvent::ContentBlockDelta {
                            delta: rustycode_llm::ContentDelta::PartialJson { partial_json },
                            ..
                        } => {
                            handle_partial_json(partial_json.clone(), &mut active_tool);
                        }
                        SSEEvent::ContentBlockStop { .. } => {
                            // End of content block - tool use flag will be reset after tool execution
                            if let Some(tool) = active_tool.take() {
                                // Check for stop signal before executing tool
                                if stop_signal
                                    .as_ref()
                                    .is_some_and(|flag| flag.load(Ordering::Relaxed))
                                {
                                    tracing::info!("Tool execution cancelled by user");
                                    let _ = stream_tx.send(StreamChunk::Text(
                                        "\n[Tool execution cancelled]\n".to_string(),
                                    ));
                                    let _ = stream_tx.send(StreamChunk::Done);
                                    return Ok(());
                                }

                                tracing::info!(
                                    "Tool use complete: {} ({}), executing...",
                                    tool.name,
                                    tool.id
                                );

                                let needs_approval = true;

                                let should_execute = if needs_approval && approval_rx.is_some() {
                                    let command = format!("{}: {}", tool.name, {
                                        let v = parse_tool_parameters(&tool.partial_json);
                                        if let Some(obj) = v.as_object() {
                                            obj.iter()
                                                .take(2)
                                                .map(|(k, v)| format!("{}={}", k, v))
                                                .collect::<Vec<_>>()
                                                .join(" ")
                                        } else {
                                            tool.partial_json.clone()
                                        }
                                    });

                                    let _ = stream_tx.send(StreamChunk::ApprovalRequest {
                                        tool_name: tool.name.clone(),
                                        tool_id: tool.id.clone(),
                                        description: format!("Execute tool: {}", tool.name),
                                        diff: Some(command),
                                    });

                                    let rx = match approval_rx.as_ref() {
                                        Some(rx) => rx,
                                        None => {
                                            let _ = stream_tx.send(StreamChunk::Error(
                                                "Error: approval channel not available".to_string(),
                                            ));
                                            // Must provide tool_result to avoid breaking conversation structure
                                            tool_executions.push(ToolExecutionResult {
                                                tool_use_id: tool.id.clone(),
                                                tool_name: tool.name.clone(),
                                                result_content:
                                                    "Error: approval channel not available"
                                                        .to_string(),
                                            });
                                            in_tool_use = false;
                                            continue;
                                        }
                                    };
                                    match rx.recv_timeout(Duration::from_secs(30)) {
                                        Ok(true) => {
                                            let _ = stream_tx.send(StreamChunk::ApprovalApproved {
                                                tool_id: tool.id.clone(),
                                            });
                                            true
                                        }
                                        Ok(false) | Err(_) => {
                                            let _ = stream_tx.send(StreamChunk::ApprovalRejected {
                                                tool_id: tool.id.clone(),
                                            });
                                            let _ = stream_tx.send(StreamChunk::Text(
                                                "[Tool execution rejected by user]\n".to_string(),
                                            ));
                                            false
                                        }
                                    }
                                } else {
                                    true
                                };

                                if !should_execute {
                                    // Must provide tool_result to avoid breaking conversation structure
                                    tool_executions.push(ToolExecutionResult {
                                        tool_use_id: tool.id.clone(),
                                        tool_name: tool.name.clone(),
                                        result_content: "[Tool execution rejected by user]"
                                            .to_string(),
                                    });
                                    in_tool_use = false;
                                    continue;
                                }

                                // Handle "question" tool specially
                                if tool.name == "question" && question_rx.is_some() {
                                    let params = parse_tool_parameters(&tool.partial_json);
                                    let question_text = params
                                        .get("question")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Please answer");
                                    let options = params
                                        .get("options")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|v| v.as_str().map(String::from))
                                                .collect::<Vec<_>>()
                                        })
                                        .unwrap_or_default();
                                    let default = params
                                        .get("default")
                                        .and_then(|v| v.as_str())
                                        .map(String::from);
                                    let multi_select = params
                                        .get("multiple")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);

                                    let question_options: Vec<crate::app::async_::QuestionOption> =
                                        options
                                            .iter()
                                            .map(|opt| crate::app::async_::QuestionOption {
                                                label: opt.clone(),
                                                description: String::new(),
                                            })
                                            .collect();

                                    let _ = stream_tx.send(StreamChunk::QuestionRequest {
                                        question_id: tool.id.clone(),
                                        question_text: question_text.to_string(),
                                        header: "Question".to_string(),
                                        options: question_options,
                                        multi_select,
                                    });

                                    let rx = match question_rx.as_ref() {
                                        Some(rx) => rx,
                                        None => {
                                            let _ = stream_tx.send(StreamChunk::Error(
                                                "Error: question channel not available".to_string(),
                                            ));
                                            // Must provide tool_result to avoid breaking conversation structure
                                            tool_executions.push(ToolExecutionResult {
                                                tool_use_id: tool.id.clone(),
                                                tool_name: tool.name.clone(),
                                                result_content:
                                                    "Error: question channel not available"
                                                        .to_string(),
                                            });
                                            in_tool_use = false;
                                            continue;
                                        }
                                    };

                                    let answer = match rx.recv_timeout(Duration::from_secs(120)) {
                                        Ok(a) => a,
                                        Err(_) => {
                                            if let Some(def) = default {
                                                def
                                            } else {
                                                // Question timed out with no default — provide tool_result
                                                tool_executions.push(ToolExecutionResult {
                                                    tool_use_id: tool.id.clone(),
                                                    tool_name: tool.name.clone(),
                                                    result_content: "Error: question timed out with no default answer".to_string(),
                                                });
                                                in_tool_use = false;
                                                continue;
                                            }
                                        }
                                    };

                                    let _ = stream_tx.send(StreamChunk::QuestionAnswered {
                                        question_id: tool.id.clone(),
                                        answer: answer.clone(),
                                    });

                                    let result_content = format!(
                                        "**Question:** {}\n\n**Your response:** {}",
                                        question_text, answer
                                    );

                                    // Don't send "[Question answered: ...]" as text noise.
                                    // The QuestionAnswered chunk is sufficient for the TUI to track state.

                                    tool_executions.push(ToolExecutionResult {
                                        tool_use_id: tool.id.clone(),
                                        tool_name: tool.name.clone(),
                                        result_content,
                                    });

                                    in_tool_use = false;
                                    continue;
                                }

                                let _ = stream_tx.send(StreamChunk::ToolStart {
                                    tool_name: tool.name.clone(),
                                    tool_id: tool.id.clone(),
                                    input_json: tool.partial_json.clone(),
                                });

                                let tool_start = std::time::Instant::now();

                                // Snapshot file content before write operations for /undo
                                if let Some(batch) =
                                    snapshot_files_for_undo(&cwd, &tool.name, &tool.partial_json)
                                {
                                    let _ = stream_tx.send(StreamChunk::FileSnapshot { batch });
                                }

                                let result = execute_tool(
                                    &cwd,
                                    &tool.name,
                                    &tool.partial_json,
                                    file_read_cache.as_ref(),
                                    error_tracker.as_ref(),
                                    todo_state.as_ref(),
                                    tool_registry.as_ref(),
                                );
                                let tool_elapsed = tool_start.elapsed().as_millis() as u64;

                                tool_executions.push(ToolExecutionResult {
                                    tool_use_id: tool.id.clone(),
                                    tool_name: tool.name.clone(),
                                    result_content: result.clone(),
                                });

                                let _ = stream_tx.send(StreamChunk::ToolComplete {
                                    tool_name: tool.name.clone(),
                                    tool_id: tool.id.clone(),
                                    duration_ms: tool_elapsed,
                                    success: !result.starts_with("Error"),
                                    output_size: result.len(),
                                });

                                let tool_todos =
                                    extract_todos_from_tool_result(&tool.name, &result);
                                if !tool_todos.is_empty() {
                                    // ExtractTasks handles task extraction silently
                                    // No text noise - the task dashboard shows extraction results
                                    let _ = stream_tx.send(StreamChunk::ExtractTasks {
                                        text: result.clone(),
                                    });
                                }
                            }
                            in_tool_use = false;
                        }
                        SSEEvent::MessageDelta { stop_reason, usage } => {
                            if let Some(usage_info) = usage {
                                tracing::debug!(
                                    "Usage: {} in, {} out",
                                    usage_info.input_tokens,
                                    usage_info.output_tokens
                                );
                                let _ = stream_tx.send(StreamChunk::TokenUsage {
                                    input_tokens: usage_info.input_tokens as usize,
                                    output_tokens: usage_info.output_tokens as usize,
                                });
                            }
                            stop_action = handle_message_delta(stop_reason.as_deref());
                        }
                        SSEEvent::MessageStop => {
                            if !assistant_response.is_empty() {
                                let _ = stream_tx.send(StreamChunk::ExtractTasks {
                                    text: assistant_response.clone(),
                                });
                            }
                            // DO NOT send StreamChunk::Done here!
                            // During tool-use turns (stop_reason="tool_use"), the outer
                            // loop will continue with another LLM request. Sending Done
                            // prematurely releases the query guard and can trigger
                            // auto-continue or allow concurrent streams.
                            // StreamChunk::Done is sent only at the end of the outer loop
                            // (see the final `stream_tx.send(StreamChunk::Done)` at the
                            // bottom of the function) or on error/timeout/cancellation.
                        }
                        _ => {
                            if !handle_sse_event(
                                event,
                                &mut in_tool_use,
                                &mut active_tool,
                                &stream_tx,
                            )? {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    // Typed error matching using ProviderError variants,
                    // with fallback to string matching for wrapped errors.
                    use rustycode_llm::provider_v2::ProviderError;
                    let enhanced = match &e {
                        ProviderError::RateLimited { retry_delay, .. } => {
                            let wait_hint = retry_delay
                                .map(|d| format!(" (retry after {}s)", d.as_secs()))
                                .unwrap_or_default();
                            format!(
                                "Rate limited by provider '{}'.{wait_hint}\n\
                                Consider using a different model or provider if this persists.\n\
                                Details: {}",
                                provider_type, e
                            )
                        }
                        ProviderError::CreditsExhausted { top_up_url, .. } => {
                            let top_up = top_up_url
                                .as_ref()
                                .map(|url| format!("\nTop up credits: {}", url))
                                .unwrap_or_default();
                            format!(
                                "API credits exhausted for provider '{}'.{top_up}\n\
                                Details: {}",
                                provider_type, e
                            )
                        }
                        ProviderError::ContextLengthExceeded(_) => {
                            format!(
                                "Context length exceeded for model '{}'. Try:\n\
                                - Start a new conversation (/clear)\n\
                                - Use a model with larger context\n\
                                Details: {}",
                                model, e
                            )
                        }
                        ProviderError::InvalidModel(_) => {
                            format!(
                                "Model '{}' not found. Check the model name is correct.\n\
                                Try: claude-sonnet-4-6, claude-opus-4-6, or claude-haiku-4-5\n\
                                Details: {}",
                                model, e
                            )
                        }
                        ProviderError::Auth(_) => {
                            format!("Authentication failed for provider '{}'. Your API key may be invalid or expired.\n\
                                Set the correct key in your config or environment variable.\n\
                                Details: {}", provider_type, e)
                        }
                        ProviderError::Network(_) => {
                            format!("Network error connecting to provider '{}'. Check your internet connection.\n\
                                Please retry if you think this is a transient error.\n\
                                Details: {}", provider_type, e)
                        }
                        ProviderError::Timeout(_) => {
                            format!("Request timed out for provider '{}'. The server may be overloaded.\n\
                                Please retry if you think this is a transient error.\n\
                                Details: {}", provider_type, e)
                        }
                        _ => {
                            // Fallback to string matching for wrapped/unknown errors
                            let error_msg = format!("{}", e);
                            if error_msg.contains("429") || error_msg.contains("rate limit") {
                                format!(
                                    "Rate limited by provider '{}'. Please wait and try again.\n\
                                    Details: {}",
                                    provider_type, error_msg
                                )
                            } else if error_msg.contains("404") || error_msg.contains("not_found") {
                                format!("Model '{}' not found. Try: claude-sonnet-4-6, claude-opus-4-6, or claude-haiku-4-5\n\
                                    Details: {}", model, error_msg)
                            } else if error_msg.contains("403") || error_msg.contains("forbidden") {
                                format!("Access denied by provider '{}'. Your account may not have access to model '{}'.\n\
                                    Details: {}", provider_type, model, error_msg)
                            } else {
                                format!(
                                    "Stream interrupted: {}. Try resending your message.",
                                    error_msg
                                )
                            }
                        }
                    };
                    let _ = stream_tx.send(StreamChunk::Error(enhanced));
                    let _ = stream_tx.send(StreamChunk::Done);
                    return Ok(());
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

            // Check for cancellation during sleep
            if stop_signal
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
            {
                let _ = stream_tx.send(StreamChunk::Done);
                return Ok(());
            }
        }

        tracing::info!(
            "Turn {} done: stop_action={:?}, assistant_response={} chars, thinking={} chars, tool_execs={}, content_blocks={}",
            turn_count,
            stop_action,
            assistant_response.len(),
            thinking_content.len(),
            tool_executions.len(),
            content_blocks.len(),
        );

        // Decide what to do next based on stop_reason
        match stop_action {
            ToolUseAction::ExecuteTools => {
                if tool_executions.is_empty() {
                    tracing::warn!("stop_reason='tool_use' but no tools were executed");
                    break;
                }

                if !thinking_content.is_empty() {
                    content_blocks.push(ContentBlock::thinking(
                        thinking_content.clone(),
                        thinking_signature.clone(),
                    ));
                }
                if !assistant_response.is_empty() {
                    content_blocks.push(ContentBlock::text(assistant_response.clone()));
                }

                // Append assistant message only if there is substantive content to send.
                // If there are structured content blocks, push them as a Blocks-based message.
                // Otherwise, push the plain textual assistant response only if non-empty.
                if !content_blocks.is_empty() {
                    messages.push(ChatMessage::assistant(MessageContent::Blocks(
                        content_blocks.clone(),
                    )));
                } else if !assistant_response.is_empty() {
                    messages.push(ChatMessage::assistant(assistant_response.clone()));
                }

                for tool_result in tool_executions {
                    let truncated_content = truncate_for_conversation(tool_result.result_content);
                    let tool_result_msg =
                        ChatMessage::tool_result(truncated_content, tool_result.tool_use_id);
                    messages.push(tool_result_msg);
                }

                prune_messages(&mut messages);
                continue;
            }
            ToolUseAction::Stop | ToolUseAction::None => {
                break;
            }
            ToolUseAction::ContinueServerTools => {
                break;
            }
        }
    }

    let _ = stream_tx.send(StreamChunk::Done);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::workspace_context::find_project_instruction_file;
    use tempfile::TempDir;

    #[test]
    fn test_load_project_instruction_file_prefers_instruction_md() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("instruction.md"), "Follow the steps").unwrap();
        std::fs::write(temp_dir.path().join("instructions.md"), "Use this instead").unwrap();

        let loaded = find_project_instruction_file(temp_dir.path());
        let (filename, content) = loaded.expect("instruction file should load");

        assert_eq!(filename, "instruction.md");
        assert_eq!(content, "Follow the steps");
    }

    #[test]
    fn test_load_project_instruction_file_missing() {
        let temp_dir = TempDir::new().unwrap();
        assert!(find_project_instruction_file(temp_dir.path()).is_none());
    }
}
