//! Text input and composition handling
//!
//! Handles text input, search box, command palette, and special input modes.

use crate::app::event_loop::TUI;
use crate::session::save_command_history;
use crate::ui::input::InputAction;
use crate::ui::message_search::SearchEngine;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};

impl TUI {
    /// Handle search box input
    pub(crate) fn handle_search_input(&mut self, key_code: KeyCode) -> Result<bool> {
        if !self.search_state.visible {
            return Ok(false);
        }

        match key_code {
            KeyCode::Esc => {
                self.search_state.clear();
                self.dirty = true;
            }
            KeyCode::Enter => {
                // Navigate to next match on Enter
                self.search_state.next_match();
                self.scroll_to_current_search_match();
                self.dirty = true;
            }
            KeyCode::Up => {
                // Navigate to previous match with Up arrow
                self.search_state.prev_match();
                self.scroll_to_current_search_match();
                self.dirty = true;
            }
            KeyCode::Down => {
                // Navigate to next match with Down arrow
                self.search_state.next_match();
                self.scroll_to_current_search_match();
                self.dirty = true;
            }
            KeyCode::Char(c) => {
                SearchEngine::add_char(&mut self.search_state, c);
                // Perform search with updated query
                let case_sensitive = self.search_state.case_sensitive;
                let role_filter = self.search_state.role_filter.clone();
                self.search_state.matches = SearchEngine::search(
                    &self.search_state.query,
                    &self.messages,
                    case_sensitive,
                    &role_filter,
                );
                SearchEngine::reset_match_position(&mut self.search_state);
                self.dirty = true;
            }
            KeyCode::Backspace => {
                SearchEngine::backspace(&mut self.search_state);
                // Perform search with updated query
                let case_sensitive = self.search_state.case_sensitive;
                let role_filter = self.search_state.role_filter.clone();
                self.search_state.matches = SearchEngine::search(
                    &self.search_state.query,
                    &self.messages,
                    case_sensitive,
                    &role_filter,
                );
                SearchEngine::reset_match_position(&mut self.search_state);
                self.dirty = true;
            }
            _ => {
                // Ignore other keys
                return Ok(true);
            }
        }
        Ok(true)
    }

