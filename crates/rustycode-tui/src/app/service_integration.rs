//! Service integration layer for TUI event loop
//!
//! This module provides the integration between the TUI event loop and background services:
//! - ConversationService (LLM streaming)
//! - ToolRuntime (tool execution)
//! - WorkspaceContext (workspace loading)
//!
//! ## Architecture
//!
//! Services run in background threads and send events through bounded channels.
//! The event loop polls ONE item per frame from each service, ensuring responsiveness.
//! Backpressure handling prevents memory bloat under heavy load.
//!
//! ## Flow
//!
//! ```text
//! Background Thread          Channel          Event Loop
//!      │                        │                  │
//!      │ 1. Try send event      │                  │
//!      ├───────────────────────>│                  │
//!      │                        │ 2. Queue (bounded)│
//!      │                        │                  │
//!      │                        │ 3. Poll ONE item  │
//!      │                        │<──────────────────┤
//!      │                        │                  │
//!      │                        │ 4. Process event  │
//!      │                        │                  │
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use rustycode_tui::app::service_integration::*;
//!
//! // Create service manager
//! let mut services = ServiceManager::new()?;
//!
//! // Start services with configuration
//! services.start_conversation(conversation_config, tool_registry)?;
//! services.start_workspace_loading(cwd)?;
//!
//! // In event loop - poll ONE item per frame
//! let frame_start = Instant::now();
//!
//! // Phase 1: Poll services (ONE item each)
//! services.poll_stream_one(|chunk| {
//!     // Handle stream chunk
//!     tui.add_stream_chunk(chunk);
//! })?;
//!
//! services.poll_tools_one(|result| {
//!     // Handle tool result
//!     tui.handle_tool_result(result);
//! })?;
//!
//! services.poll_workspace_one(|update| {
//!     // Handle workspace update
//!     tui.handle_workspace_update(update);
//! })?;
//!
//! // Phase 2: Check frame budget
//! let elapsed = frame_start.elapsed();
//! if elapsed < FRAME_BUDGET_60FPS {
//!     // Phase 3: Render
//!     terminal.draw(|f| tui.render(f))?;
//!
//!     // Phase 4: Handle input
//!     let timeout = FRAME_BUDGET_60FPS.saturating_sub(elapsed);
//!     if crossterm::event::poll(timeout)? {
//!         tui.handle_input()?;
//!     }
//! }
//! ```

use crate::agent_mode::AiMode;
use crate::app::async_::*;
use crate::app::streaming::stream_llm_response;
use crate::conversation_service::{ConversationConfig, ConversationService};
// sessions_dir import used by auto-session feature
use crate::workspace_context;
use crate::{ErrorTracker, FileReadCache};
use anyhow::{Context, Result};
use rustycode_protocol::QueryGuard;
use rustycode_tools::ToolRegistry;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::thread;

// ── Service Manager ───────────────────────────────────────────────────────────

/// Manages all background services for the TUI
///
/// The service manager owns all service channels and handles the lifecycle
/// of background tasks. It provides one-item-per-frame polling methods
/// for integration with the event loop.
pub struct ServiceManager {
    /// Conversation service (LLM streaming)
    conversation: Option<ConversationService>,

    /// Channel for LLM stream chunks
    stream_channel: Option<BoundedChannel<StreamChunk>>,

    /// Channel for tool execution results
    tool_channel: Option<BoundedChannel<ToolResult>>,

    /// Channel for workspace loading updates
    workspace_channel: Option<BoundedChannel<WorkspaceUpdate>>,

    /// Channel for slash command results
    command_channel: Option<BoundedChannel<SlashCommandResult>>,

    /// Channel for approval responses (TUI → streaming thread)
    approval_tx: Option<std::sync::mpsc::Sender<bool>>,

    /// Channel for question responses (TUI → streaming thread)
    question_tx: Option<std::sync::mpsc::Sender<String>>,

    /// Current AI mode
    ai_mode: AiMode,

    /// Current specialized agent mode
    agent_mode: crate::agent_mode::AgentMode,

    /// Current working directory
    cwd: PathBuf,

    /// Cooperative stop flag for active streaming requests
    stream_stop_requested: Arc<AtomicBool>,

    /// File read deduplication cache
    file_read_cache: Arc<StdMutex<FileReadCache>>,

    /// Tool error tracker
    error_tracker: Arc<StdMutex<ErrorTracker>>,

    /// Guard ensuring only one LLM query runs at a time
    query_guard: QueryGuard,

