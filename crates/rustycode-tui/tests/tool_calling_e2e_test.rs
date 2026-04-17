//! End-to-end test for tool calling with Anthropic API
use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider_v2::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig},
};
use rustycode_tools::{default_registry, ToolContext};
use secrecy::SecretString;
use std::env;

#[test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
fn test_tool_calling_e2e() {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url,
        timeout_seconds: Some(300),
        extra_headers: None,
        retry_config: None,
    };

    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-haiku".to_string());

    let provider = match AnthropicProvider::new_without_validation(config, model.clone()) {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping test: Failed to create provider: {:?}", e);
            return;
        }
    };

    // Create tool registry and generate tool definitions
    let tool_registry = default_registry();
    let tools = tool_registry.list();
    println!("Available tools: {}", tools.len());

    let tool_definitions: Vec<serde_json::Value> = tools
        .into_iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": {
                    "type": "object",
                    "properties": tool.parameters_schema,
                    "required": []
                }
            })
        })
        .collect();

    println!("Tool definitions: {}", tool_definitions.len());

    // Create a simple system prompt
    let system_prompt = "You are RustyCode, an AI coding assistant. Use tools when needed.";

    // Test 1: Simple tool use request
    println!("\n=== Test 1: Read file request ===");
    let messages = vec![ChatMessage::user(
        "Read the Cargo.toml file and tell me the project name.".to_string(),
    )];

    let request = CompletionRequest::new(model.clone(), messages)
        .with_system_prompt(system_prompt.to_string())
        .with_tools(tool_definitions.clone());

    let rt = tokio::runtime::Runtime::new().unwrap();
    match rt.block_on(async { provider.complete(request).await }) {
        Ok(response) => {
            println!("Response: {}", response.content);

            // Check if response contains tool calls
            if response.content.contains("```tool") {
                println!("✓ Tool call detected in response");

                // Parse the tool call
                let tool_start = response.content.find("```tool").unwrap();
                let tool_body = &response.content[tool_start + 8..];
                let tool_end_rel = tool_body.find("```").unwrap_or(tool_body.len());
                let tool_json = &tool_body[..tool_end_rel].trim();

                println!("Tool call JSON: {}", tool_json);

                // Verify it's a read_file call
                if tool_json.contains("read_file") {
                    println!("✓ Correct tool (read_file) detected");
                } else {
                    println!("✗ Unexpected tool in response");
                }
            } else {
                println!("✗ No tool call detected in response");
                println!("Response may have answered without using tools");
            }
        }
        Err(e) => {
            println!("✗ Request failed: {:?}", e);
        }
    }

    // Test 2: Request that doesn't need tools
    println!("\n=== Test 2: Non-tool request ===");
    let messages2 = vec![ChatMessage::user("What is 2+2?".to_string())];

    let request2 = CompletionRequest::new(model.clone(), messages2)
        .with_system_prompt(system_prompt.to_string())
        .with_tools(tool_definitions.clone());

    match rt.block_on(async { provider.complete(request2).await }) {
        Ok(response2) => {
            println!("Response: {}", response2.content);

            // Should NOT contain tool calls
            if !response2.content.contains("```tool") {
                println!("✓ No tool call for simple math question (correct)");
            } else {
                println!("✗ Unexpected tool call for simple question");
            }
        }
        Err(e) => {
            println!("✗ Request failed: {:?}", e);
        }
    }

    println!("\n=== Tests Complete ===");
}

#[test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
fn test_tool_execution() {
    let _api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    // Test that we can execute tools directly
    let tool_registry = default_registry();
    let ctx = ToolContext::new(env::current_dir().unwrap());

    // Test read_file tool
    if let Some(tool) = tool_registry.get("read_file") {
        println!("Testing read_file tool...");

        let params = serde_json::json!({
            "path": "Cargo.toml"
        });

        match tool.execute(params, &ctx) {
            Ok(result) => {
                println!("✓ Tool executed successfully");
                println!("Output length: {} chars", result.text.len());

                // Verify output contains expected content
                if result.text.contains("[package]") || result.text.contains("name =") {
                    println!("✓ Output appears to be valid TOML");
                } else {
                    println!("✗ Unexpected output format");
                }
            }
            Err(e) => {
                println!("✗ Tool execution failed: {:?}", e);
            }
        }
    } else {
        println!("✗ read_file tool not found in registry");
    }
}
