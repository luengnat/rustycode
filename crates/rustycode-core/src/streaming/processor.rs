//! Unified SSE event processor for streaming LLM responses
//!
//! This module provides a canonical SSE event dispatch implementation
//! that both headless and TUI can use, eliminating duplication of
//! ContentBlockStart/Delta/Stop matching logic.

use crate::streaming::tool_state::ToolAccumulator;
use rustycode_llm::provider_v2::{ContentBlockType, ContentDelta, SSEEvent};

/// Callbacks for SSE event handling
///
/// Implemented by headless and TUI to handle semantic results of events.
/// This is the seam between shared dispatch logic and domain-specific handling.
pub trait StreamingCallbacks {
    /// Called when a text delta arrives (not inside a tool use block)
    fn on_text(&mut self, text: &str);

    /// Called when a thinking delta arrives (not inside a tool use block)
    ///
    /// Default implementation is a no-op, since headless ignores thinking.
    /// TUI overrides this to send thinking to the UI.
    fn on_thinking(&mut self, _thinking: &str) {}

    /// Called when a tool use block starts (ContentBlockStart with ToolUse)
    ///
    /// Default implementation is a no-op. Headless overrides to print tool name.
    fn on_tool_start(&mut self, _id: &str, _name: &str) {}

    /// Called when a tool accumulator is complete (ContentBlockStop after ToolUse)
    fn on_tool_complete(&mut self, tool: ToolAccumulator);

    /// Called when a content block ends (ContentBlockStop for any block type)
    ///
    /// For text blocks, this is called instead of on_tool_complete.
    /// Default implementation is a no-op.
    fn on_content_block_stop(&mut self) {}

    /// Called when MessageDelta arrives with stop_reason and usage
    fn on_message_delta(
        &mut self,
        stop_reason: Option<&str>,
        usage: Option<&rustycode_llm::provider_v2::Usage>,
    );

    /// Called on MessageStop (end of streaming response)
    fn on_message_stop(&mut self);

    /// Called on error event
    fn on_error(&mut self, error_type: &str, message: &str);
}

/// Unified SSE event processor
///
/// Maintains state (in_tool_use flag, active_tool accumulator) and dispatches
/// SSE events to callbacks. Callers should create one instance per stream
/// and call process_event for each SSE event.
pub struct SseEventProcessor {
    in_tool_use: bool,
    active_tool: Option<ToolAccumulator>,
}

impl SseEventProcessor {
    /// Create a new processor
    pub fn new() -> Self {
        Self {
            in_tool_use: false,
            active_tool: None,
        }
    }

