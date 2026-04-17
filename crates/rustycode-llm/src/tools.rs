//! Tool definitions for LLM providers
//!
//! This module defines common tools that can be exposed to LLMs (Claude, GPT, etc.)
//! to enable them to execute actions like running commands, reading files, etc.

use serde::{Deserialize, Serialize};
use serde_json::json;

/// A tool definition that can be sent to LLM providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<serde_json::Value>>,
    /// Whether this is a server-side tool (executed by Anthropic)
    #[serde(skip_serializing)]
    pub is_server_tool: bool,
    /// Enable fine-grained tool streaming (Anthropic-specific)
    /// When true, tool parameters stream without buffering or JSON validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            examples: None,
            is_server_tool: false,
            eager_input_streaming: None,
        }
    }

    /// Add examples to the tool definition
    pub fn with_examples(mut self, examples: Vec<serde_json::Value>) -> Self {
        self.examples = Some(examples);
        self
    }

    /// Mark this as a server-side tool (executed by Anthropic, not locally)
    pub fn server_tool(mut self) -> Self {
        self.is_server_tool = true;
        self
    }

    /// Enable eager input streaming for this tool (Anthropic-specific)
    /// When enabled, tool parameters will stream without buffering or JSON validation
    /// This results in faster streaming with longer chunks and fewer word breaks
    pub fn with_eager_streaming(mut self) -> Self {
        self.eager_input_streaming = Some(true);
        self
    }
}

/// Bash/command execution tool
fn bash_tool() -> ToolDefinition {
    ToolDefinition::new(
        "bash",
        "Execute a bash/shell command in the current working directory. Use this to run commands, list files, search code, etc. Returns the command output (stdout and stderr).",
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The CLI command to execute. This should be valid for the current operating system. Ensure the command is properly formatted and does not contain any harmful instructions."
                }
            },
            "required": ["command"]
        }),
    ).with_examples(vec![
        json!({"command": "ls -la"}),
        json!({"command": "grep -r pattern src/"}),
        json!({"command": "cargo test"}),
    ])
}

/// File reading tool
fn read_file_tool() -> ToolDefinition {
    ToolDefinition::new(
        "read_file",
        "Request to read the contents of a file at the specified path. Use this when you need to examine the contents of an existing file you do not know the contents of, for example to analyze code, review text files, or extract information from configuration files. Automatically extracts raw text from PDF and DOCX files. May not be suitable for other types of binary files, as it returns the raw content as a string. Do NOT use this tool to list the contents of a directory.",
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path of the file to read (relative to the current working directory)"
                }
            },
            "required": ["path"]
        }),
    ).with_examples(vec![
        json!({"path": "README.md"}),
        json!({"path": "src/main.rs"}),
        json!({"path": "Cargo.toml"}),
    ])
}

/// File writing tool
fn write_file_tool() -> ToolDefinition {
    ToolDefinition::new(
        "write_file",
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does.",
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write (relative to current working directory or absolute)"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["path", "content"]
        }),
    ).with_examples(vec![
        json!({"path": "output.txt", "content": "Hello, world!"}),
        json!({"path": "src/config.rs", "content": "pub struct Config {\n    pub debug: bool,\n}"}),
        json!({"path": ".env", "content": "API_KEY=placeholder-key\nDEBUG=true"}),
    ])
}

/// Web fetch tool
fn web_fetch_tool() -> ToolDefinition {
    ToolDefinition::new(
        "web_fetch",
        "Fetch and read content from a web page or PDF. Use this to read documentation, blog posts, GitHub files, or online articles. Returns the page content as text.",
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from (e.g., 'https://docs.anthropic.com', 'https://github.com/user/repo/blob/main/README.md')"
                }
            },
            "required": ["url"]
        }),
    ).with_examples(vec![
        json!({"url": "https://docs.anthropic.com/en/docs/tool-use"}),
        json!({"url": "https://github.com/rust-lang/rust/blob/main/README.md"}),
        json!({"url": "https://www.anthropic.com/engineering/advanced-tool-use"}),
        json!({"url": "https://example.com/api/documentation"}),
    ])
}

/// Web search tool (server-side - executed by Anthropic)
fn web_search_tool() -> ToolDefinition {
    ToolDefinition::new(
        "web_search",
        "Search the web for current information, documentation, error solutions, or programming patterns. Returns relevant results with automatic citations.",
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query (e.g., 'Rust tokio spawn best practices', 'how to fix borrow checker error')"
                }
            },
            "required": ["query"]
        }),
    ).with_examples(vec![
        json!({"query": "Rust async spawn best practices"}),
        json!({"query": "Anthropic Claude tool use documentation"}),
        json!({"query": "how to fix cannot borrow as mutable"}),
        json!({"query": "Rust 2024 edition new features"}),
    ])
    .server_tool() // Mark as server tool
}

