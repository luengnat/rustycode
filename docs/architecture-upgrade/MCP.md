# MCP Integration Guide

## What is MCP?

The **Model Context Protocol (MCP)** is an open standard that enables AI assistants to interact with external tools, resources, and data sources through a unified interface. MCP provides:

- **Tool Discovery**: Dynamic tool enumeration and metadata
- **Tool Execution**: Standardized tool calling mechanism
- **Resource Access**: Read files, prompts, templates, and other resources
- **Server Management**: Lifecycle and health monitoring of MCP servers
- **Protocol Standardization**: Common interface for all tools

### Benefits

- **Extensibility**: Add tools without modifying core code
- **Standardization**: Uniform interface for all tools
- **Language Agnostic**: Tools can be written in any language
- **Enterprise Features**: Authentication, rate limiting, monitoring
- **Ecosystem**: Growing ecosystem of MCP servers

### Use Cases

- **File Operations**: Read, write, search files
- **Database Access**: Query and update databases
- **API Integration**: Call external APIs
- **Version Control**: Git operations
- **Build Tools**: Run builds, tests, linting
- **Custom Tools**: Domain-specific operations

## Server Management

### Starting MCP Servers

```rust
use rustycode_mcp::{McpClient, McpClientConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = McpClientConfig::default();
    let mut client = McpClient::new(config);

    // Start MCP server via stdio
    client.connect_stdio(
        "filesystem",
        "/usr/local/bin/mcp-filesystem-server",
        &["--allow-read", "/Users/nat/dev"]
    ).await?;

    // List available tools
    let tools = client.list_tools().await?;
    for tool in tools {
        println!("Tool: {} - {}", tool.name, tool.description);
    }

    Ok(())
}
```

### Server Lifecycle

```rust
use rustycode_mcp::McpClient;

async fn manage_server() -> anyhow::Result<()> {
    let mut client = McpClient::default();

    // Connect to server
    client.connect_stdio("my-server", "/path/to/server", &[]).await?;

    // Check server health
    let health = client.ping_server("my-server").await?;
    println!("Server status: {:?}", health);

    // Disconnect when done
    client.disconnect("my-server").await?;

    Ok(())
}
```

### Health Monitoring

```rust
use tokio::time::{interval, Duration};

async fn monitor_server(client: &McpClient, server_id: &str) {
    let mut ticker = interval(Duration::from_secs(30));

    loop {
        ticker.tick().await;

        match client.ping_server(server_id).await {
            Ok(health) => {
                if !health.is_healthy() {
                    eprintln!("Server {} is unhealthy!", server_id);
                    // Attempt recovery
                }
            }
            Err(e) => {
                eprintln!("Failed to ping server {}: {}", server_id, e);
            }
        }
    }
}
```

### Auto-Recovery

```rust
async fn auto_recovery(client: &McpClient, server_id: &str) -> anyhow::Result<()> {
    let mut retry_count = 0;
    let max_retries = 3;

    loop {
        match client.ping_server(server_id).await {
            Ok(_) => {
                // Server is healthy, reset retry count
                retry_count = 0;
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
            Err(_) if retry_count < max_retries => {
                // Attempt to reconnect
                retry_count += 1;
                eprintln!("Attempting to reconnect {} (attempt {})", server_id, retry_count);

                client.connect_stdio(
                    server_id,
                    "/path/to/server",
                    &[]
                ).await?;

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(e) => {
                // Max retries reached, give up
                return Err(e.into());
            }
        }
    }
}
```

## Tool Calling

### Tool Discovery

```rust
use rustycode_mcp::McpClient;

async fn discover_tools(client: &McpClient, server_id: &str) -> anyhow::Result<()> {
    // List all tools from a server
    let tools = client.list_tools_for_server(server_id).await?;

    for tool in tools {
        println!("=== {} ===", tool.name);
        println!("Description: {}", tool.description);
        println!("Input Schema: {}", tool.input_schema);

        if let Some(example) = tool.example {
            println!("Example: {}", example);
        }
    }

    Ok(())
}
```

### Calling Tools

```rust
use rustycode_mcp::McpClient;
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = McpClient::default();

    // Connect to filesystem server
    client.connect_stdio(
        "filesystem",
        "/usr/local/bin/mcp-filesystem-server",
        &[]
    ).await?;

    // Call a tool
    let result = client.call_tool(
        "read_file",
        json!({
            "path": "/Users/nat/dev/rustycode/Cargo.toml"
        })
    ).await?;

    println!("Result: {:?}", result);

    Ok(())
}
```

### Parallel Tool Execution

