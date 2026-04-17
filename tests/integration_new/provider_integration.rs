// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Provider integration tests
//!
//! Tests cover:
//! - Provider bootstrap and auto-discovery
//! - Model registry and listing
//! - Cost tracking and accumulation
//! - Multi-provider usage
//! - Provider capabilities

use std::env;

use rustycode_providers::{
    bootstrap_from_env, CostTracker, ModelInfo, ModelRegistry, ProviderMetadata,
};

mod common;
use common::TestEnv;

#[tokio::test]
async fn test_provider_bootstrap() {
    let mut test_env = TestEnv::new();

    // Set API keys for various providers
    test_env.set("ANTHROPIC_API_KEY", "sk-ant-test-key");
    test_env.set("OPENAI_API_KEY", "sk-openai-test-key");
    test_env.set("OPENROUTER_API_KEY", "sk-or-test-key");
    test_env.set("GEMINI_API_KEY", "test-gemini-key");
    test_env.set("OLLAMA_BASE_URL", "http://localhost:11434");

    // Bootstrap providers from environment
    let registry = bootstrap_from_env().await;

    // Verify providers were discovered
    let provider_count = registry.count();
    assert!(provider_count >= 1, "Should discover at least Ollama provider");

    // List providers
    let providers = registry.list_providers();
    assert!(!providers.is_empty(), "Should have at least one provider");

    // Verify expected providers are present
    if env::var("ANTHROPIC_API_KEY").is_ok() {
        assert!(providers.contains(&"anthropic".to_string()));
    }
    if env::var("OPENAI_API_KEY").is_ok() {
        assert!(providers.contains(&"openai".to_string()));
    }
    if env::var("OPENROUTER_API_KEY").is_ok() {
        assert!(providers.contains(&"openrouter".to_string()));
    }
    // Ollama should always be present
    assert!(providers.contains(&"ollama".to_string()));
}

#[tokio::test]
async fn test_model_registry() {
    let registry = ModelRegistry::new();

    // Register a test provider
    let test_provider = ProviderMetadata {
        id: "test-provider".to_string(),
        name: "Test Provider".to_string(),
        base_url: "https://api.test.com".to_string(),
        api_key_env: "TEST_API_KEY".to_string(),
        capabilities: rustycode_providers::ProviderCapabilities {
            supports_streaming: true,
            supports_function_calling: true,
            supports_vision: false,
            max_tokens: 4096,
            max_context_window: 128000,
        },
        pricing: rustycode_providers::PricingInfo {
            input_cost_per_1k: 0.001,
            output_cost_per_1k: 0.002,
            currency: rustycode_providers::Currency::Usd,
        },
    };

    registry.register_provider(test_provider).await;

    // Verify provider is registered
    assert!(registry.has_provider("test-provider"));

    // Get provider metadata
    let provider = registry.get_provider("test-provider");
    assert!(provider.is_some());
    let provider = provider.unwrap();
    assert_eq!(provider.id, "test-provider");
    assert_eq!(provider.name, "Test Provider");

    // List providers
    let providers = registry.list_providers();
    assert!(providers.contains(&"test-provider".to_string()));
}

#[tokio::test]
async fn test_model_listing() {
    let registry = ModelRegistry::new();

    // Register Anthropic provider
    let anthropic = rustycode_providers::providers::anthropic();
    registry.register_provider(anthropic).await;

    // List models for provider
    let models = registry.list_models("anthropic").await;

    // Should have Claude models
    assert!(!models.is_empty(), "Should have at least one model");

    // Verify model metadata
    for model in models {
        assert!(!model.id.is_empty());
        assert!(!model.name.is_empty());
        assert!(model.input_cost_per_1k >= 0.0);
        assert!(model.output_cost_per_1k >= 0.0);
    }
}

#[tokio::test]
async fn test_cost_tracking() {
    let mut cost_tracker = CostTracker::new();

    // Track some API calls
    cost_tracker
        .track_usage("anthropic", "claude-3-5-sonnet", 1000, 500)
        .unwrap();

    cost_tracker
        .track_usage("anthropic", "claude-3-5-sonnet", 2000, 1000)
        .unwrap();

    // Get summary
    let summary = cost_tracker.get_summary();

    // Verify totals
    assert_eq!(summary.total_input_tokens, 3000);
    assert_eq!(summary.total_output_tokens, 1500);
    assert!(summary.total_cost > 0.0);

    // Get per-model costs
    let model_costs = cost_tracker.get_costs_by_model();
    assert!(!model_costs.is_empty());

    let anthropic_costs = model_costs.get("claude-3-5-sonnet");
    assert!(anthropic_costs.is_some());
    let costs = anthropic_costs.unwrap();
    assert_eq!(costs.input_tokens, 3000);
    assert_eq!(costs.output_tokens, 1500);
}

