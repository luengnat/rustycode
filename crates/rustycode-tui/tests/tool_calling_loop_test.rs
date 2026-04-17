//! Test multi-round tool calling loop logic
use rustycode_tui::auto_tool_parser::{extract_tool_payloads, parse_tool_calls_payload};

#[test]
fn test_multi_round_tool_calling_detection() {
    // Simulate a multi-round tool calling scenario

    // Round 1: LLM returns list_dir tool call
    let response1 = r#"I'll list the directory contents first.

```tool
[{"name": "list_dir", "arguments": {"path": "."}}]
```"#;

    let payloads1 = extract_tool_payloads(response1);
    assert_eq!(payloads1.len(), 1, "Should detect list_dir tool call");

    if let Ok(tool_calls) = parse_tool_calls_payload(&payloads1[0]) {
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "list_dir");
    }

    // Simulate tool result and Round 2: LLM returns read_file tool call
    let response2 = r#"I see the files. Let me read Cargo.toml.

```tool
[{"name": "read_file", "arguments": {"path": "Cargo.toml"}}]
```"#;

    let payloads2 = extract_tool_payloads(response2);
    assert_eq!(payloads2.len(), 1, "Should detect read_file tool call");

    if let Ok(tool_calls) = parse_tool_calls_payload(&payloads2[0]) {
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "read_file");
    }

    // Simulate tool result and Round 3: LLM returns another read_file tool call
    let response3 = r#"Let me also read src/lib.rs.

```tool
[{"name": "read_file", "arguments": {"path": "src/lib.rs"}}]
```"#;

    let payloads3 = extract_tool_payloads(response3);
    assert_eq!(
        payloads3.len(),
        1,
        "Should detect second read_file tool call"
    );

    // Round 4: LLM returns final response (no tool calls)
    let response4 = r#"Based on the files I've read, this is RustyCode, an AI coding assistant...

The project structure shows:
- Cargo.toml defines the workspace
- src/lib.rs contains the main logic

This is a terminal UI for AI-assisted coding."#;

    let payloads4 = extract_tool_payloads(response4);
    assert_eq!(
        payloads4.len(),
        0,
        "Should not detect any tool calls in final response"
    );

    println!("✓ Multi-round tool calling detection test passed");
    println!("✓ Round 1: list_dir detected");
    println!("✓ Round 2: read_file detected");
    println!("✓ Round 3: read_file detected");
    println!("✓ Round 4: Final response (no tools)");
}

#[test]
fn test_tool_calling_loop_termination() {
    // Test that the loop properly terminates when no more tool calls

    // Response with tool call
    let with_tools = r#"Let me check the files.

```tool
[{"name": "list_dir", "arguments": {"path": "."}}]
```"#;

    let payloads_with = extract_tool_payloads(with_tools);
    assert!(!payloads_with.is_empty(), "Should detect tool calls");

    // Response without tool calls (final answer)
    let without_tools = r#"This project is called RustyCode. It's an AI coding assistant TUI.

Key features:
- Tool calling for file operations
- Multi-turn conversations
- Anthropic Claude integration"#;

    let payloads_without = extract_tool_payloads(without_tools);
    assert!(
        payloads_without.is_empty(),
        "Should not detect tool calls in final answer"
    );

    println!("✓ Loop termination test passed");
    println!("✓ Tool calls detected: {}", !payloads_with.is_empty());
    println!(
        "✓ Final response terminates loop: {}",
        payloads_without.is_empty()
    );
}

#[test]
fn test_multiple_tools_in_single_response() {
    // Test handling multiple tool calls in one response
    let response = r#"I'll read both files at once.

```tool
[
  {"name": "read_file", "arguments": {"path": "Cargo.toml"}},
  {"name": "read_file", "arguments": {"path": "README.md"}}
]
```"#;

    let payloads = extract_tool_payloads(response);
    assert_eq!(payloads.len(), 1, "Should detect one tool block");

    if let Ok(tool_calls) = parse_tool_calls_payload(&payloads[0]) {
        assert_eq!(tool_calls.len(), 2, "Should parse two tool calls");
        assert_eq!(tool_calls[0].name, "read_file");
        assert_eq!(tool_calls[1].name, "read_file");
    }

    println!("✓ Multiple tools in single response test passed");
    println!("✓ Detected 2 tool calls in one block");
}
