//! Provider Router Example
//!
//! This example demonstrates how to use the multi-provider router
//! to intelligently select between different LLM providers based on
//! routing strategies like cost optimization, capability matching,
//! and failover.

use rustycode_llm::provider_router::{
    default_router, ProviderConfig, ProviderRouter, RoutingRequirements, RoutingStrategy,
};

fn main() {
    println!("=== Multi-Provider Router Example ===\n");

    // Example 1: Default router with common providers
    println!("1. Default Router Configuration");
    println!("--------------------------------");
    let router = default_router();
    println!("Enabled providers:");
    for provider in router.enabled_providers() {
        println!(
            "  - {} (priority: {}, cost: ${:.2}/1K input, ${:.2}/1K output)",
            provider.name,
            provider.priority,
            provider.cost_per_1k_input.unwrap_or(0.0),
            provider.cost_per_1k_output.unwrap_or(0.0)
        );
        println!("    Capabilities: {:?}", provider.capabilities);
    }
    println!();

    // Example 2: Primary strategy (always use highest priority)
    println!("2. Primary Strategy");
    println!("--------------------");
    let mut router = default_router();
    router.set_strategy(RoutingStrategy::Primary);
    let decision = router.select(&RoutingRequirements::new()).unwrap();
    println!("Selected: {}", decision.provider);
    println!("Reason: {}", decision.reason);
    println!();

    // Example 3: Cost-optimized strategy
    println!("3. Cost-Optimized Strategy");
    println!("--------------------------");
    let mut router = default_router();
    router.set_strategy(RoutingStrategy::CostOptimized);
    let decision = router.select(&RoutingRequirements::new()).unwrap();
    println!("Selected: {}", decision.provider);
    println!("Reason: {}", decision.reason);
    println!();

    // Example 4: Capability-based routing
    println!("4. Capability-Based Routing");
    println!("---------------------------");
    let mut router = default_router();
    router.set_strategy(RoutingStrategy::Capability);

    // Request extended thinking capability (only Anthropic has it)
    let req = RoutingRequirements::new()
        .with_capability("extended_thinking")
        .with_capability("tools");
    let decision = router.select(&req).unwrap();
    println!("Required capabilities: {:?}", req.required_capabilities);
    println!("Selected: {}", decision.provider);
    println!("Reason: {}", decision.reason);
    println!();

    // Example 5: Context size filtering
    println!("5. Context Size Filtering");
    println!("-------------------------");
    let mut router = default_router();
    router.set_strategy(RoutingStrategy::Primary);

    // Request large context window (500K tokens)
    let req = RoutingRequirements::new().with_context_size(500_000);
    let decision = router.select(&req).unwrap();
    println!("Required context: 500K tokens");
    println!("Selected: {}", decision.provider);
    println!("Reason: {}", decision.reason);
    println!("Note: Gemini has 1M token context, the largest!");
    println!();

    // Example 6: Failover handling
    println!("6. Failover Handling");
    println!("--------------------");
    let mut router = default_router();
    router.set_strategy(RoutingStrategy::Failover);

    // Primary selection
    let primary = router.select(&RoutingRequirements::new()).unwrap();
    println!("Primary: {}", primary.provider);

    // Simulate failure and get fallback
    let fallback = router.get_fallback(&primary.provider).unwrap();
    println!("Fallback: {}", fallback.provider);
    println!("Reason: {}", fallback.reason);
    println!();

    // Example 7: Custom router configuration
    println!("7. Custom Router Configuration");
    println!("-------------------------------");
    let mut custom_router = ProviderRouter::new(RoutingStrategy::CostOptimized);

    // Add custom providers
    custom_router.add_providers(vec![
        ProviderConfig {
            name: "budget_provider".into(),
            priority: 10,
            enabled: true,
            cost_per_1k_input: Some(0.01),
            cost_per_1k_output: Some(0.01),
            capabilities: vec!["streaming".into()],
            max_context: Some(32_000),
        },
        ProviderConfig {
            name: "premium_provider".into(),
            priority: 1,
            enabled: true,
            cost_per_1k_input: Some(0.50),
            cost_per_1k_output: Some(2.00),
            capabilities: vec![
                "streaming".into(),
                "tools".into(),
                "vision".into(),
                "extended_thinking".into(),
            ],
            max_context: Some(200_000),
        },
    ]);

    let decision = custom_router.select(&RoutingRequirements::new()).unwrap();
    println!("Selected: {}", decision.provider);
    println!("Reason: {}", decision.reason);
    println!();

    // Example 8: Round-robin load balancing
    println!("8. Round-Robin Load Balancing");
    println!("------------------------------");
    let mut router = default_router();
    router.set_strategy(RoutingStrategy::RoundRobin);

    println!("Distributing load across providers:");
    for i in 0..5 {
        let decision = router.select(&RoutingRequirements::new()).unwrap();
        println!("  Request {}: {}", i + 1, decision.provider);
    }
    println!();

    // Example 9: Dynamic provider management
    println!("9. Dynamic Provider Management");
    println!("-------------------------------");
    let mut router = default_router();

    println!(
        "Initial enabled providers: {}",
        router.enabled_providers().len()
    );

    // Disable a provider
    router.set_provider_enabled("anthropic", false);
    println!(
        "After disabling Anthropic: {}",
        router.enabled_providers().len()
    );

    // Re-enable the provider
    router.set_provider_enabled("anthropic", true);
    println!(
        "After re-enabling Anthropic: {}",
        router.enabled_providers().len()
    );
    println!();

    println!("=== Example Complete ===");
}