#[tokio::test]
async fn test_multi_provider_usage() {
    let registry = ModelRegistry::new();

    // Register multiple providers
    registry
        .register_provider(rustycode_providers::providers::anthropic())
        .await;
    registry
        .register_provider(rustycode_providers::providers::openai())
        .await;
    registry
        .register_provider(rustycode_providers::providers::ollama("http://localhost:11434"))
        .await;

    // Track costs across providers
    let mut cost_tracker = CostTracker::new();

    // Use Anthropic
    cost_tracker
        .track_usage("anthropic", "claude-3-5-sonnet", 1000, 500)
        .unwrap();

    // Use OpenAI
    cost_tracker
        .track_usage("openai", "gpt-4o", 2000, 1000)
        .unwrap();

    // Use Ollama (free)
    cost_tracker
        .track_usage("ollama", "llama2", 500, 250)
        .unwrap();

    // Get summary
    let summary = cost_tracker.get_summary();

    // Should have tracked all providers
    let provider_costs = cost_tracker.get_costs_by_provider();
    assert_eq!(provider_costs.len(), 3);

    // Anthropic and OpenAI should have costs
    assert!(provider_costs.contains_key("anthropic"));
    assert!(provider_costs.contains_key("openai"));

    // Ollama should be free
    let ollama_cost = provider_costs.get("ollama").unwrap();
    assert_eq!(ollama_cost.cost, 0.0);
}

#[tokio::test]
async fn test_provider_capabilities() {
    let registry = ModelRegistry::new();

    // Register providers with different capabilities
    registry
        .register_provider(rustycode_providers::providers::anthropic())
        .await;
    registry
        .register_provider(rustycode_providers::providers::openai())
        .await;

    // Get Anthropic provider
    let anthropic = registry.get_provider("anthropic").unwrap();
    assert!(anthropic.capabilities.supports_streaming);
    assert!(anthropic.capabilities.supports_function_calling);
    assert!(anthropic.capabilities.supports_vision);
    assert_eq!(anthropic.capabilities.max_context_window, 200_000);

    // Get OpenAI provider
    let openai = registry.get_provider("openai").unwrap();
    assert!(openai.capabilities.supports_streaming);
    assert!(openai.capabilities.supports_function_calling);
    assert!(openai.capabilities.supports_vision);
    assert_eq!(openai.capabilities.max_context_window, 128_000);
}

#[tokio::test]
async fn test_model_info_structure() {
    let model = ModelInfo {
        id: "test-model".to_string(),
        name: "Test Model".to_string(),
        provider: "test-provider".to_string(),
        input_cost_per_1k: 0.003,
        output_cost_per_1k: 0.015,
        max_tokens: 4096,
        supports_streaming: true,
        supports_function_calling: true,
        supports_vision: false,
    };

    // Verify model properties
    assert_eq!(model.id, "test-model");
    assert_eq!(model.name, "Test Model");
    assert_eq!(model.provider, "test-provider");
    assert_eq!(model.input_cost_per_1k, 0.003);
    assert_eq!(model.output_cost_per_1k, 0.015);
    assert!(model.supports_streaming);
    assert!(model.supports_function_calling);
    assert!(!model.supports_vision);
}

#[tokio::test]
async fn test_cost_accumulation_accuracy() {
    let mut cost_tracker = CostTracker::new();

    // Track usage with known costs
    // Anthropic: $0.003/1k input, $0.015/1k output
    let input_tokens = 1000;
    let output_tokens = 500;

    cost_tracker
        .track_usage("anthropic", "claude-3-5-sonnet", input_tokens, output_tokens)
        .unwrap();

    let summary = cost_tracker.get_summary();

    // Calculate expected cost
    let expected_input_cost = (input_tokens as f64 / 1000.0) * 0.003;
    let expected_output_cost = (output_tokens as f64 / 1000.0) * 0.015;
    let expected_total = expected_input_cost + expected_output_cost;

    // Verify cost calculation (within small epsilon for floating point)
    assert!(
        (summary.total_cost - expected_total).abs() < 0.0001,
        "Cost mismatch: expected {}, got {}",
        expected_total,
        summary.total_cost
    );
}

#[tokio::test]
async fn test_provider_pricing_currency() {
    let registry = ModelRegistry::new();

    // Register providers
    registry
        .register_provider(rustycode_providers::providers::anthropic())
        .await;
    registry
        .register_provider(rustycode_providers::providers::openai())
        .await;

    // Check pricing is in USD
    let anthropic = registry.get_provider("anthropic").unwrap();
    assert_eq!(
        anthropic.pricing.currency,
        rustycode_providers::Currency::Usd
    );

    let openai = registry.get_provider("openai").unwrap();
    assert_eq!(openai.pricing.currency, rustycode_providers::Currency::Usd);
}

#[tokio::test]
async fn test_cost_reset() {
    let mut cost_tracker = CostTracker::new();

    // Track some usage
    cost_tracker
        .track_usage("anthropic", "claude-3-5-sonnet", 1000, 500)
        .unwrap();

    let summary1 = cost_tracker.get_summary();
    assert!(summary1.total_cost > 0.0);

    // Reset
    cost_tracker.reset();

    let summary2 = cost_tracker.get_summary();
    assert_eq!(summary2.total_input_tokens, 0);
    assert_eq!(summary2.total_output_tokens, 0);
    assert_eq!(summary2.total_cost, 0.0);
}

#[tokio::test]
async fn test_model_registry_persistence() {
    let registry = ModelRegistry::new();

    // Register provider
    registry
        .register_provider(rustycode_providers::providers::anthropic())
        .await;

    // Verify it's registered
    assert!(registry.has_provider("anthropic"));

    // Count should be 1
    assert_eq!(registry.count(), 1);

    // List providers
    let providers = registry.list_providers();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0], "anthropic");
}

#[tokio::test]
async fn test_unknown_provider_handling() {
    let registry = ModelRegistry::new();

    // Try to get unknown provider
    let unknown = registry.get_provider("unknown-provider");
    assert!(unknown.is_none());

    // Try to list models for unknown provider
    let models = registry.list_models("unknown-provider").await;
    assert!(models.is_empty());
}
