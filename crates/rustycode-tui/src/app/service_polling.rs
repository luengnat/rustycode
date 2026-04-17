//! Service polling operations
//!
//! Handles polling async services for events.

use super::async_::RecvStatus;
use super::event_loop::TUI;
use anyhow::Result;
use rustycode_core::team::orchestrator::TeamEvent;

impl TUI {
    /// Poll all services (ONE item per frame each)
    pub(crate) fn poll_services(&mut self) -> Result<()> {
        // Inline poll implementation to avoid closure borrow issues

        // Poll LLM stream chunks — drain up to 32 per frame to prevent backlog
        // during active streaming. Text chunks are cheap (just string append + dirty flag),
        // so batching many per frame keeps the UI responsive.
        //
        // We collect chunks into a vec first to release the channel borrow before
        // passing self to the handler (avoids E0499 double-mutable-borrow).
        const MAX_STREAM_CHUNKS_PER_FRAME: usize = 32;
        let mut had_stream = false;
        let mut channel_disconnected = false;
        {
            let mut chunks: Vec<crate::app::async_::StreamChunk> = Vec::new();
            if let Some(channel) = self.services.stream_channel_mut() {
                for _ in 0..MAX_STREAM_CHUNKS_PER_FRAME {
                    match channel.try_recv_ex() {
                        RecvStatus::Item(chunk) => chunks.push(chunk),
                        RecvStatus::Empty => break,
                        RecvStatus::Disconnected => {
                            // Channel disconnected — sender dropped without sending Done
                            channel_disconnected = true;
                            break;
                        }
                    }
                }
            }
            for chunk in chunks {
                crate::app::handlers::handle_stream_chunk(self, chunk);
                had_stream = true;
            }
        }

        // If channel disconnected while streaming, force cleanup to prevent
        // the TUI from being stuck in is_streaming=true forever.
        if channel_disconnected && self.is_streaming {
            tracing::warn!("Stream channel disconnected without Done — forcing cleanup");
            self.is_streaming = false;
            self.stream_cancelled = false;
            self.active_tools.clear();
            self.streaming_render_buffer =
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
            self.chunks_received = 0;
            self.current_stream_content.clear();
            self.stream_start_time = None;
            self.services.complete_query();
            self.update_terminal_title();
            self.add_system_message(
                "⚠ Stream connection lost unexpectedly. You can retry.".to_string(),
            );
            self.dirty = true;
        }

        // Poll tool results — drain up to 8 per frame (tools are heavier)
        let mut had_tool = false;
        {
            let mut results: Vec<crate::app::async_::ToolResult> = Vec::new();
            if let Some(channel) = self.services.tool_channel_mut() {
                for _ in 0..8 {
                    match channel.try_recv() {
                        Some(result) => results.push(result),
                        None => break,
                    }
                }
            }
            for result in results {
                crate::app::handlers::handle_tool_result(self, result);
                had_tool = true;
            }
        }

        // Poll workspace updates
        let had_workspace = {
            let update = self
                .services
                .workspace_channel_mut()
                .and_then(|ch| ch.try_recv());
            match update {
                Some(update) => {
                    crate::app::handlers::handle_workspace_update(self, update);
                    true
                }
                None => false,
            }
        };

        // Poll slash command results
        let had_command = {
            let result = self
                .services
                .command_channel_mut()
                .and_then(|ch| ch.try_recv());
            match result {
                Some(result) => {
                    crate::app::handlers::handle_slash_command_result(self, result);
                    true
                }
                None => false,
            }
        };

        // Log if we processed any events (for debugging)
        if had_stream || had_tool || had_workspace || had_command {
            tracing::debug!(
                "Processed events: stream={}, tool={}, workspace={}, command={}",
                had_stream,
                had_tool,
                had_workspace,
                had_command
            );
        }

        // Poll team events (from TeamOrchestrator broadcast channel)
        self.poll_team_events();

        // Poll worker registry updates
        self.poll_worker_registry();

        // Poll background bash command result
        let bash_result = {
            let mut store = self
                .pending_bash_result
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            store.take()
        };
        if let Some(text) = bash_result {
            // Truncate long output for display
            let display = if text.len() > 4000 {
                let byte_limit = text.floor_char_boundary(4000);
                let end = text[..byte_limit].rfind('\n').unwrap_or(byte_limit);
                format!(
                    "{}\n... ({} chars truncated)",
                    &text[..end],
                    text.len() - end
                )
            } else {
                text
            };
            self.add_system_message(format!("✓ {}", display));
            self.auto_scroll();
            self.dirty = true;
        }

        Ok(())
    }

    /// Poll team events from the orchestrator
    fn poll_team_events(&mut self) {
        if let Some(ref mut rx) = self.team_handler.event_rx {
            let mut team_messages: Vec<String> = Vec::new();
            // Drain all available team events (they're small and cheap to process)
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        self.team_panel.handle_event(&event);
                        self.dirty = true;

                        // Collect chat messages for key events (applied after loop
                        // to avoid borrow conflicts with team_panel)
                        match &event {
                            TeamEvent::AgentActivated { role, turn, reason } => {
                                team_messages.push(format!(
                                    "[Team] {} activated (turn {}): {}",
                                    role, turn, reason
                                ));
                            }
                            TeamEvent::Insight { role, message } => {
                                team_messages.push(format!("[Team/{}] {}", role, message));
                            }
                            TeamEvent::TaskCompleted {
                                success,
                                turns,
                                files_modified,
                                ..
                            } => {
                                let status = if *success { "SUCCESS" } else { "FAILED" };
                                let files_msg = if files_modified.is_empty() {
                                    String::new()
                                } else {
                                    format!("\n   Files: {}", files_modified.join(", "))
                                };
                                team_messages.push(format!(
                                    "[Team] {} in {} turns.{}",
                                    status, turns, files_msg
                                ));
                            }
                            TeamEvent::CodeChanged { files, author, .. } => {
                                team_messages.push(format!(
                                    "[Team] {} modified: {}",
                                    author,
                                    files.join(", ")
                                ));
                            }
                            TeamEvent::CompilationFailed { errors, .. } => {
                                team_messages.push(format!(
                                    "[Team] Compilation failed: {} error(s)",
                                    errors.len()
                                ));
                            }
                            _ => {} // Other events handled by panel only
                        }
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                        // Channel closed — orchestrator finished
                        self.team_handler.event_rx = None;
                        break;
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::warn!("Team event receiver lagged by {} events", n);
                        continue;
                    }
                }
            }
            // Apply collected messages (separate from team_panel borrow)
            for msg in team_messages {
                self.add_system_message(msg);
            }
        }
    }

    /// Poll worker registry and update worker panel
    fn poll_worker_registry(&mut self) {
        use rustycode_protocol::worker_registry::global_worker_registry;

        // Skip polling if the worker panel isn't visible and there are no active agents
        // This avoids needless global_worker_registry() calls every frame
        if !self.worker_panel.visible && self.agent_manager.get_agents().is_empty() {
            return;
        }

        let registry = global_worker_registry();
        let workers = registry.list();

        let prev_count = self.worker_panel.total_workers();
        self.worker_panel.update_from_workers(&workers);

        // Mark dirty only when worker count or panel visibility changed
        if prev_count != workers.len() || (!workers.is_empty() && self.worker_panel.visible) {
            self.dirty = true;
        }
    }
}