    /// Shared todo state for LLM todo tools (todo_read, todo_write, todo_update)
    todo_state: Option<rustycode_tools::TodoState>,

    /// Shared tool registry for executing tools (including skill tools)
    tool_registry: Option<Arc<rustycode_tools::ToolRegistry>>,
}

impl ServiceManager {
    /// Create a new service manager
    pub fn new(cwd: PathBuf, ai_mode: AiMode) -> Self {
        Self {
            conversation: None,
            stream_channel: None,
            tool_channel: None,
            workspace_channel: None,
            command_channel: Some(BoundedChannel::new(100)),
            approval_tx: None,
            question_tx: None,
            ai_mode,
            agent_mode: crate::agent_mode::AgentMode::Code,
            cwd,
            stream_stop_requested: Arc::new(AtomicBool::new(false)),
            file_read_cache: Arc::new(StdMutex::new(FileReadCache::new())),
            error_tracker: Arc::new(StdMutex::new(ErrorTracker::new())),
            query_guard: QueryGuard::new(),
            todo_state: None,
            tool_registry: None,
        }
    }

    /// Get the current working directory
    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    /// Get the file read cache
    pub fn file_read_cache(&self) -> Arc<StdMutex<FileReadCache>> {
        Arc::clone(&self.file_read_cache)
    }

    /// Get the error tracker
    pub fn error_tracker(&self) -> Arc<StdMutex<ErrorTracker>> {
        Arc::clone(&self.error_tracker)
    }

    /// Set the shared todo state for LLM todo tools
    pub fn set_todo_state(&mut self, state: rustycode_tools::TodoState) {
        self.todo_state = Some(state);
    }

    /// Start the conversation service
    ///
    /// Initializes the conversation service and creates the stream channel.
    /// Call this once at startup before sending messages.
    pub fn start_conversation(
        &mut self,
        config: ConversationConfig,
        tool_registry: ToolRegistry,
    ) -> Result<()> {
        // Store Arc reference to tool registry for tool execution
        let tool_registry_arc = Arc::new(tool_registry);
        self.tool_registry = Some(Arc::clone(&tool_registry_arc));

        // Create conversation service - pass the Arc'd registry
        let service = ConversationService::new(config, tool_registry_arc);

        // Create bounded channel for stream chunks (capacity 100)
        let stream_channel = BoundedChannel::new(100);

        // Create channel for tool results (capacity 50)
        let tool_channel = BoundedChannel::new(50);

        self.conversation = Some(service);
        self.stream_channel = Some(stream_channel);
        self.tool_channel = Some(tool_channel);

        tracing::info!("Conversation service started");

        Ok(())
    }

    /// Send a user message and start streaming response
    ///
    /// This method is called from the event loop when the user sends a message.
    /// It spawns a background task that streams the LLM response through the channel.
    pub fn send_message(
        &mut self,
        content: String,
        workspace_context: Option<String>,
    ) -> Result<()> {
        self.send_message_with_history(content, workspace_context, None)
    }

