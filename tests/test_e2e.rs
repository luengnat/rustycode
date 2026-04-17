//! Simple end-to-end test for LLM provider functionality
//! Run with: cargo run --bin test_e2e

use rustycode_llm::anthropic::AnthropicProvider;
use rustycode_llm::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig};
use secrecy::SecretString;
use std::env;

fn main() {
    println!("=== LLM Provider End-to-End Test ===\n");

    // Load configuration from environment
    let api_key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");
    let base_url =
        env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| "https://api.anthropic.com".to_string());
    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-haiku".to_string());

    println!("Configuration:");
    println!("  Base URL: {}", base_url);
    println!("  Model: {}", model);
    println!();

    // Initialize provider
    println!("Initializing provider...");
    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url: Some(base_url),
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider = match AnthropicProvider::new_without_validation(config, model.clone()) {
        Ok(p) => {
            println!("✓ Provider initialized successfully");
            println!("  Provider name: {}", p.name());
            p
        }
        Err(e) => {
            eprintln!("✗ Failed to initialize provider: {:?}", e);
            std::process::exit(1);
        }
    };
    println!();

    // Create runtime for async execution
    let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");

    // Test 1: Simple completion
    println!("Test 1: Simple completion");
    println!("  Request: What is 2+2?");
    let request1 = CompletionRequest::new(
        model.clone(),
        vec![ChatMessage::user("What is 2+2?".to_string())],
    );

    match rt.block_on(provider.complete(request1)) {
        Ok(response) => {
            println!("  ✓ Response: {}", response.content.trim());
            if let Some(usage) = response.usage {
                println!(
                    "  Usage: {} input tokens, {} output tokens",
                    usage.input_tokens, usage.output_tokens
                );
            }
        }
        Err(e) => {
            eprintln!("  ✗ Request failed: {:?}", e);
        }
    }
    println!();

    // Test 2: Code generation
    println!("Test 2: Code generation");
    println!("  Request: Write a hello world in Rust");
    let request2 = CompletionRequest::new(
        model.clone(),
        vec![ChatMessage::user(
            "Write a hello world in Rust (keep it under 5 lines)".to_string(),
        )],
    );

    match rt.block_on(provider.complete(request2)) {
        Ok(response) => {
            println!("  ✓ Response received");
            println!("  Content:\n{}", response.content.trim());
        }
        Err(e) => {
            eprintln!("  ✗ Request failed: {:?}", e);
        }
    }
    println!();

    // Test 3: Math problem
    println!("Test 3: Math problem");
    println!("  Request: What is 15 * 23?");
    let request3 = CompletionRequest::new(
        model.clone(),
        vec![ChatMessage::user(
            "What is 15 * 23? Answer with just the number.".to_string(),
        )],
    );

    match rt.block_on(provider.complete(request3)) {
        Ok(response) => {
            println!("  ✓ Response: {}", response.content.trim());
        }
        Err(e) => {
            eprintln!("  ✗ Request failed: {:?}", e);
        }
    }
    println!();

    println!("=== All tests completed ===");
}