```rust
use futures::future::join_all;
use rustycode_mcp::McpClient;
use serde_json::json;

async fn parallel_tools(client: &McpClient) -> anyhow::Result<()> {
    let files = vec![
        "/Users/nat/dev/rustycode/Cargo.toml",
        "/Users/nat/dev/rustycode/README.md",
        "/Users/nat/dev/rustycode/LICENSE",
    ];

    // Execute tool calls in parallel
    let tasks: Vec<_> = files.into_iter()
        .map(|path| {
            client.call_tool(
                "read_file",
                json!({"path": path})
            )
        })
        .collect();

    let results = join_all(tasks).await?;

    for result in results {
        println!("{:?}", result?);
    }

    Ok(())
}
```

### Tool Error Handling

```rust
use rustycode_mcp::{McpError, McpClient};

async fn safe_tool_call(client: &McpClient, tool_name: &str, args: serde_json::Value) -> Result<String, String> {
    match client.call_tool(tool_name, args).await {
        Ok(result) => Ok(result.content),
        Err(McpError::ToolNotFound(tool)) => {
            Err(format!("Tool '{}' not found", tool))
        }
        Err(McpError::InvalidRequest(msg)) => {
            Err(format!("Invalid request: {}", msg))
        }
        Err(McpError::InternalError(msg)) => {
            Err(format!("Internal error: {}", msg))
        }
        Err(e) => {
            Err(format!("Unexpected error: {}", e))
        }
    }
}
```

## Resource Access

### Listing Resources

```rust
use rustycode_mcp::McpClient;

async fn list_resources(client: &McpClient, server_id: &str) -> anyhow::Result<()> {
    // List all resources from a server
    let resources = client.list_resources_for_server(server_id).await?;

    for resource in resources {
        println!("=== {} ===", resource.name);
        println!("URI: {}", resource.uri);
        println!("Description: {}", resource.description);
        println!("MIME Type: {}", resource.mime_type);
    }

    Ok(())
}
```

### Reading Resources

```rust
use rustycode_mcp::McpClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = McpClient::default();

    // Connect to server
    client.connect_stdio("my-server", "/path/to/server", &[]).await?;

    // Read a resource
    let content = client.read_resource("file:///Users/nat/dev/rustycode/README.md").await?;

    println!("Content:\n{}", content);

    Ok(())
}
```

### Resource Monitoring

```rust
use rustycode_mcp::McpClient;
use tokio::time::{interval, Duration};

async fn monitor_resource(client: &McpClient, uri: &str) {
    let mut ticker = interval(Duration::from_secs(60));
    let mut last_content = String::new();

    loop {
        ticker.tick().await;

        match client.read_resource(uri).await {
            Ok(content) => {
                if content != last_content {
                    println!("Resource changed: {}", uri);
                    last_content = content;
                }
            }
            Err(e) => {
                eprintln!("Failed to read resource {}: {}", uri, e);
            }
        }
    }
}
```

## Configuration

### Server Configuration

```json
{
  "features": {
    "mcp_servers": [
      {
        "id": "filesystem",
        "command": "/usr/local/bin/mcp-filesystem-server",
        "args": ["--allow-read", "/Users/nat/dev"],
        "enabled": true
      },
      {
        "id": "git",
        "command": "/usr/local/bin/mcp-git-server",
        "args": ["--repo", "/Users/nat/dev/rustycode"],
        "enabled": true
      }
    ]
  }
}
```

### Connection Pooling

```rust
use rustycode_mcp::{McpClient, McpClientConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = McpClientConfig {
        max_connections: 10,
        connection_timeout: std::time::Duration::from_secs(30),
        ..McpClientConfig::default()
    };

    let client = McpClient::new(config);

    // Connect to multiple servers
    for server_config in server_configs {
        client.connect_stdio(
            &server_config.id,
            &server_config.command,
            &server_config.args
        ).await?;
    }

    Ok(())
}
```

### Rate Limiting

```rust
use rustycode_mcp::{McpClient, McpClientConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = McpClientConfig {
        max_requests_per_second: 10,
        ..McpClientConfig::default()
    };

    let client = McpClient::new(config);

    // Client will automatically enforce rate limiting
    for i in 0..100 {
        client.call_tool("my_tool", json!({"id": i})).await?;
    }

    Ok(())
}
```

### Timeout Settings

```rust
use rustycode_mcp::{McpClient, McpClientConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = McpClientConfig {
        request_timeout: Duration::from_secs(10),
        connection_timeout: Duration::from_secs(5),
        ..McpClientConfig::default()
    };

    let client = McpClient::new(config);

    // Requests will timeout after 10 seconds
    match client.call_tool("slow_tool", json!({})).await {
        Ok(result) => println!("Success: {:?}", result),
        Err(rustycode_mcp::McpError::Timeout) => {
            eprintln!("Request timed out");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}
```