    /// Process one SSE event, calling appropriate callbacks
    ///
    /// # Returns
    /// - `Ok(true)` to continue processing
    /// - `Ok(false)` to stop (on MessageStop or Error)
    ///
    /// # Errors
    /// Returns error only for internal failures; SSE-level errors
    /// are passed to callbacks via `on_error`.
    pub fn process_event<C: StreamingCallbacks>(
        &mut self,
        event: SSEEvent,
        callbacks: &mut C,
    ) -> anyhow::Result<bool> {
        match event {
            SSEEvent::ContentBlockStart { content_block, .. } => match content_block {
                ContentBlockType::ToolUse { id, name, input } => {
                    self.in_tool_use = true;
                    let initial_json = extract_eager_tool_input(input);
                    callbacks.on_tool_start(&id, &name);
                    self.active_tool = Some(ToolAccumulator::new(id, name, initial_json));
                }
                ContentBlockType::Text { .. } => {
                    self.in_tool_use = false;
                }
                _ => {
                    self.in_tool_use = false;
                }
            },

            SSEEvent::ContentBlockDelta { delta, .. } => {
                match delta {
                    ContentDelta::Text { text } => {
                        if !self.in_tool_use {
                            callbacks.on_text(&text);
                        }
                    }
                    ContentDelta::Thinking { thinking } => {
                        if !self.in_tool_use {
                            callbacks.on_thinking(&thinking);
                        }
                    }
                    ContentDelta::PartialJson { partial_json } => {
                        if let Some(ref mut tool) = self.active_tool {
                            tool.push_json(&partial_json);
                        }
                    }
                    ContentDelta::Signature { .. } => {
                        // Extended thinking signature; ignore for now
                    }
                    ContentDelta::Citations { .. } => {
                        // Citation metadata; ignore for now
                    }
                    _ => {
                        // Future ContentDelta variants; ignore
                    }
                }
            }

            SSEEvent::ContentBlockStop { .. } => {
                self.in_tool_use = false;
                if let Some(tool) = self.active_tool.take() {
                    callbacks.on_tool_complete(tool);
                } else {
                    // Text or other block type ended
                    callbacks.on_content_block_stop();
                }
            }

            SSEEvent::MessageDelta { stop_reason, usage } => {
                callbacks.on_message_delta(stop_reason.as_deref(), usage.as_ref());
            }

            SSEEvent::MessageStop => {
                callbacks.on_message_stop();
                return Ok(false);
            }

            SSEEvent::Error {
                error_type,
                message,
            } => {
                callbacks.on_error(&error_type, &message);
                return Ok(false);
            }

            // Legacy plain-text events from non-SSE providers
            SSEEvent::Text { text } => {
                if !self.in_tool_use {
                    callbacks.on_text(&text);
                }
            }

            SSEEvent::ThinkingDelta { thinking } => {
                if !self.in_tool_use {
                    callbacks.on_thinking(&thinking);
                }
            }

            // Ignore these
            SSEEvent::Ping => {}
            SSEEvent::MessageStart { .. } => {}
            SSEEvent::SignatureDelta { .. } => {}
            _ => {}
        }

        Ok(true)
    }
}

