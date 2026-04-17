//! Shared conversation/runtime helpers for streamed LLM interactions
//!
//! This module provides common utilities for handling streaming LLM conversations,
//! particularly for tool use scenarios. It abstracts the complexity of streaming
//! response handling and tool call accumulation.
//!
//! # Key Features
//!
//! - **Tool streaming support**: Handles both eager and delta-based parameter streaming
//! - **Message accumulation**: Collects assistant responses and tool calls from streams
//! - **Tool registry integration**: Converts tool registry to LLM-compatible schemas
//! - **Conversation management**: Helpers for building conversation history
//!
//! # Tool Streaming Modes
//!
//! ## Eager Streaming (Anthropic)
//! When Anthropic's API uses eager streaming, complete tool parameters are sent
//! in the `content_block_start` event's `input` field. This is more efficient as
//! no additional delta events are needed.
//!
//! Example:
//! ```text
//! ContentBlockStart { type: "tool_use", name: "read_file", input: {"path": "/tmp/test"} }
//! ContentBlockStop
//! ```
//!
//! ## Delta-Based Streaming
//! Traditional streaming where tool parameters arrive incrementally via
//! `ContentBlockDelta` events with `PartialJson` content.
//!
//! Example:
//! ```text
//! ContentBlockStart { type: "tool_use", name: "read_file", input: {} }
//! ContentBlockDelta { partial_json: '{"path":' }
//! ContentBlockDelta { partial_json: '"/tmp/test"}' }
//! ContentBlockStop
//! ```
//!
//! # Usage Example
//!
//! ```rust,no_run,ignore
//! use rustycode_orchestra::conversation_runtime::*;
//! use rustycode_llm::{ChatMessage, CompletionRequest};
//!
//! // Build request with tools
//! let tools = tool_schemas(&tool_registry);
//! let request = CompletionRequest::new(model, messages).with_tools(tools);
//!
//! // Stream and collect tool calls
//! let (response, tool_calls) = stream_tool_round(&provider, request).await?;
//!
//! // Execute tools and continue conversation
//! let results = execute_tools(tool_calls).await;
//! append_assistant_and_tool_results(&mut messages, response, results);
//! ```

use std::sync::Arc;

use rustycode_llm::{ChatMessage, CompletionRequest, ContentDelta, LLMProvider};
use rustycode_tools::ToolRegistry;
use tracing::{debug, warn};

/// Active tool use being accumulated during streaming
///
/// Tracks the state of a tool use that is currently being received from the
/// LLM stream. This struct accumulates parameters either via eager streaming
/// (complete at start) or delta-based streaming (accumulated via chunks).
///
/// # Fields
///
/// * `id` - Unique identifier for this tool use instance (e.g., "toolu_123")
/// * `name` - Name of the tool being invoked (e.g., "read_file", "bash")
/// * `partial_json` - Accumulated JSON parameters for the tool call
///
/// # Example
///
/// ```rust,no_run,ignore
/// let active_tool = ActiveToolUse {
///     id: "toolu_123".to_string(),
///     name: "write_file".to_string(),
///     partial_json: r#"{"path": "/tmp/test"}"#.to_string(),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct ActiveToolUse {
    pub id: String,
    pub name: String,
    pub partial_json: String,
}

/// Pending tool call ready for execution
///
/// Represents a fully accumulated tool call that is ready to be executed.
/// This is the output of the streaming process.
///
/// # Fields
///
/// * `id` - Unique identifier matching the original tool use request
/// * `name` - Name of the tool to execute
/// * `input_json` - Complete JSON parameters for the tool call
///
/// # Example
///
/// ```rust,no_run,ignore
/// let pending = PendingToolCall {
///     id: "toolu_123".to_string(),
///     name: "read_file".to_string(),
///     input_json: r#"{"path": "/tmp/test.txt"}"#.to_string(),
/// };
///
/// // Execute the tool
/// let result = execute_tool(&pending.name, &pending.input_json).await;
/// ```
#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub input_json: String,
}

