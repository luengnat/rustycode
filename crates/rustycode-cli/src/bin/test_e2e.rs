//! Simple end-to-end test for LLM provider functionality
//! Run with: cargo run -p rustycode-cli --bin test_e2e

use rustycode_llm::anthropic::AnthropicProvider;
use rustycode_llm::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig};
use secrecy::SecretString;
use std::env;
use tracing::{info, warn};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into()),
        )
        .init();

    info!("=== LLM Provider End-to-End Test ===");

    let api_key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");
    let base_url =
        env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| "https://api.anthropic.com".to_string());
    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-haiku".to_string());

    info!("Configuration:");
    info!("  Base URL: {}", base_url);
    info!("  Model: {}", model);

    info!("Initializing provider...");
    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url: Some(base_url),
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider: AnthropicProvider =
        match AnthropicProvider::new_without_validation(config, model.clone()) {
            Ok(p) => {
                info!("Provider initialized successfully");
                info!("  Provider name: {}", p.name());
                p
            }
            Err(e) => {
                warn!("Failed to initialize provider: {:?}", e);
                std::process::exit(1);
            }
        };

    let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");

    info!("Test 1: What is 2+2?");
    let request1 = CompletionRequest::new(
        model.clone(),
        vec![ChatMessage::user("What is 2+2?".to_string())],
    );

    match rt.block_on(provider.complete(request1)) {
        Ok(response) => {
            info!("Response: {}", response.content.trim());
            if let Some(usage) = response.usage {
                info!(
                    "  Usage: {} input, {} output tokens",
                    usage.input_tokens, usage.output_tokens
                );
            }
        }
        Err(e) => {
            warn!("Failed: {:?}", e);
        }
    }

    info!("Test 2: Math - 15 * 23?");
    let request2 = CompletionRequest::new(
        model.clone(),
        vec![ChatMessage::user(
            "What is 15 * 23? Answer with just the number.".to_string(),
        )],
    );

    match rt.block_on(provider.complete(request2)) {
        Ok(response) => {
            info!("Response: {}", response.content.trim());
        }
        Err(e) => {
            warn!("Failed: {:?}", e);
        }
    }

    info!("=== All tests completed ===");
}
