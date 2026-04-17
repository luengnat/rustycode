# RustyCode MCP (Model Context Protocol)

Full async implementation of the Model Context Protocol (MCP) for Rust, enabling communication between AI assistants and external tools/resources via stdio.

## Features

- **Full MCP Protocol Implementation**: JSON-RPC 2.0 over stdio
- **Async/Await**: Built on Tokio for efficient async I/O
- **Tool Discovery & Calling**: Dynamic tool registration and execution
- **Resource Access**: File-like resource access with MIME type support
- **Prompt Templates**: Reusable prompt templates with argument substitution
- **Tool Proxying**: Delegate calls to external MCP servers
- **Integration**: Seamless integration with rustycode-tools

## Architecture

### Components

1. **Client** (`client.rs`): Connects to MCP servers and exposes their capabilities
2. **Server** (`server.rs`): Hosts tools and resources for AI assistants
3. **Transport** (`transport.rs`): JSON-RPC communication over stdio
4. **Protocol** (`protocol.rs`): JSON-RPC message types
5. **Types** (`types.rs`): MCP-specific data structures
6. **Proxy** (`proxy.rs`): Tool proxying for multi-server setups
7. **Testing** (`testing.rs`): Mock server and test utilities

## Usage

### Creating an MCP Server

```rust
use rustycode_mcp::{McpServer, McpServerConfig, McpContent, McpPromptContent, McpPromptMessage};
use rustycode_tools::ToolExecutor;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> rustycode_mcp::McpResult<()> {
    // Create tool executor
    let executor = ToolExecutor::new(PathBuf::from("."));

    // Create MCP server
    let config = McpServerConfig {
        server_name: "my-server".to_string(),
        server_version: "0.1.0".to_string(),
        enable_tools: true,
        enable_resources: true,
        enable_prompts: true,
        timeout_secs: 30,
    };

    let mut server = McpServer::new("my-server", config);

    // Register tool executor
    server.register_tool_executor(executor);

    // Register a resource
    server.register_resource(
        "example://hello",
        "Hello World",
        "A simple greeting",
        "text/plain",
        || Ok(vec![McpContent::Text { text: "Hello, World!".to_string() }])
    ).await;

    // Register a prompt template
    server.register_prompt(
        "code-review",
        "Code review template",
        |args| {
            let language = args
                .and_then(|a| a.get("language"))
                .and_then(|l| l.as_str())
                .unwrap_or("code");

            Ok(vec![McpPromptMessage {
                role: "user".to_string(),
                content: McpPromptContent::Text {
                    text: format!("Please review this {}", language),
                },
            }])
        },
    ).await;

    // Run server on stdio
    server.run_stdio().await?;

    Ok(())
}
```

### Creating an MCP Client

```rust
use rustycode_mcp::{McpClient, McpClientConfig};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create client
    let config = McpClientConfig::default();
    let mut client = McpClient::new(config);

    // Connect to server
    client.connect_stdio(
        "my-server",
        "/path/to/server",
        &["--option1"]
    ).await?;

    // List available tools
    let tools = client.list_tools("my-server").await?;
    for tool in tools {
        println!("Tool: {} - {}", tool.name, tool.description);
    }

    // Call a tool
    let result = client.call_tool(
        "my-server",
        "read_file",
        json!({"path": "/tmp/test.txt"})
    ).await?;

    println!("Result: {:?}", result);

    Ok(())
}
```

### Tool Proxying

```rust
use rustycode_mcp::proxy::{ProxyConfig, ToolProxy};

#[tokio::main]
async fn main() -> rustycode_mcp::McpResult<()> {
    let config = ProxyConfig {
        server_name: "external-server".to_string(),
        command: "/usr/local/bin/mcp-server".to_string(),
        args: vec![],
        tool_prefix: Some("external_".to_string()),
        cache_tools: true,
    };

    let proxy = ToolProxy::with_discovery(config).await?;
    let tools = proxy.get_tools().await;

    for tool in tools {
        println!("Proxied tool: {}", tool.name);
    }

    Ok(())
}
```

## Protocol Support

The implementation supports the following MCP methods:

### Initialization
- `initialize` - Protocol negotiation and capability exchange
- `notifications/initialized` - Ready notification

### Tools
- `tools/list` - List available tools
- `tools/call` - Execute a tool

### Resources
- `resources/list` - List available resources
- `resources/read` - Read resource contents

### Prompts
- `prompts/list` - List available prompts
- `prompts/get` - Get prompt with arguments

### Notifications
- `notifications/tools/list_changed` - Tool list changed
- `notifications/resources/list_changed` - Resource list changed
- `notifications/prompts/list_changed` - Prompt list changed

## Testing

Run the test suite:

```bash
cargo test -p rustycode-mcp
```

Run specific test modules:

```bash
cargo test -p rustycode-mcp --lib
cargo test -p rustycode-mcp --test mcp_protocol_tests
```

## Examples

See the `examples/` directory:

- `mcp_client_example.rs` - Basic client usage
- `mcp_server_example.rs` - Basic server with resources and prompts
- `mcp-test-server/` - Standalone test server implementation

## Integration with RustyCode

The MCP crate integrates seamlessly with `rustycode-tools`:

- All tools from `ToolExecutor` are automatically exposed via MCP
- Tool execution respects sandboxing and permission settings
- Event bus integration for tool execution events

## Error Handling

All MCP operations return `McpResult<T>` which can be:

- `Ok(T)` - Success
- `Err(McpError)` - Various error types:
  - `JsonRpcError` - JSON-RPC protocol errors
  - `TransportError` - I/O communication errors
  - `ProtocolError` - MCP protocol violations
  - `ToolNotFound` - Tool not found
  - `ResourceNotFound` - Resource not found
  - `InvalidRequest` - Invalid request parameters
  - `MethodNotFound` - Unknown method
  - `InternalError` - Internal server error
  - `Timeout` - Request timeout
  - `ConnectionClosed` - Connection lost

## License

MIT

## Contributing

Contributions welcome! Please ensure tests pass before submitting PRs.