/// Convert a list of chat messages to a debug string representation
///
/// This is primarily used for logging and debugging purposes.
///
/// # Arguments
/// * `messages` - Slice of chat messages to convert
///
/// # Returns
/// * String containing debug representation of all messages
///
/// # Example
///
/// ```rust,no_run,ignore
/// let messages = vec![
///     ChatMessage::system("You are a helpful assistant"),
///     ChatMessage::user("Hello!"),
/// ];
///
/// let debug_text = messages_to_text(&messages);
/// println!("Conversation:\n{}", debug_text);
/// ```
pub fn messages_to_text(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|message| format!("{:?}", message))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert tool registry to LLM-compatible tool schemas
///
/// Transforms the internal tool registry format into the JSON schema format
/// expected by LLM providers (Anthropic, OpenAI, etc.).
///
/// # Arguments
/// * `tool_registry` - The tool registry containing available tools
///
/// # Returns
/// * Vector of JSON values representing tool schemas
///
/// # Schema Format
///
/// Each tool schema includes:
/// - `name`: Tool identifier
/// - `description`: Human-readable description
/// - `input_schema`: JSON schema for tool parameters
///
/// # Example
///
/// ```rust,no_run,ignore
/// let tools = tool_schemas(&tool_registry);
/// let request = CompletionRequest::new(model, messages)
///     .with_tools(tools);
/// ```
pub fn tool_schemas(tool_registry: &ToolRegistry) -> Vec<serde_json::Value> {
    tool_registry
        .list()
        .into_iter()
        .map(|tool_info| {
            serde_json::json!({
                "name": tool_info.name,
                "description": tool_info.description,
                "input_schema": tool_info.parameters_schema
            })
        })
        .collect()
}

/// Append assistant response and tool results to message history
///
/// Helper function to add the assistant's response and subsequent tool results
/// to the conversation history. This is used after a tool round to prepare
/// for the next LLM call.
///
/// # Arguments
/// * `messages` - Mutable vector of messages to append to
/// * `assistant_response` - The assistant's text response
/// * `tool_results` - Vector of (tool_id, result) tuples
///
/// # Example
///
/// ```rust,no_run,ignore
/// let mut messages = vec![ChatMessage::user("Read the file")];
///
/// // After streaming and executing tools
/// append_assistant_and_tool_results(
///     &mut messages,
///     "I'll read that file for you".to_string(),
///     vec![("toolu_123", "File contents here...".to_string())],
/// );
///
/// // messages now contains user message, assistant response, and tool result
/// ```
pub fn append_assistant_and_tool_results(
    messages: &mut Vec<ChatMessage>,
    assistant_response: String,
    tool_results: Vec<(String, String)>,
) {
    messages.push(ChatMessage::assistant(assistant_response));
    for (tool_id, result) in tool_results {
        messages.push(ChatMessage::tool_result(result, tool_id));
    }
}