    /// Send a message with conversation history for multi-turn context
    pub fn send_message_with_history(
        &mut self,
        content: String,
        workspace_context: Option<String>,
        conversation_history: Option<Vec<rustycode_llm::provider_v2::ChatMessage>>,
    ) -> Result<()> {
        use rustycode_prompt::ModelProvider;

        // Prevent concurrent LLM queries — reject if already running
        if self.query_guard.is_active() {
            anyhow::bail!(
                "A query is already in progress. Wait for it to complete or cancel it first."
            );
        }

        let _generation = self
            .query_guard
            .try_start()
            .context("Failed to start query guard")?;

        // Verify service is started (release query guard on failure to prevent stuck state)
        let service = match self.conversation.as_ref() {
            Some(s) => s,
            None => {
                self.query_guard.force_end();
                anyhow::bail!("Conversation service not started");
            }
        };

        // Get tools schema from the service
        let provider_type = rustycode_llm::load_provider_config_from_env()
            .map(|(pt, _, _)| pt)
            .unwrap_or_else(|_| "anthropic".to_string());

        let provider = ModelProvider::from_model_id(&provider_type);
        let tools_schema = service.generate_tool_schema_for_provider(provider);

        tracing::debug!(
            "Generated tools schema: {}",
            serde_json::to_string_pretty(&tools_schema).unwrap_or_else(|_| "Invalid".to_string())
        );

        // Extract the tools array from the schema
        let tools = tools_schema
            .get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        tracing::info!(
            "🔧 Provider: {} | Tools registered: {} | Sending to LLM: {}",
            provider_type,
            service.tool_registry.list().len(),
            tools.len()
        );

        if tools.is_empty() {
            tracing::error!("⚠️  NO TOOLS SENT TO LLM! Tool use will NOT work!");
        } else {
            tracing::info!("✅ Tool names: {}", {
                let names: Vec<_> = tools
                    .iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                    .collect();
                names.join(", ")
            });
        }

        // Clone channel sender for background thread (release query guard on failure)
        let stream_tx = match self.stream_channel.as_ref() {
            Some(ch) => ch.clone_sender(),
            None => {
                self.query_guard.force_end();
                anyhow::bail!("Stream channel not created");
            }
        };

        // Clone cwd for the thread
        let cwd = self.cwd.clone();
        let stop_flag = Arc::clone(&self.stream_stop_requested);
        let agent_mode = self.agent_mode;

        // Clone cache and tracker for the thread
        let file_read_cache = Arc::clone(&self.file_read_cache);
        let error_tracker = Arc::clone(&self.error_tracker);

        // Clone todo state for the thread (LLM todo tools)
        let todo_state_clone = self.todo_state.clone();

        // Move conversation history into the thread
        let history = conversation_history;

        // Reset stop signal for this new request
        self.stream_stop_requested.store(false, Ordering::SeqCst);

        // Create approval channel for TUI → streaming thread communication
        let (approval_tx, approval_rx) = std::sync::mpsc::channel();
        self.approval_tx = Some(approval_tx);

        // Create question channel for TUI → streaming thread communication
        let (question_tx, question_rx) = std::sync::mpsc::channel();
        self.question_tx = Some(question_tx);

        // Clone tool registry for the background thread
        let tool_registry_for_thread = self.tool_registry.clone();

        // Spawn background task for real LLM streaming
        //
        // Panic guard: if the thread panics (e.g. during tokio runtime creation
        // or block_on), we catch it and send Error+Done so the TUI doesn't get
        // stuck in is_streaming=true forever.
        let stream_tx_panic = stream_tx.clone();
        thread::spawn(move || {
            // Catch panics to prevent frozen TUI state
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Use tokio runtime for async LLM calls
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = stream_tx.send(StreamChunk::Error(format!(
                            "Failed to create runtime: {}",
                            e
                        )));
                        let _ = stream_tx.send(StreamChunk::Done);
                        return;
                    }
                };

                // Execute async LLM call in the runtime
                let result = rt.block_on(async {
                    let config =
                        crate::app::streaming::StreamConfig::new(&content, &cwd, stream_tx.clone())
                            .workspace_context_opt(workspace_context)
                            .stop_signal_opt(Some(stop_flag))
                            .tools_schema_opt(Some(tools))
                            .approval_rx_opt(Some(approval_rx))
                            .question_rx_opt(Some(question_rx))
                            .agent_mode_opt(Some(agent_mode))
                            .file_read_cache_opt(Some(file_read_cache))
                            .error_tracker_opt(Some(error_tracker))
                            .todo_state_opt(todo_state_clone)
                            .conversation_history_opt(history)
                            .tool_registry_opt(tool_registry_for_thread);

                    stream_llm_response(config).await
                });

                if let Err(e) = result {
                    let _ = stream_tx.send(StreamChunk::Error(format!("LLM error: {}", e)));
                    let _ = stream_tx.send(StreamChunk::Done);
                }
            }));

            if result.is_err() {
                // Thread panicked — ensure TUI gets unstuck
                let _ = stream_tx_panic.send(StreamChunk::Error(
                    "Internal error: streaming thread panicked".to_string(),
                ));
                let _ = stream_tx_panic.send(StreamChunk::Done);
            }
        });

        Ok(())
    }

    /// Request cooperative cancellation of an active stream.
    pub fn request_stop_stream(&mut self) {
        self.stream_stop_requested.store(true, Ordering::SeqCst);
        self.query_guard.force_end();
    }

    /// Mark the current query as completed (called when stream ends).
    pub fn complete_query(&mut self) {
        self.query_guard.force_end();
    }

    /// Check if a query is currently active.
    pub fn is_query_active(&self) -> bool {
        self.query_guard.is_active()
    }

    /// Send approval response to the streaming thread
    ///
    /// Called by TUI when user responds to an approval request.
    /// `true` = approve, `false` = reject
    pub fn send_approval_response(&self, approved: bool) {
        if let Some(ref tx) = self.approval_tx {
            if let Err(e) = tx.send(approved) {
                tracing::warn!("Failed to send approval response: {}", e);
            }
        } else {
            tracing::warn!("No approval channel available — response dropped");
        }
    }

    /// Send question response to the streaming thread
    ///
    /// Called by TUI when user answers a question.
    pub fn send_question_response(&self, answer: String) {
        if let Some(ref tx) = self.question_tx {
            let _ = tx.send(answer);
        }
    }

    /// Check if there's an active approval channel
    pub fn has_approval_channel(&self) -> bool {
        self.approval_tx.is_some()
    }

    /// Get the current agent mode
    pub fn agent_mode(&self) -> crate::agent_mode::AgentMode {
        self.agent_mode
    }

    /// Set the agent mode
    pub fn set_agent_mode(&mut self, mode: crate::agent_mode::AgentMode) {
        self.agent_mode = mode;
    }

    /// Cycle to the next agent mode
    pub fn next_agent_mode(&mut self) -> crate::agent_mode::AgentMode {
        self.agent_mode = self.agent_mode.next_mode();
        self.agent_mode
    }

    /// Cycle to the previous agent mode
    pub fn prev_agent_mode(&mut self) -> crate::agent_mode::AgentMode {
        self.agent_mode = self.agent_mode.prev();
        self.agent_mode
    }

    /// Check if a tool is allowed in the current agent mode
    pub fn allows_tool(&self, tool_name: &str) -> bool {
        self.agent_mode.allows_tool(tool_name)
    }

    /// Start workspace context loading
    ///
    /// Spawns a background task that loads workspace information and sends
    /// progress updates through the workspace channel.
    pub fn start_workspace_loading(&mut self) -> Result<()> {
        // Create bounded channel for workspace updates (capacity 20)
        let workspace_channel = BoundedChannel::new(20);

        let cwd = self.cwd.clone();
        let tx = workspace_channel.clone_sender();

        // Spawn background task for workspace loading with progress tracking
        let tx_final = workspace_channel.clone_sender();
        thread::spawn(move || {
            // Create progress callback that sends updates through the channel
            let progress_callback: workspace_context::ScanProgressCallback =
                Box::new(move |scanned: usize, total: usize| {
                    let _ = tx.send(WorkspaceUpdate::ScanProgress { scanned, total });
                });

            // Load workspace context with progress tracking
            let context = workspace_context::load_workspace_context_with_progress(
                &cwd,
                10,
                20,
                Some(progress_callback),
            );

            // Send final context loaded message
            let _ = tx_final.send(WorkspaceUpdate::ContextLoaded(context));

            match workspace_context::find_project_instruction_file(&cwd) {
                Some((filename, _)) => {
                    let _ = tx_final.send(WorkspaceUpdate::Notice(format!(
                        "Loaded {} from the workspace root",
                        filename
                    )));
                }
                None => {
                    let _ = tx_final.send(WorkspaceUpdate::Notice(
                        "No instruction.md or instructions.md found in the workspace root"
                            .to_string(),
                    ));
                }
            }
        });

        self.workspace_channel = Some(workspace_channel);

        tracing::info!("Workspace loading started with progress tracking");

        Ok(())
    }

    /// Poll ONE stream chunk from the LLM
    ///
    /// This method is called once per frame in the event loop.
    /// It processes at most ONE chunk, ensuring responsiveness.
    ///
    /// # Arguments
    ///
    /// * `callback` - Function to handle the received chunk
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if a chunk was processed
    /// * `Ok(false)` if no chunk was available
    /// * `Err` if there was an error
    pub fn poll_stream_one<F>(&mut self, callback: F) -> Result<bool>
    where
        F: FnOnce(StreamChunk),
    {
        let channel = self
            .stream_channel
            .as_mut()
            .context("Stream channel not created")?;

        match channel.try_recv() {
            Some(chunk) => {
                callback(chunk);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Poll ONE tool result from the tool execution
    ///
    /// This method is called once per frame in the event loop.
    /// It processes at most ONE result, ensuring responsiveness.
    pub fn poll_tools_one<F>(&mut self, callback: F) -> Result<bool>
    where
        F: FnOnce(ToolResult),
    {
        let channel = self
            .tool_channel
            .as_mut()
            .context("Tool channel not created")?;

        match channel.try_recv() {
            Some(result) => {
                callback(result);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Poll ONE workspace update from the workspace loader
    ///
    /// This method is called once per frame in the event loop.
    /// It processes at most ONE update, ensuring responsiveness.
    pub fn poll_workspace_one<F>(&mut self, callback: F) -> Result<bool>
    where
        F: FnOnce(WorkspaceUpdate),
    {
        let channel = self
            .workspace_channel
            .as_mut()
            .context("Workspace channel not created")?;

        match channel.try_recv() {
            Some(update) => {
                callback(update);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Get the current AI mode
    pub fn ai_mode(&self) -> AiMode {
        self.ai_mode
    }

    /// Set the AI mode
    pub fn set_ai_mode(&mut self, mode: AiMode) {
        self.ai_mode = mode;
        // Note: ConversationService mode setting will be implemented
        // when the service API is finalized
    }

    /// Get statistics about channel health
    pub fn channel_stats(&self) -> ServiceStats {
        ServiceStats {
            stream_dropped: self
                .stream_channel
                .as_ref()
                .map(|c| c.dropped_count())
                .unwrap_or(0),
            tool_dropped: self
                .tool_channel
                .as_ref()
                .map(|c| c.dropped_count())
                .unwrap_or(0),
            workspace_dropped: self
                .workspace_channel
                .as_ref()
                .map(|c| c.dropped_count())
                .unwrap_or(0),
        }
    }

    /// Get mutable reference to stream channel (for event loop polling)
    pub fn stream_channel_mut(&mut self) -> Option<&mut BoundedChannel<StreamChunk>> {
        self.stream_channel.as_mut()
    }

    /// Get mutable reference to tool channel (for event loop polling)
    pub fn tool_channel_mut(&mut self) -> Option<&mut BoundedChannel<ToolResult>> {
        self.tool_channel.as_mut()
    }

    /// Get mutable reference to workspace channel (for event loop polling)
    pub fn workspace_channel_mut(&mut self) -> Option<&mut BoundedChannel<WorkspaceUpdate>> {
        self.workspace_channel.as_mut()
    }

    /// Get a mutable reference to the command channel
    pub fn command_channel_mut(&mut self) -> Option<&mut BoundedChannel<SlashCommandResult>> {
        self.command_channel.as_mut()
    }

    /// Get a sender for the command channel
    pub fn command_sender(&self) -> Option<std::sync::mpsc::SyncSender<SlashCommandResult>> {
        self.command_channel.as_ref().map(|c| c.clone_sender())
    }
}

// ── Statistics ────────────────────────────────────────────────────────────────

/// Statistics about service channel health
#[derive(Debug, Clone)]
pub struct ServiceStats {
    /// Number of dropped stream chunks (backpressure)
    pub stream_dropped: usize,
    /// Number of dropped tool results (backpressure)
    pub tool_dropped: usize,
    /// Number of dropped workspace updates (backpressure)
    pub workspace_dropped: usize,
}

impl ServiceStats {
    /// Check if any service is experiencing backpressure
    pub fn has_backpressure(&self) -> bool {
        self.stream_dropped > 0 || self.tool_dropped > 0 || self.workspace_dropped > 0
    }

    /// Get total dropped events
    pub fn total_dropped(&self) -> usize {
        self.stream_dropped + self.tool_dropped + self.workspace_dropped
    }
}

// ── Integration with TUI ───────────────────────────────────────────────────────

/// Integration layer for connecting services to the TUI
///
/// This trait provides methods for converting service events into TUI state updates.
pub trait TUIIntegration {
    /// Handle a stream chunk from the LLM
    fn handle_stream_chunk(&mut self, chunk: StreamChunk);

    /// Handle a tool execution result
    fn handle_tool_result(&mut self, result: ToolResult);

    /// Handle a workspace update
    fn handle_workspace_update(&mut self, update: WorkspaceUpdate);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_manager_creation() {
        let cwd = PathBuf::from("/tmp");
        let manager = ServiceManager::new(cwd, AiMode::Act);
        assert_eq!(manager.ai_mode(), AiMode::Act);
    }

    #[test]
    fn test_ai_mode_get_set() {
        let cwd = PathBuf::from("/tmp");
        let mut manager = ServiceManager::new(cwd, AiMode::Act);

        manager.set_ai_mode(AiMode::Plan);
        assert_eq!(manager.ai_mode(), AiMode::Plan);
    }

    #[test]
    fn test_channel_stats() {
        let stats = ServiceStats {
            stream_dropped: 5,
            tool_dropped: 2,
            workspace_dropped: 0,
        };

        assert!(stats.has_backpressure());
        assert_eq!(stats.total_dropped(), 7);
    }

    #[test]
    fn test_channel_stats_no_backpressure() {
        let stats = ServiceStats {
            stream_dropped: 0,
            tool_dropped: 0,
            workspace_dropped: 0,
        };

        assert!(!stats.has_backpressure());
        assert_eq!(stats.total_dropped(), 0);
    }
}