## Examples

### Basic Tool Calling

```rust
use rustycode_mcp::McpClient;
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = McpClient::default();

    // Connect to filesystem server
    client.connect_stdio(
        "filesystem",
        "mcp-filesystem-server",
        &[]
    ).await?;

    // Read a file
    let result = client.call_tool(
        "read_file",
        json!({"path": "/Users/nat/dev/rustycode/Cargo.toml"})
    ).await?;

    println!("File content:\n{}", result.content);

    Ok(())
}
```

### Parallel Operations

```rust
use futures::future::join_all;
use rustycode_mcp::McpClient;
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = McpClient::default();

    client.connect_stdio("filesystem", "mcp-filesystem-server", &[]).await?;

    // Read multiple files in parallel
    let files = vec!["Cargo.toml", "README.md", "LICENSE"];
    let tasks: Vec<_> = files.into_iter()
        .map(|file| {
            client.call_tool(
                "read_file",
                json!({"path": file})
            )
        })
        .collect();

    let results = join_all(tasks).await?;

    for result in results {
        println!("{:?}", result?);
    }

    Ok(())
}
```

### Error Recovery

```rust
use rustycode_mcp::{McpClient, McpError};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = McpClient::default();

    // Connect with retry logic
    let mut retry_count = 0;
    loop {
        match client.connect_stdio("my-server", "mcp-server", &[]).await {
            Ok(_) => break,
            Err(_) if retry_count < 3 => {
                retry_count += 1;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }

    // Use the server
    match client.call_tool("my_tool", json!({})).await {
        Ok(result) => println!("Success: {}", result.content),
        Err(McpError::InternalError(msg)) => {
            eprintln!("Internal error, retrying...");
            // Retry logic here
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}
```

## Best Practices

### Server Selection

1. **Use official servers** when possible
2. **Verify server authenticity** before connecting
3. **Sandbox servers** to limit access
4. **Monitor server health** continuously

### Tool Usage

1. **Validate tool inputs** before calling
2. **Handle tool errors** gracefully
3. **Cache tool results** when appropriate
4. **Use parallel execution** for independent tools

### Resource Access

1. **Check resource permissions** before accessing
2. **Monitor resource changes** for updates
3. **Cache resource content** to reduce load
4. **Handle resource errors** appropriately

### Performance

1. **Limit concurrent connections** to avoid overwhelming servers
2. **Use connection pooling** for repeated access
3. **Implement rate limiting** to protect servers
4. **Cache results** to reduce redundant calls

## Troubleshooting

### Server Not Starting

**Problem**: MCP server fails to start

**Solutions**:
```bash
# Check if server executable exists
ls -la /usr/local/bin/mcp-filesystem-server

# Test server manually
mcp-filesystem-server --help

# Check server logs
mcp-filesystem-server --verbose
```

### Tool Not Found

**Problem**: Tool call returns "tool not found" error

**Solutions**:
```rust
// List available tools
let tools = client.list_tools().await?;
for tool in tools {
    println!("Available tool: {}", tool.name);
}

// Check tool name spelling
// Verify server connection
// Check server capabilities
```

### Connection Issues

**Problem**: Cannot connect to MCP server

**Solutions**:
```bash
# Check if server is running
pgrep -f mcp-server

# Test server connection
echo '{"jsonrpc":"2.0","method":"ping","id":1}' | mcp-server

# Check firewall rules
sudo ufw status
```

### Performance Issues

**Problem**: Tool calls are slow

**Solutions**:
1. **Enable parallel execution**:
```rust
let tasks: Vec<_> = tools.into_iter()
    .map(|tool| client.call_tool(tool.name, args))
    .collect();

let results = join_all(tasks).await?;
```

2. **Implement caching**:
```rust
use std::collections::HashMap;
use tokio::sync::RwLock;

let cache: RwLock<HashMap<String, String>> = RwLock::new();

// Check cache first
{
    let cache = cache.read().await;
    if let Some(cached) = cache.get(&cache_key) {
        return Ok(cached.clone());
    }
}

// Execute tool call
let result = client.call_tool(tool_name, args).await?;

// Update cache
{
    let mut cache = cache.write().await;
    cache.insert(cache_key, result.content.clone());
}
```

## Conclusion

The RustyCode MCP integration is designed to be:
- **Extensible**: Easy to add new tools and servers
- **Standardized**: Unified interface for all tools
- **Production-Ready**: Robust error handling and monitoring
- **Performant**: Parallel execution and caching support

For more information, see:
- [Architecture Overview](ARCHITECTURE.md)
- [Agent System Guide](AGENTS.md)
- [Provider Guide](PROVIDERS.md)
- [MCP Specification](https://modelcontextprotocol.io/)