/// Stream a text-only round (no tools)
///
/// Streams an LLM response and accumulates the text content.
/// This is a simplified version for conversations without tool use.
///
/// # Arguments
/// * `provider` - The LLM provider to use
/// * `request` - The completion request
///
/// # Returns
/// * `Ok(String)` - The accumulated assistant response
/// * `Err` - If streaming fails
///
/// # Output
///
/// Text content is printed to stdout as it arrives (for real-time feedback).
///
/// # Example
///
/// ```rust,no_run,ignore
/// let request = CompletionRequest::new(model, messages);
/// let response = stream_text_round(&provider, request).await?;
/// println!("\nFull response: {}", response);
/// ```
pub async fn stream_text_round(
    provider: &Arc<dyn LLMProvider>,
    request: CompletionRequest,
) -> anyhow::Result<String> {
    let mut stream = provider.complete_stream(request).await?;
    let mut assistant_response = String::new();

    while let Some(chunk_result) = futures::StreamExt::next(&mut stream).await {
        match chunk_result {
            Ok(event) => match event {
                rustycode_llm::SSEEvent::ContentBlockDelta {
                    delta: ContentDelta::Text { text },
                    ..
                } => {
                    assistant_response.push_str(&text);
                    print!("{}", text);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
                rustycode_llm::SSEEvent::MessageStop => break,
                _ => {}
            },
            Err(e) => {
                warn!("Stream error: {:?}", e);
                break;
            }
        }
    }

    Ok(assistant_response)
}

/// Stream a tool-enabled round and collect tool calls
///
/// Streams an LLM response with tool support, accumulating both the assistant's
/// text response and any tool calls requested. Handles both eager and delta-based
/// parameter streaming.
///
/// # Arguments
/// * `provider` - The LLM provider to use
/// * `request` - The completion request (should include tools schema)
///
/// # Returns
/// * `Ok((String, Vec<PendingToolCall>))` - Tuple of (assistant_response, tool_calls)
/// * `Err` - If streaming fails
///
/// # Eager Streaming Support
///
/// This function automatically detects and handles eager streaming where tool
/// parameters arrive complete in the `ContentBlockStart` event. When the `input`
/// field contains non-empty parameters, they are captured immediately without
/// waiting for delta events.
///
/// # Example
///
/// ```rust,no_run,ignore
/// let tools = tool_schemas(&registry);
/// let request = CompletionRequest::new(model, messages)
///     .with_tools(tools);
///
/// let (response, tool_calls) = stream_tool_round(&provider, request).await?;
///
/// // Execute collected tools
/// for tool_call in &tool_calls {
///     println!("Tool: {} with params: {}",
///         tool_call.name,
///         tool_call.input_json
///     );
/// }
/// ```
pub async fn stream_tool_round(
    provider: &Arc<dyn LLMProvider>,
    request: CompletionRequest,
) -> anyhow::Result<(String, Vec<PendingToolCall>)> {
    let mut stream = provider.complete_stream(request).await?;
    let mut assistant_response = String::new();
    let mut tool_calls = Vec::new();
    let mut active_tool: Option<ActiveToolUse> = None;

    debug!("\n🤖 LLM Response:");

    while let Some(chunk_result) = futures::StreamExt::next(&mut stream).await {
        match chunk_result {
            Ok(event) => match event {
                rustycode_llm::SSEEvent::ContentBlockStart {
                    content_block: rustycode_llm::ContentBlockType::ToolUse { id, name, input },
                    ..
                } => {
                    let partial_json = match input {
                        Some(input_value)
                            if !input_value
                                .as_object()
                                .map(|o| o.is_empty())
                                .unwrap_or(false) =>
                        {
                            debug!(
                                "Received tool parameters via eager streaming ({} chars)",
                                input_value.to_string().len()
                            );
                            input_value.to_string()
                        }
                        _ => String::new(),
                    };

                    active_tool = Some(ActiveToolUse {
                        id,
                        name,
                        partial_json,
                    });
                }
                // Handle content deltas - text or partial JSON
                rustycode_llm::SSEEvent::ContentBlockDelta { delta, .. } => match &delta {
                    // Accumulate partial JSON for delta-based streaming
                    ContentDelta::PartialJson { partial_json } => {
                        if let Some(tool) = &mut active_tool {
                            tool.partial_json.push_str(partial_json);
                        }
                    }
                    // Accumulate text response
                    ContentDelta::Text { text } => {
                        assistant_response.push_str(text);
                        debug!("{}", text);
                    }
                    _ => {}
                },
                // Handle end of content block - finalize tool call
                rustycode_llm::SSEEvent::ContentBlockStop { .. } => {
                    if let Some(tool) = active_tool.take() {
                        println!("\n🔧 Tool requested: {}", tool.name);
                        tool_calls.push(PendingToolCall {
                            id: tool.id,
                            name: tool.name,
                            input_json: tool.partial_json,
                        });
                    }
                }
                rustycode_llm::SSEEvent::MessageStop => break,
                _ => {}
            },
            Err(e) => {
                warn!("Stream error: {:?}", e);
                break;
            }
        }
    }

    debug!("Stream complete");
    Ok((assistant_response, tool_calls))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_to_text_includes_message_content() {
        let messages = vec![
            ChatMessage::system("system prompt"),
            ChatMessage::user("user prompt"),
        ];

        let text = messages_to_text(&messages);

        assert!(text.contains("system prompt"));
        assert!(text.contains("user prompt"));
    }

    #[test]
    fn append_assistant_and_tool_results_appends_all_messages() {
        let mut messages = vec![ChatMessage::user("start")];

        append_assistant_and_tool_results(
            &mut messages,
            "assistant reply".to_string(),
            vec![("tool-1".to_string(), "tool output".to_string())],
        );

        assert_eq!(messages.len(), 3);

        let rendered = messages_to_text(&messages);
        assert!(rendered.contains("assistant reply"));
        assert!(rendered.contains("tool output"));
    }
}
