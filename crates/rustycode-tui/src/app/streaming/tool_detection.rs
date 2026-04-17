//! Tool use detection for LLM streaming
//!
//! This module handles detection and accumulation of tool use from LLM streams,
//! including both eager streaming (Anthropic) and delta-based streaming.

use super::events::extract_tool_input;
use super::{ActiveToolUse, ToolUseAction};

/// Handle start of content block - detects tool use
///
/// When a `content_block_start` event is received, this function checks if it's
/// a tool use block. If so, it sets the suppression flag and initializes the
/// active tool accumulation state.
///
/// # Arguments
/// * `content_block` - The content block type from the SSE event
/// * `in_tool_use` - Mutable flag to set when tool use is detected
/// * `active_tool` - Mutable reference to the active tool accumulator
///
/// # Eager vs Delta Streaming
///
/// Anthropic's API uses two modes for tool parameter streaming:
/// - **Eager streaming**: Complete parameters sent in `content_block_start` event's `input` field
/// - **Delta-based**: Parameters arrive incrementally via `ContentBlockDelta` events
///
/// This function handles both by checking for eager parameters and initializing
/// the accumulator for delta-based streaming if needed.
pub fn handle_content_block_start(
    content_block: rustycode_llm::ContentBlockType,
    in_tool_use: &mut bool,
    active_tool: &mut Option<ActiveToolUse>,
) {
    match content_block {
        rustycode_llm::ContentBlockType::ToolUse { id, name, input } => {
            // Tool use detected - suppress all content until block end
            *in_tool_use = true;
            tracing::info!(
                "Tool use started: {} ({}) - accumulating parameters",
                name,
                id
            );

            // Check if input is already provided (eager streaming)
            let partial_json = extract_tool_input(input);

            // Initialize active tool to accumulate parameters
            *active_tool = Some(ActiveToolUse {
                id,
                name,
                partial_json,
            });
        }
        _ => {
            // Other block types don't need special handling
            *in_tool_use = false;
        }
    }
}

/// Handle partial JSON (tool parameters)
///
/// Accumulates JSON chunks for the active tool. This is used in delta-based
/// streaming where parameters arrive piece by piece.
///
/// # Arguments
/// * `partial_json` - The JSON chunk to accumulate
/// * `active_tool` - Mutable reference to the active tool being accumulated
pub fn handle_partial_json(partial_json: String, active_tool: &mut Option<ActiveToolUse>) {
    // Accumulate JSON for the active tool
    if let Some(tool) = active_tool {
        tool.partial_json.push_str(&partial_json);
        tracing::debug!(
            "Accumulated tool parameters ({} chars total)",
            tool.partial_json.len()
        );
    } else {
        tracing::warn!("Received partial JSON but no active tool!");
    }
}

/// Handle message metadata (stop reason, usage) and return appropriate action
///
/// Determines what action to take based on the LLM's stop reason.
/// This controls conversation continuation flow.
///
/// # Arguments
/// * `stop_reason` - The stop reason from the LLM response
///
/// # Returns
/// * `ToolUseAction::ExecuteTools` - Execute tools and continue conversation
/// * `ToolUseAction::Stop` - Conversation is complete
/// * `ToolUseAction::ContinueServerTools` - Server-side tools requested
/// * `ToolUseAction::None` - No specific action
pub fn handle_message_delta(stop_reason: Option<&str>) -> ToolUseAction {
    match stop_reason {
        Some("tool_use") => {
            tracing::info!(
                "Stream stopped with reason: tool_use - will execute tools and continue"
            );
            ToolUseAction::ExecuteTools
        }
        Some("stop") => {
            tracing::info!("Stream stopped with reason: stop - conversation complete");
            ToolUseAction::Stop
        }
        Some("pause_turn") => {
            tracing::info!("Stream stopped with reason: pause_turn - server-side tools");
            ToolUseAction::ContinueServerTools
        }
        Some(other) => {
            tracing::debug!("Stream stopped with reason: {}", other);
            ToolUseAction::None
        }
        None => {
            tracing::debug!("Stream stopped without stop_reason");
            ToolUseAction::None
        }
    }
}

/// Check if text looks like a tool call (JSON patterns or XML-style tags)
///
/// This is a safety filter to prevent raw tool call JSON or XML from appearing
/// in the UI. It detects common patterns used by various LLM providers for
/// tool calls.
///
/// # Arguments
/// * `text` - The text to check
///
/// # Returns
/// * `true` if the text appears to be a tool call
/// * `false` otherwise
pub fn looks_like_tool_call(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    let trimmed_lower = text.trim().to_lowercase();

    trimmed_lower.starts_with('{')
        || trimmed_lower.starts_with("\"function\":")
        || trimmed_lower.starts_with("\"tool\":")
        || text_lower.contains("\"tool_use\"")
        || text_lower.contains("\"tooluse\"")
        || text_lower.contains("\"parameters\"")
        || text_lower.contains("\"argument\"")
        || text_lower.contains("\"input\"")
        || text_lower.contains("<read_file>")
        || text_lower.contains("<bash>")
        || text_lower.contains("<write_file>")
        || text_lower.contains("<grep>")
        || text_lower.contains("<edit>")
        // Check for common Anthropic tool call patterns
        || (trimmed_lower.starts_with('{') && text_lower.contains("\"name\""))
        // Check for OpenAI tool call format
        || (text_lower.contains("\"function\"") && text_lower.contains("\"name\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_tool_call() {
        assert!(looks_like_tool_call("{"));
        assert!(looks_like_tool_call("{\"name\": \"bash\""));
        assert!(looks_like_tool_call("\"tool_use\":"));
        assert!(looks_like_tool_call("\"function\":"));
        assert!(looks_like_tool_call("\"function\": {\"name\":"));
        assert!(looks_like_tool_call("\"parameters\":"));
        assert!(looks_like_tool_call("\"argument\":"));
        assert!(!looks_like_tool_call("Hello, world!"));
        assert!(!looks_like_tool_call("This is text"));
        assert!(!looks_like_tool_call("```code```"));
    }

    #[test]
    fn test_looks_like_tool_call_case_insensitive() {
        assert!(looks_like_tool_call("\"TOOL_USE\":"));
        assert!(looks_like_tool_call("\"Function\":"));
        assert!(looks_like_tool_call("\"PARAMETERS\":"));
    }
}
