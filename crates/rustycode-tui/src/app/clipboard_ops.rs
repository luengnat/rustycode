//! Clipboard and export operations
//!
//! Handles copying messages and conversations to clipboard, and exporting to files.

use super::event_loop::TUI;
use anyhow::Result;

impl TUI {
    /// Copy selected message to clipboard
    pub(crate) fn copy_selected_message(&mut self) -> Result<()> {
        use crate::clipboard::copy_text_to_clipboard_both;

        if self.selected_message < self.messages.len() {
            let msg = &self.messages[self.selected_message];

            // Get the message content (without the role prefix)
            let content = msg.content.clone();

            // Copy to clipboard
            let copy_result = copy_text_to_clipboard_both(&content);

            if let Err(e) = copy_result {
                tracing::error!("Failed to copy message to clipboard: {}", e);
                self.toast_manager.error(format!("Failed to copy: {}", e));
                self.add_system_message(format!("[X] Failed to copy: {}", e));
            } else {
                let chars = content.chars().count();
                tracing::debug!("Copied message to clipboard ({} chars)", chars);
                self.toast_manager
                    .success(format!("✓ Copied {} chars", chars));
                self.add_system_message(format!("[OK] Copied {} characters to clipboard", chars));
            }

            self.dirty = true;
        }

        Ok(())
    }

    /// Copy the last AI assistant response to clipboard (Ctrl+Y)
    pub(crate) fn copy_last_ai_response(&mut self) -> Result<()> {
        use crate::clipboard::copy_text_to_clipboard_both;

        // Find the last assistant message
        let last_ai_idx = self
            .messages
            .iter()
            .rposition(|msg| matches!(msg.role, crate::ui::message::MessageRole::Assistant));

        match last_ai_idx {
            Some(idx) => {
                let content = self.messages[idx].content.clone();
                let chars = content.chars().count();

                match copy_text_to_clipboard_both(&content) {
                    Ok(()) => {
                        self.toast_manager
                            .success(format!("✓ Copied last response ({} chars)", chars));
                        self.add_system_message(format!(
                            "[OK] Copied last AI response ({} chars) to clipboard",
                            chars
                        ));
                    }
                    Err(e) => {
                        self.toast_manager.error(format!("Failed to copy: {}", e));
                        self.add_system_message(format!("[X] Failed to copy: {}", e));
                    }
                }
            }
            None => {
                self.add_system_message("No AI response to copy yet".to_string());
            }
        }

        self.dirty = true;
        Ok(())
    }

    /// Copy entire conversation to clipboard (excludes system messages and tool panel)
    pub(crate) fn copy_all_conversation(&mut self) -> Result<()> {
        use crate::clipboard::copy_text_to_clipboard_both;
        use crate::ui::message::MessageRole;

        // Build conversation text with just user/assistant messages
        let mut conversation = Vec::new();

        for msg in &self.messages {
            match msg.role {
                MessageRole::User => {
                    conversation.push(format!("User: {}", msg.content));
                }
                MessageRole::Assistant => {
                    conversation.push(format!("Assistant: {}", msg.content));
                }
                MessageRole::System => {
                    // Skip system messages (help, status, etc.)
                    continue;
                }
            }
        }

        let content = conversation.join("\n\n");

        // Copy to clipboard
        let copy_result = copy_text_to_clipboard_both(&content);

        if let Err(e) = copy_result {
            tracing::error!("Failed to copy conversation to clipboard: {}", e);
            self.toast_manager.error(format!("Failed to copy: {}", e));
            self.add_system_message(format!("[X] Failed to copy conversation: {}", e));
        } else {
            let msg_count = self
                .messages
                .iter()
                .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
                .count();
            let chars = content.chars().count();
            tracing::debug!(
                "Copied {} messages to clipboard ({} chars)",
                msg_count,
                chars
            );
            self.toast_manager
                .success(format!("✓ Copied {} messages", msg_count));
            let success_msg = format!(
                "[OK] Copied {} messages ({} characters) to clipboard",
                msg_count, chars
            );
            self.add_system_message(success_msg);
        }

        self.dirty = true;
        Ok(())
    }

