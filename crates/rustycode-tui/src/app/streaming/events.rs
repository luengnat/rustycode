//! SSE event handling for LLM streaming
//!
//! This module handles individual SSE (Server-Sent Events) from the LLM provider,
//! including text, thinking, and metadata events.

use anyhow::Result;
use std::sync::mpsc::SyncSender;

use super::tool_detection::{
    handle_content_block_start, handle_partial_json, looks_like_tool_call,
};
use super::ActiveToolUse;
use crate::app::async_::StreamChunk;

/// Handle a single SSE event, returning false if streaming should stop
///
/// This is a secondary event handler for events not handled in the main loop.
/// Most events are now handled directly in `stream_llm_response` for better
/// control over the conversation flow.
///
/// # Arguments
/// * `event` - The SSE event to handle
/// * `in_tool_use` - Mutable flag tracking if we're inside a tool use block
/// * `active_tool` - Mutable reference to the currently accumulating tool
/// * `stream_tx` - Channel sender for stream chunks
///
/// # Returns
/// * `Ok(true)` - Continue streaming
/// * `Ok(false)` - Stop streaming (MessageStop received)
pub fn handle_sse_event(
    event: rustycode_llm::SSEEvent,
    in_tool_use: &mut bool,
    active_tool: &mut Option<ActiveToolUse>,
    stream_tx: &SyncSender<StreamChunk>,
) -> Result<bool> {
    use rustycode_llm::SSEEvent;

    match event {
        SSEEvent::Text { text }
        | SSEEvent::ContentBlockDelta {
            delta: rustycode_llm::ContentDelta::Text { text },
            ..
        } => {
            let _ = handle_text_event(text, in_tool_use, stream_tx);
        }
        SSEEvent::ThinkingDelta { thinking }
        | SSEEvent::ContentBlockDelta {
            delta: rustycode_llm::ContentDelta::Thinking { thinking },
            ..
        } => {
            let _ = handle_thinking_event(thinking, in_tool_use, stream_tx);
        }
        SSEEvent::ContentBlockStart { content_block, .. } => {
            handle_content_block_start(content_block, in_tool_use, active_tool);
        }
        SSEEvent::ContentBlockDelta {
            delta: rustycode_llm::ContentDelta::PartialJson { partial_json },
            ..
        } => {
            handle_partial_json(partial_json, active_tool);
        }
        SSEEvent::ContentBlockStop { .. } => {
            *in_tool_use = false;
        }
        SSEEvent::MessageDelta {
            stop_reason,
            usage: _,
        } => {
            if let Some(reason) = stop_reason {
                tracing::debug!("Stream stop reason: {}", reason);
            }
        }
        SSEEvent::MessageStop => {
            let _ = stream_tx.send(StreamChunk::Done);
            return Ok(false);
        }
        SSEEvent::Ping => {
            // Keep-alive, ignore
        }
        SSEEvent::MessageStart {
            message_id,
            message_type,
            role,
        } => {
            tracing::debug!(
                "Message start: id={}, type={}, role={}",
                message_id,
                message_type,
                role
            );
        }
        SSEEvent::Error {
            error_type,
            message,
        } => {
            let _ = stream_tx.send(StreamChunk::Error(format!("{}: {}", error_type, message)));
            return Ok(false);
        }
        _ => {
            tracing::warn!(
                "Unhandled SSE event: {:?}, full event: {:?}",
                std::mem::discriminant(&event),
                event
            );
        }
    }

    Ok(true)
}

/// Handle text content from the stream
///
/// Filters out text that appears to be tool calls to prevent raw JSON
/// from appearing in the UI. Only sends text to TUI when it's actual
/// assistant message content.
///
/// # Arguments
/// * `text` - The text content to handle
/// * `in_tool_use` - Flag indicating if we're inside a tool use block
/// * `stream_tx` - Channel sender for stream chunks
pub fn handle_text_event(
    text: String,
    in_tool_use: &mut bool,
    stream_tx: &SyncSender<StreamChunk>,
) -> Result<()> {
    // Suppress ALL text if we're inside a tool use block
    if *in_tool_use {
        tracing::debug!(
            "Suppressing text inside tool use block: {} chars",
            text.len()
        );
        return Ok(());
    }

    // Filter out text that looks like tool calls (JSON patterns)
    if looks_like_tool_call(&text) {
        tracing::debug!("Filtered tool call text: {} chars", text.len());
        return Ok(());
    }

    // Send text chunk to TUI
    if stream_tx.send(StreamChunk::Text(text)).is_err() {
        tracing::debug!("Channel closed while sending text");
    }

    Ok(())
}

/// Handle thinking/reasoning content
///
/// Sends thinking content to the TUI with a [thinking] prefix.
/// Thinking content is suppressed during tool use to maintain clean output.
///
/// # Arguments
/// * `thinking` - The thinking content to handle
/// * `in_tool_use` - Flag indicating if we're inside a tool use block
/// * `stream_tx` - Channel sender for stream chunks
pub fn handle_thinking_event(
    thinking: String,
    in_tool_use: &bool,
    stream_tx: &SyncSender<StreamChunk>,
) -> Result<()> {
    // Skip thinking content while inside a tool use block
    if *in_tool_use {
        return Ok(());
    }

    // Send thinking as a dedicated chunk so the TUI can render it separately
    if stream_tx.send(StreamChunk::Thinking(thinking)).is_err() {
        tracing::debug!("Channel closed while sending thinking");
    }

    Ok(())
}

/// Check if a JSON value represents an empty object
///
/// Used to determine if tool parameters were provided via eager streaming.
/// An empty object `{}` indicates no eager parameters (will use delta streaming).
///
/// # Arguments
/// * `value` - The JSON value to check
///
/// # Returns
/// * `true` if the value is an empty object
/// * `false` otherwise (including non-objects)
pub fn is_empty_json_object(value: &serde_json::Value) -> bool {
    value.as_object().map(|obj| obj.is_empty()).unwrap_or(false)
}

/// Extract tool input from eager streaming or return empty string
///
/// When Anthropic's API uses eager streaming, the complete tool parameters
/// are sent in the `content_block_start` event's `input` field. This function
/// extracts those parameters if present, or returns an empty string if
/// parameters will be streamed via delta events.
///
/// # Arguments
/// * `input` - Optional JSON value from the content block
///
/// # Returns
/// * JSON string of tool parameters, or empty string if no parameters
pub fn extract_tool_input(input: Option<serde_json::Value>) -> String {
    match input {
        Some(input_value) if !is_empty_json_object(&input_value) => {
            tracing::info!(
                "Received tool parameters via eager streaming ({} chars)",
                input_value.to_string().len()
            );
            serde_json::to_string(&input_value).unwrap_or_default()
        }
        _ => String::new(),
    }
}
