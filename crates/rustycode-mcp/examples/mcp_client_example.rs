//! Example MCP client usage

use rustycode_mcp::{McpClient, McpClientConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info,mcp_client=debug")
        .init();

    println!("MCP Client Example");

    // Create MCP client
    let config = McpClientConfig::default();
    let _client = McpClient::new(config);

    // Example: Connect to a server (uncomment to use)
    // client.connect_stdio(
    //     "my-server",
    //     "/path/to/server",
    //     &["--option1", "option2"]
    // ).await?;

    println!("Client created successfully");
    println!("To connect to a server, use client.connect_stdio()");
    println!("Then call client.list_tools(), client.call_tool(), etc.");

    Ok(())
}