    /// Export current conversation to file (Ctrl+Shift+E)
    pub(crate) fn export_conversation(&mut self) -> Result<()> {
        use crate::ui::message_export::{ConversationExporter, ExportFormat, ExportOptions};
        use dirs::home_dir;

        // Determine export directory
        let export_dir = if let Some(home) = home_dir() {
            home.join(".rustycode").join("exports")
        } else {
            std::path::PathBuf::from("./exports")
        };

        // Create exporter
        let exporter = ConversationExporter::new(export_dir.clone())?;

        // Use default export options (include tools, exclude thinking/metadata/timestamps)
        let options = ExportOptions::default();

        // Export as markdown
        let path = exporter.export(&self.messages, ExportFormat::Markdown, options)?;

        let msg_count = self
            .messages
            .iter()
            .filter(|m| {
                matches!(
                    m.role,
                    crate::ui::message::MessageRole::User
                        | crate::ui::message::MessageRole::Assistant
                )
            })
            .count();

        tracing::debug!("Exported {} messages to {}", msg_count, path.display());

        self.toast_manager
            .success(format!("✓ Exported {} messages", msg_count));

        let success_msg = format!(
            "[OK] Exported {} messages to {}",
            msg_count,
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("conversation.md")
        );
        self.add_system_message(success_msg);

        self.dirty = true;
        Ok(())
    }

    /// Regenerate the last AI response
    pub(crate) fn regenerate_last_response(&mut self) -> Result<()> {
        // Don't regenerate if we're already streaming
        if self.is_streaming {
            self.add_system_message(
                "⚠️  Cannot regenerate while streaming. Please wait.".to_string(),
            );
            return Ok(());
        }

        // Find the last AI message (assistant role)
        let last_ai_msg_idx = self
            .messages
            .iter()
            .rposition(|msg| msg.role == crate::ui::message::MessageRole::Assistant);

        let last_ai_msg_idx = match last_ai_msg_idx {
            Some(idx) => idx,
            None => {
                self.add_system_message(
                    "⚠️  No AI response to regenerate. Send a message first.".to_string(),
                );
                return Ok(());
            }
        };

        // Get the message before the AI message (the user's prompt)
        let user_msg_idx = match last_ai_msg_idx.checked_sub(1) {
            Some(idx) => idx,
            None => {
                self.add_system_message(
                    "⚠️  Cannot find user prompt to regenerate from.".to_string(),
                );
                return Ok(());
            }
        };

        let user_prompt = self.messages[user_msg_idx].content.clone();

        // Show regeneration started message
        let regen_msg = "🔄 Regenerating response...".to_string();
        self.add_system_message(regen_msg);

        // Remove the old AI message
        self.messages.remove(last_ai_msg_idx);

        // Update dirty flag
        self.dirty = true;

        // Send the user prompt again to get a new response
        let workspace_context = self.workspace_context.clone();
        let history = self.build_conversation_history();

        // Set streaming state before send to prevent double-Enter races
        self.is_streaming = true;
        self.chunks_received = 0;
        self.stream_start_time = Some(std::time::Instant::now());
        self.current_stream_content.clear();
        self.streaming_render_buffer =
            crate::app::streaming_render_buffer::StreamingRenderBuffer::new();

        if let Err(e) =
            self.services
                .send_message_with_history(user_prompt, workspace_context, Some(history))
        {
            tracing::error!("Failed to regenerate response: {}", e);
            self.is_streaming = false;
            self.chunks_received = 0;
            self.current_stream_content.clear();
            self.streaming_render_buffer =
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
            self.stream_start_time = None;
            self.active_tools.clear();
            self.add_system_message(format!("Regeneration failed: {}", e));
        } else {
            // Create empty assistant message for streaming to fill
            let assistant_msg = crate::ui::message::Message::assistant(String::new());
            self.messages.push(assistant_msg);
            self.auto_scroll();
        }

        Ok(())
    }

    /// Undo the last task extraction
    pub(crate) fn undo_last_extraction(&mut self) -> Result<()> {
        if let Some((old_tasks, old_todos)) = self.last_extraction.take() {
            // Update workspace tasks
            self.workspace_tasks.tasks = old_tasks;
            self.workspace_tasks.todos = old_todos;

            // Save the reverted tasks
            if let Err(e) = crate::tasks::save_tasks(&self.workspace_tasks) {
                self.add_system_message(format!("❌ Failed to save reverted tasks: {}", e));
                return Err(e.into());
            }

            // Update analytics
            if let Err(e) = crate::extraction_analytics::record_undo() {
                tracing::warn!("Failed to record extraction undo: {}", e);
            }

            self.add_system_message(
                "✅ Successfully reverted the last task extraction.".to_string(),
            );
            self.dirty = true;
            Ok(())
        } else {
            self.add_system_message("⚠️  No recent task extraction to revert.".to_string());
            Ok(())
        }
    }
}