    /// Handle command palette navigation
    pub(crate) fn handle_command_palette_input(
        &mut self,
        key_code: KeyCode,
        _modifiers: KeyModifiers,
    ) -> Result<bool> {
        if !self.showing_command_palette {
            return Ok(false);
        }

        match key_code {
            KeyCode::Esc => {
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.command_palette.state_mut().clear_query();
                // Only clear the '/' prefix, preserve any other text the user typed
                let current = self.input_handler.state.all_text();
                if let Some(rest) = current.strip_prefix('/') {
                    self.input_handler.state.clear();
                    for c in rest.chars() {
                        self.input_handler.state.insert_char(c);
                    }
                    self.input_mode = self.input_handler.state.mode;
                }
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Enter => {
                // Insert selected command into input and submit
                if let Some(command) = self.command_palette.state().selected_command() {
                    let cmd_name = command.name.clone();
                    self.input_handler.state.clear();
                    for c in cmd_name.chars() {
                        self.input_handler.state.insert_char(c);
                    }
                    self.input_mode = self.input_handler.state.mode;
                }
                // Close palette silently — normal dispatch handles the typed command.
                // Avoids spurious "No matching command found" before actual dispatch.
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.command_palette.state_mut().clear_query();
                self.dirty = true;
                // Return false to allow command submission
                Ok(false)
            }
            KeyCode::Tab => {
                // Insert selected command into input, keep palette for args
                if let Some(command) = self.command_palette.state().selected_command() {
                    let cmd_name = command.name.clone();
                    let has_hint = !command.argument_hint.is_empty();
                    self.input_handler.state.clear();
                    for c in cmd_name.chars() {
                        self.input_handler.state.insert_char(c);
                    }
                    if has_hint {
                        // Add space after command for argument typing
                        self.input_handler.state.insert_char(' ');
                    }
                    self.input_mode = self.input_handler.state.mode;
                }
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.command_palette.state_mut().clear_query();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Up => {
                self.command_palette.state_mut().move_up();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Down => {
                self.command_palette.state_mut().move_down();
                self.dirty = true;
                Ok(true)
            }
            _ => {
                // All other keys go to normal input handler
                // Palette will filter from input text
                Ok(false)
            }
        }
    }

    /// Handle skill palette input
    pub(crate) fn handle_skill_palette_input(
        &mut self,
        key_code: KeyCode,
        key: crossterm::event::KeyEvent,
    ) -> Result<bool> {
        if !self.showing_skill_palette {
            return Ok(false);
        }

        match key_code {
            KeyCode::Esc => {
                self.showing_skill_palette = false;
                self.skill_palette.close();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Enter => {
                if let Some(skill) = self.skill_palette.take_selected() {
                    self.insert_skill_mention(&skill.name);
                    self.add_system_message(format!("Selected skill: {}", skill.name));
                }
                self.showing_skill_palette = false;
                self.skill_palette.close();
                self.dirty = true;
                Ok(true)
            }
            _ => {
                if self.skill_palette.handle_key(key) {
                    self.dirty = true;
                }
                Ok(true)
            }
        }
    }

    /// Process send message action
    pub(crate) fn process_send_message(&mut self, lines: Vec<String>) -> Result<()> {
        // Queue message if already streaming (goose pattern)
        if self.is_streaming {
            let text = lines.join("\n");
            if !text.trim().is_empty() {
                if self.queued_message.is_some() {
                    // Already have a queued message — offer to replace it
                    self.queued_message = Some(text);
                    self.add_system_message("Replaced queued message".to_string());
                } else {
                    self.queued_message = Some(text);
                    self.add_system_message(
                        "Message queued - will send when generation completes".to_string(),
                    );
                    self.input_handler.state.clear();
                    self.input_mode = self.input_handler.state.mode;
                }
                self.dirty = true;
            }
            return Ok(());
        }

        // Check if we're in rate limit state - if so, check if retry is allowed
        if self.rate_limit.until.is_some() {
            let can_retry = if let Some(until) = self.rate_limit.until {
                until
                    .saturating_duration_since(std::time::Instant::now())
                    .as_secs()
                    == 0
            } else {
                true
            };

            if can_retry {
                // Rate limit expired — clear it and send the new message normally
                self.rate_limit.clear();
            } else {
                let remaining = self
                    .rate_limit
                    .until
                    .map(|t| {
                        t.saturating_duration_since(std::time::Instant::now())
                            .as_secs()
                    })
                    .unwrap_or(0);
                self.add_system_message(format!("⚠️  Rate limit active - wait {}s", remaining));
                self.auto_scroll();
                return Ok(());
            }
        }

        let content = lines.join("\n");
        if content.trim().is_empty() {
            return Ok(());
        }

        // Check if this is the first user message BEFORE pushing (for shell history injection)
        let is_first_user_message = !self
            .messages
            .iter()
            .any(|m| matches!(m.role, crate::ui::message::MessageRole::User));

        // Extract images from input state before clearing
        let attached_images: Vec<_> = self.input_handler.state.images.drain(..).collect();

        const MAX_MESSAGE_LENGTH: usize = 100_000;
        if content.len() > MAX_MESSAGE_LENGTH {
            self.add_system_message(format!(
                "⚠️  Message too long ({} chars). Maximum is {} chars.",
                content.len(),
                MAX_MESSAGE_LENGTH
            ));
            self.auto_scroll();
            return Ok(());
        }

        // Check if task might benefit from team mode
        let team_suggestion = TUI::evaluate_team_mode_suggestion(&content);
        if let Some(suggestion) = team_suggestion {
            self.add_system_message(suggestion);
        }

        self.input_handler.add_to_history(content.clone());
        let _ = save_command_history(self.input_handler.get_history());

        let injection_summary = self.get_injection_summary_display(&content);
        if !injection_summary.is_empty() {
            self.add_system_message(injection_summary);
        }

        let message = crate::ui::message::Message::user(content.clone());
        self.messages.push(message);
        self.dirty = true;

        // Show image attachment notification
        if !attached_images.is_empty() {
            let img_count = attached_images.len();
            self.add_system_message(format!(
                "Attached {} image{} to message",
                img_count,
                if img_count > 1 { "s" } else { "" }
            ));
        }

        if !self.messages.is_empty() {
            self.selected_message = self.messages.len() - 1;
            self.scroll_offset_line = 0;
            self.user_scrolled = false;
        }

        self.input_handler.state.clear();
        self.input_mode = self.input_handler.state.mode;

        // Clear search state when sending a message to prevent stale highlighting
        self.search_state.query.clear();
        self.search_state.visible = false;

        if let Some(rest) = content.strip_prefix('!') {
            // Bash mode: execute shell command
            let cmd = rest.trim();
            if !cmd.is_empty() {
                self.add_system_message(format!("$ {}", cmd));
                self.execute_bash_command(cmd);
            }
            self.dirty = true;
            self.auto_scroll();
        } else if content.starts_with('/') {
            if content == "/" {
                // If palette is showing, pick the highlighted command
                if self.showing_command_palette {
                    if let Some(command) = self.command_palette.state().selected_command() {
                        let cmd_name = command.name.clone();
                        self.showing_command_palette = false;
                        self.command_palette.hide();
                        self.command_palette.state_mut().clear_query();
                        // Execute the slash command directly (no need to re-enter)
                        self.input_handler.state.clear();
                        self.input_mode = self.input_handler.state.mode;
                        if let Err(e) = self.handle_slash_command(&cmd_name) {
                            self.add_system_message(format!("Command failed: {}", e));
                        }
                        self.dirty = true;
                        return Ok(());
                    }
                }
                // No palette or no selection — just show it
                self.showing_command_palette = true;
                self.command_palette.show();
                self.command_palette.state_mut().clear_query();
                self.input_handler.state.clear();
                self.dirty = true;
                return Ok(());
            }

            let content_clone = content.clone();
            if let Err(e) = self.handle_slash_command(&content_clone) {
                tracing::error!("Slash command failed: {}", e);
                let err_str = e.to_string();
                let user_msg =
                    if err_str.contains("not found") || err_str.contains("Unknown command") {
                        "Unknown command. Type /help for available commands."
                    } else {
                        // Show actual error so user can fix it
                        &*format!("Command failed: {}", err_str)
                    };
                self.add_system_message(user_msg.to_string());
            }
            self.dirty = true;
            self.auto_scroll();
        } else {
            let message_to_send = self.inject_memory_if_needed(&content);
            let message_to_send = if is_first_user_message {
                self.inject_shell_history_if_first_message(&message_to_send)
            } else {
                message_to_send
            };
            let workspace_context = self.workspace_context.clone();

            // Build conversation history from existing messages for multi-turn context
            let mut history = self.build_conversation_history();

            // If images were attached, replace the last user message (which has text-only content)
            // with a multi-content message that includes image blocks
            if !attached_images.is_empty() {
                use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
                use rustycode_llm::provider_v2::{ContentBlock, ImageSource};
                use rustycode_protocol::MessageContent;

                let mut blocks = vec![ContentBlock::text(&message_to_send)];

                for img in &attached_images {
                    match std::fs::read(&img.path) {
                        Ok(bytes) => {
                            let b64 = BASE64.encode(&bytes);
                            blocks.push(ContentBlock::image(ImageSource::base64(
                                &img.mime_type,
                                b64,
                            )));
                            tracing::info!(
                                "Attached image: {} ({} bytes, {})",
                                img.id,
                                bytes.len(),
                                img.mime_type
                            );
                        }
                        Err(e) => {
                            tracing::warn!("Failed to read image {}: {}", img.path.display(), e);
                            blocks.push(ContentBlock::text(format!(
                                "[Image {} could not be loaded: {}]",
                                img.id, e
                            )));
                        }
                    }
                }

                // Replace the last user message in history with the image-enriched version
                if let Some(last_msg) = history.last_mut() {
                    if last_msg.role == rustycode_llm::provider_v2::MessageRole::User {
                        last_msg.content = MessageContent::blocks(blocks);
                    }
                }
            }

            self.rate_limit.last_message = Some(message_to_send.clone());

            // Set streaming flag BEFORE sending to prevent double-Enter races.
            // If send fails, we clear it below.
            self.is_streaming = true;
            self.chunks_received = 0;
            self.stream_start_time = Some(std::time::Instant::now());
            self.current_stream_content.clear();
            self.streaming_render_buffer =
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new();

            if let Err(e) = self.services.send_message_with_history(
                message_to_send,
                workspace_context,
                Some(history),
            ) {
                tracing::error!("Failed to send message: {}", e);
                self.is_streaming = false;
                self.chunks_received = 0;
                self.current_stream_content.clear();
                self.streaming_render_buffer =
                    crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
                self.stream_start_time = None;
                self.active_tools.clear();

                // Keep the user message visible so they see what they typed.
                // Just add an error system message below it.
                let user_msg = if e.to_string().contains("not started") {
                    "⚠️  Service initializing - please try again in a moment".to_string()
                } else {
                    format!("⚠️  Send failed: {} - press Enter to retry", e)
                };
                self.add_system_message(user_msg);
                self.dirty = true;
                self.auto_scroll();
            } else {
                let assistant_msg = crate::ui::message::Message::assistant(String::new());
                self.messages.push(assistant_msg);
                self.dirty = true;
                self.auto_scroll();

                self.rate_limit.clear();
            }
        }

        Ok(())
    }

    /// Handle input action from input handler
    pub(crate) fn handle_input_action(
        &mut self,
        action: InputAction,
        key: crossterm::event::KeyEvent,
    ) -> Result<()> {
        match action {
            InputAction::OpenCommandPalette => {
                self.showing_skill_palette = false;
                self.skill_palette.close();
                self.showing_command_palette = true;
                self.command_palette.show();
                self.command_palette.state_mut().clear_query();
                self.dirty = true;
            }
            InputAction::OpenSkillPalette => {
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.showing_skill_palette = true;
                self.skill_palette.open();
                self.dirty = true;
            }
            InputAction::SendMessage(lines) => {
                self.process_send_message(lines)?;
            }
            InputAction::Consumed => {
                self.input_mode = self.input_handler.state.mode;

                // Auto-open command palette when user types '/' as first character
                let input_text = self.input_handler.state.all_text();
                if let Some(query) = input_text.strip_prefix('/') {
                    if query.contains(' ') || query.contains('\n') {
                        // User is past the command name (typing arguments) — close palette
                        if self.showing_command_palette {
                            self.showing_command_palette = false;
                            self.command_palette.hide();
                        }
                    } else {
                        self.showing_command_palette = true;
                        // Sync palette query from input text (don't call show() which resets)
                        let state = self.command_palette.state_mut();
                        if !state.visible {
                            state.visible = true;
                        }
                        state.query = query.to_string();
                        state.update_filtered();
                    }
                } else if self.showing_command_palette {
                    // Close palette if input no longer starts with /
                    self.showing_command_palette = false;
                    self.command_palette.hide();
                }

                self.dirty = true;
            }
            InputAction::Ignored => {
                self.handle_global_shortcut(key.code, key.modifiers)?;
            }
            InputAction::HistoryPrevious | InputAction::HistoryNext => {
                // History navigation is handled via InputAction::Consumed
                // in the input handler (Up/Down in single-line mode).
                self.dirty = true;
            }
            InputAction::SearchReverse => {
                self.dirty = true;
            }
            InputAction::RemoveImage(_) => {
                self.dirty = true;
            }
        }
        Ok(())
    }
}