impl Default for SseEventProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract eagerly-streamed tool input from ContentBlockStart
///
/// When Anthropic's API uses eager streaming, the complete tool parameters
/// are sent in the `input` field of ContentBlockStart. This function extracts
/// those parameters if present, or returns an empty string if parameters will
/// be streamed via delta events.
fn extract_eager_tool_input(input: Option<serde_json::Value>) -> String {
    match input {
        Some(input_value) if !is_empty_json_object(&input_value) => {
            serde_json::to_string(&input_value).unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// Check if a JSON value is an empty object
fn is_empty_json_object(value: &serde_json::Value) -> bool {
    value.as_object().map(|obj| obj.is_empty()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCallbacks {
        texts: Vec<String>,
        thinkings: Vec<String>,
        tools_completed: Vec<(String, String)>,
        stop_reasons: Vec<Option<String>>,
        errors: Vec<(String, String)>,
    }

    impl StreamingCallbacks for TestCallbacks {
        fn on_text(&mut self, text: &str) {
            self.texts.push(text.to_string());
        }

        fn on_thinking(&mut self, thinking: &str) {
            self.thinkings.push(thinking.to_string());
        }

        fn on_tool_start(&mut self, _id: &str, _name: &str) {
            // Test implementation: no-op
        }

        fn on_tool_complete(&mut self, tool: ToolAccumulator) {
            self.tools_completed.push((tool.id, tool.name));
        }

        fn on_content_block_stop(&mut self) {
            // Test implementation: no-op
        }

        fn on_message_delta(
            &mut self,
            stop_reason: Option<&str>,
            _usage: Option<&rustycode_llm::provider_v2::Usage>,
        ) {
            self.stop_reasons.push(stop_reason.map(String::from));
        }

        fn on_message_stop(&mut self) {}

        fn on_error(&mut self, error_type: &str, message: &str) {
            self.errors
                .push((error_type.to_string(), message.to_string()));
        }
    }

    #[test]
    fn test_text_event() {
        let mut processor = SseEventProcessor::new();
        let mut callbacks = TestCallbacks {
            texts: vec![],
            thinkings: vec![],
            tools_completed: vec![],
            stop_reasons: vec![],
            errors: vec![],
        };

        let event = SSEEvent::Text {
            text: "Hello".to_string(),
        };

        let should_continue = processor.process_event(event, &mut callbacks).unwrap();
        assert!(should_continue);
        assert_eq!(callbacks.texts, vec!["Hello"]);
    }

    #[test]
    fn test_thinking_event() {
        let mut processor = SseEventProcessor::new();
        let mut callbacks = TestCallbacks {
            texts: vec![],
            thinkings: vec![],
            tools_completed: vec![],
            stop_reasons: vec![],
            errors: vec![],
        };

        let event = SSEEvent::ThinkingDelta {
            thinking: "Reasoning...".to_string(),
        };

        let should_continue = processor.process_event(event, &mut callbacks).unwrap();
        assert!(should_continue);
        assert_eq!(callbacks.thinkings, vec!["Reasoning..."]);
    }

    #[test]
    fn test_tool_accumulation() {
        let mut processor = SseEventProcessor::new();
        let mut callbacks = TestCallbacks {
            texts: vec![],
            thinkings: vec![],
            tools_completed: vec![],
            stop_reasons: vec![],
            errors: vec![],
        };

        // Start tool
        let start_event = SSEEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlockType::ToolUse {
                id: "call_1".to_string(),
                name: "read_file".to_string(),
                input: None,
            },
        };
        processor
            .process_event(start_event, &mut callbacks)
            .unwrap();

        // Delta 1
        let delta1 = SSEEvent::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::PartialJson {
                partial_json: r#"{"path":""#.to_string(),
            },
        };
        processor.process_event(delta1, &mut callbacks).unwrap();

        // Delta 2
        let delta2 = SSEEvent::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::PartialJson {
                partial_json: r#"/tmp/test.txt"}"#.to_string(),
            },
        };
        processor.process_event(delta2, &mut callbacks).unwrap();

        // Stop
        let stop_event = SSEEvent::ContentBlockStop { index: 0 };
        processor.process_event(stop_event, &mut callbacks).unwrap();

        assert_eq!(callbacks.tools_completed.len(), 1);
        assert_eq!(callbacks.tools_completed[0].0, "call_1");
        assert_eq!(callbacks.tools_completed[0].1, "read_file");
    }

    #[test]
    fn test_message_stop() {
        let mut processor = SseEventProcessor::new();
        let mut callbacks = TestCallbacks {
            texts: vec![],
            thinkings: vec![],
            tools_completed: vec![],
            stop_reasons: vec![],
            errors: vec![],
        };

        let event = SSEEvent::MessageStop;
        let should_continue = processor.process_event(event, &mut callbacks).unwrap();
        assert!(!should_continue);
    }

    #[test]
    fn test_error_stops_processing() {
        let mut processor = SseEventProcessor::new();
        let mut callbacks = TestCallbacks {
            texts: vec![],
            thinkings: vec![],
            tools_completed: vec![],
            stop_reasons: vec![],
            errors: vec![],
        };

        let event = SSEEvent::Error {
            error_type: "timeout".to_string(),
            message: "Stream timeout".to_string(),
        };
        let should_continue = processor.process_event(event, &mut callbacks).unwrap();
        assert!(!should_continue);
        assert_eq!(callbacks.errors.len(), 1);
        assert_eq!(callbacks.errors[0].0, "timeout");
    }

    #[test]
    fn test_text_suppressed_during_tool_use() {
        let mut processor = SseEventProcessor::new();
        let mut callbacks = TestCallbacks {
            texts: vec![],
            thinkings: vec![],
            tools_completed: vec![],
            stop_reasons: vec![],
            errors: vec![],
        };

        // Start tool
        let start = SSEEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlockType::ToolUse {
                id: "id".to_string(),
                name: "tool".to_string(),
                input: None,
            },
        };
        processor.process_event(start, &mut callbacks).unwrap();

        // Text during tool use should not call on_text
        let text_event = SSEEvent::Text {
            text: "ignored".to_string(),
        };
        processor.process_event(text_event, &mut callbacks).unwrap();
        assert!(callbacks.texts.is_empty());
    }
}