/// LSP diagnostics tool
fn lsp_diagnostics_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_diagnostics",
        "Get diagnostics (errors, warnings, hints) for a source file using the Language Server Protocol. Use this when user asks about errors, warnings, code quality, or 'check this file'. Requires an LSP server like rust-analyzer to be installed.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to analyze (e.g., 'src/main.rs', 'crates/rustycode-tui/src/lib.rs')"
                }
            },
            "required": ["file_path"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs"}),
        json!({"file_path": "crates/rustycode-tui/src/lib.rs"}),
        json!({"file_path": "/Users/nat/dev/rustycode/Cargo.toml"}),
    ])
}

/// LSP hover tool
fn lsp_hover_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_hover",
        "Get hover information (type signature, documentation) for a symbol at a specific position. Use this when user asks 'what is this', 'what type is', 'how does this work', or 'show me documentation for'.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-indexed, so line 1 is 0, line 10 is 9)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character position (0-indexed)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "line": 5, "character": 10}),
        json!({"file_path": "crates/rustycode-tui/src/lib.rs", "line": 15, "character": 8}),
    ])
}

/// LSP go to definition tool
fn lsp_definition_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_definition",
        "Find the definition of a symbol at a specific position. Use this when user asks 'where is this defined', 'find the definition', 'go to definition', or 'show me where this comes from'.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-indexed)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character position (0-indexed)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "line": 10, "character": 5}),
        json!({"file_path": "crates/rustycode-tui/src/lib.rs", "line": 20, "character": 12}),
    ])
}

/// LSP completion tool
fn lsp_completion_tool() -> ToolDefinition {
    ToolDefinition::new(
        "lsp_completion",
        "Get code completions at a specific position. Use this when user asks for completions, autocomplete, 'what can I use here', or 'show me available methods/functions'.",
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file (e.g., 'src/main.rs')"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-indexed)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character position (0-indexed)"
                }
            },
            "required": ["file_path", "line", "character"]
        }),
    ).with_examples(vec![
        json!({"file_path": "src/main.rs", "line": 15, "character": 20}),
        json!({"file_path": "crates/rustycode-tui/src/lib.rs", "line": 25, "character": 15}),
    ])
}

/// Get all available tools for the TUI
pub fn get_tui_tools() -> Vec<ToolDefinition> {
    vec![
        bash_tool(),
        read_file_tool(),
        write_file_tool(),
        web_fetch_tool(),
        web_search_tool(), // Server tool
        lsp_diagnostics_tool(),
        lsp_hover_tool(),
        lsp_definition_tool(),
        lsp_completion_tool(),
    ]
}

/// Convert tool definitions to the format expected by Anthropic API
pub fn to_anthropic_tools(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .filter(|tool| !tool.is_server_tool) // Don't send server tools - they're handled automatically by Anthropic
        .map(|tool| {
            let mut tool_json = json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema
            });

            // Include examples if present
            if let Some(ref examples) = tool.examples {
                if !examples.is_empty() {
                    tool_json["examples"] = json!(examples);
                }
            }

            // Include eager_input_streaming if enabled
            if let Some(eager) = tool.eager_input_streaming {
                if eager {
                    tool_json["eager_input_streaming"] = json!(true);
                }
            }

            tool_json
        })
        .collect()
}

/// Convert tool definitions to the format expected by OpenAI API
pub fn to_openai_tools(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_tool_definition() {
        let tools = get_tui_tools();
        let bash = tools.iter().find(|t| t.name == "bash").unwrap();

        assert_eq!(bash.name, "bash");
        assert!(bash.description.contains("bash"));
        assert!(bash.input_schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("command")));
    }

    #[test]
    fn test_anthropic_conversion() {
        let tools = get_tui_tools();
        let anthropic_tools = to_anthropic_tools(&tools);

        assert!(!anthropic_tools.is_empty());
        let bash_tool = anthropic_tools
            .iter()
            .find(|t| t["name"] == "bash")
            .unwrap();
        assert!(bash_tool["description"].is_string());
        assert!(bash_tool["input_schema"].is_object());
    }

    #[test]
    fn test_server_tools_filtered_out() {
        let tools = get_tui_tools();
        let anthropic_tools = to_anthropic_tools(&tools);

        // web_search is a server tool, should NOT be in the anthropic tools array
        let web_search_in_anthropic = anthropic_tools.iter().any(|t| t["name"] == "web_search");
        assert!(
            !web_search_in_anthropic,
            "Server tools should be filtered out"
        );

        // But it should be in the full tools list
        let web_search_in_full = tools.iter().any(|t| t.name == "web_search");
        assert!(
            web_search_in_full,
            "Server tools should be in full tools list"
        );
    }

    #[test]
    fn test_web_search_is_server_tool() {
        let tools = get_tui_tools();
        let web_search = tools.iter().find(|t| t.name == "web_search").unwrap();
        assert!(
            web_search.is_server_tool,
            "web_search should be marked as server tool"
        );

        // Local tools should not be marked as server tools
        let bash = tools.iter().find(|t| t.name == "bash").unwrap();
        assert!(!bash.is_server_tool, "bash should not be a server tool");
    }
}
