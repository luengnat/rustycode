//! Tests for Anthropic fine-grained tool streaming support
//! https://platform.claude.com/docs/en/agents-and-tools/tool-use/fine-grained-tool-streaming

use rustycode_llm::tools::{to_anthropic_tools, ToolDefinition};
use serde_json::json;

#[test]
fn test_tool_with_eager_streaming_enabled() {
    let tool = ToolDefinition::new(
        "bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_eager_streaming();

    assert_eq!(tool.name, "bash");
    assert_eq!(tool.eager_input_streaming, Some(true));
}

#[test]
fn test_tool_without_eager_streaming() {
    let tool = ToolDefinition::new(
        "read_file",
        "Read file contents",
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            }
        }),
    );

    assert_eq!(tool.name, "read_file");
    assert_eq!(tool.eager_input_streaming, None);
}

#[test]
fn test_anthropic_tool_conversion_with_eager_streaming() {
    let tools = vec![ToolDefinition::new(
        "bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_eager_streaming()];

    let anthropic_tools = to_anthropic_tools(&tools);
    assert_eq!(anthropic_tools.len(), 1);

    let bash_tool = &anthropic_tools[0];
    assert_eq!(bash_tool["name"], "bash");
    assert_eq!(bash_tool["eager_input_streaming"], true);
}

#[test]
fn test_anthropic_tool_conversion_without_eager_streaming() {
    let tools = vec![ToolDefinition::new(
        "read_file",
        "Read file contents",
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            }
        }),
    )];

    let anthropic_tools = to_anthropic_tools(&tools);
    assert_eq!(anthropic_tools.len(), 1);

    let read_file_tool = &anthropic_tools[0];
    assert_eq!(read_file_tool["name"], "read_file");
    // eager_input_streaming should not be present
    assert!(read_file_tool.get("eager_input_streaming").is_none());
}

#[test]
fn test_mixed_tools_with_and_without_eager_streaming() {
    let tools = vec![
        ToolDefinition::new(
            "bash",
            "Execute bash commands",
            json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                }
            }),
        )
        .with_eager_streaming(),
        ToolDefinition::new(
            "read_file",
            "Read file contents",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        ),
        ToolDefinition::new(
            "web_fetch",
            "Fetch web content",
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"}
                }
            }),
        )
        .with_eager_streaming(),
    ];

    let anthropic_tools = to_anthropic_tools(&tools);
    assert_eq!(anthropic_tools.len(), 3);

    // bash should have eager_input_streaming
    assert_eq!(anthropic_tools[0]["eager_input_streaming"], true);

    // read_file should not have eager_input_streaming
    assert!(anthropic_tools[1].get("eager_input_streaming").is_none());

    // web_fetch should have eager_input_streaming
    assert_eq!(anthropic_tools[2]["eager_input_streaming"], true);
}

#[test]
fn test_eager_streaming_with_examples() {
    let tool = ToolDefinition::new(
        "bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_examples(vec![
        json!({"command": "ls -la"}),
        json!({"command": "cargo test"}),
    ])
    .with_eager_streaming();

    assert!(tool.examples.is_some());
    assert_eq!(tool.examples.as_ref().unwrap().len(), 2);
    assert_eq!(tool.eager_input_streaming, Some(true));

    let anthropic_tools = to_anthropic_tools(&[tool]);
    let bash_tool = &anthropic_tools[0];

    assert_eq!(bash_tool["eager_input_streaming"], true);
    assert!(bash_tool.get("examples").is_some());
    assert_eq!(bash_tool["examples"].as_array().unwrap().len(), 2);
}

#[test]
fn test_server_tool_with_eager_streaming() {
    // Server tools shouldn't have eager_input_streaming in the output
    // since they're not sent in the tool definitions
    let tool = ToolDefinition::new(
        "web_search",
        "Search the web",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            }
        }),
    )
    .server_tool()
    .with_eager_streaming();

    assert!(tool.is_server_tool);
    assert_eq!(tool.eager_input_streaming, Some(true));

    // Server tools should be filtered out in conversion
    let anthropic_tools = to_anthropic_tools(&[tool]);
    assert_eq!(anthropic_tools.len(), 0);
}

#[test]
fn test_tool_builder_chaining() {
    // Test that builder methods can be chained in any order
    let tool = ToolDefinition::new(
        "bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_examples(vec![json!({"command": "ls"})])
    .with_eager_streaming();

    assert_eq!(tool.name, "bash");
    assert!(tool.examples.is_some());
    assert_eq!(tool.eager_input_streaming, Some(true));

    // Test reverse order
    let tool2 = ToolDefinition::new(
        "bash",
        "Execute bash commands",
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"}
            }
        }),
    )
    .with_eager_streaming()
    .with_examples(vec![json!({"command": "ls"})]);

    assert_eq!(tool2.name, "bash");
    assert!(tool2.examples.is_some());
    assert_eq!(tool2.eager_input_streaming, Some(true));
}
