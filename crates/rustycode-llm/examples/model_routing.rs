//! Example: Dynamic Model Routing
//!
//! Demonstrates how to use the model router to automatically select
//! the appropriate Claude model based on task complexity.

use rustycode_llm::model_router::{ModelChoice, ModelRouter, Request, RouterConfig, SimpleRouter};

fn main() {
    println!("=== Model Router Examples ===\n");

    // Example 1: Simple routing with defaults
    println!("Example 1: Default Balanced Routing");
    println!("-----------------------------------");
    let router = SimpleRouter::new(Default::default());

    let simple_task = Request::new("What is the capital of France?");
    println!("Task: \"{}\"", simple_task.text);
    println!(
        "Selected: {} (Strategy: {})\n",
        router.route(&simple_task).short_name(),
        router.strategy_name()
    );

    let code_task = Request::new("Optimize this function for performance").with_code(true);
    println!("Task: \"{}\" [code]", code_task.text);
    println!("Selected: {}\n", router.route(&code_task).short_name());

    let complex_task = Request::new("a".repeat(8000)); // 2000 tokens
    println!("Task: Very long request (2000 tokens)");
    println!("Selected: {}\n", router.route(&complex_task).short_name());

    // Example 2: Cost-optimized routing
    println!("\nExample 2: Cost-Optimized Routing");
    println!("----------------------------------");
    let cost_config = RouterConfig {
        prefer_cost: true,
        ..Default::default()
    };
    let cost_router = SimpleRouter::new(cost_config);

    let complex_task = Request::new("Design a distributed system architecture").with_code(true);
    println!("Task: \"{}\" [code]", complex_task.text);
    println!(
        "Selected with cost optimization: {} (saves ~{}x cost vs Opus)\n",
        cost_router.route(&complex_task).short_name(),
        ModelChoice::Opus.relative_cost() / ModelChoice::Sonnet.relative_cost()
    );

    // Example 3: Speed-optimized routing
    println!("\nExample 3: Speed-Optimized Routing");
    println!("-----------------------------------");
    let speed_config = RouterConfig {
        prefer_speed: true,
        ..Default::default()
    };
    let speed_router = SimpleRouter::new(speed_config);

    let simple_task = Request::new("Quick question");
    println!("Task: \"{}\"", simple_task.text);
    println!(
        "Selected: {} (fastest response time)\n",
        speed_router.route(&simple_task).short_name()
    );

    // Example 4: Capability-optimized routing
    println!("\nExample 4: Capability-Optimized Routing");
    println!("--------------------------------------");
    let capability_config = RouterConfig {
        prefer_capability: true,
        ..Default::default()
    };
    let capability_router = SimpleRouter::new(capability_config);

    let task = Request::new("What is 2+2?");
    println!("Task: \"{}\"", task.text);
    println!(
        "Selected: {} (highest capability)\n",
        capability_router.route(&task).short_name()
    );

    // Example 5: User preference override
    println!("\nExample 5: User Preference Override");
    println!("------------------------------------");
    let task = Request::new("a".repeat(8000)) // Would normally be Opus
        .with_model_preference(ModelChoice::Haiku); // But user forces Haiku
    println!("Task: Very long request (normally Opus)");
    println!("User preference: Haiku");
    println!(
        "Selected: {} (user choice wins)\n",
        router.route(&task).short_name()
    );

    // Example 6: Multi-turn conversation
    println!("\nExample 6: Multi-Turn Conversation");
    println!("-----------------------------------");
    let turn1 = Request::new("Explain recursion").with_conversation_turn(1);
    let turn5 = Request::new("Show me a practical example").with_conversation_turn(5);

    println!("Turn 1: \"{}\"", turn1.text);
    println!("Selected: {}", router.route(&turn1).short_name());
    println!("\nTurn 5: \"{}\" [in multi-turn conversation]", turn5.text);
    println!(
        "Selected: {} (conversation complexity increases)\n",
        router.route(&turn5).short_name()
    );

    // Example 7: Model information
    println!("\nExample 7: Model Information");
    println!("-----------------------------");
    for choice in &[ModelChoice::Haiku, ModelChoice::Sonnet, ModelChoice::Opus] {
        println!(
            "{}: {} | Cost multiplier: {}x | Model ID: {}",
            choice.short_name(),
            choice,
            choice.relative_cost(),
            choice.model_id()
        );
    }

    // Example 8: Custom thresholds
    println!("\nExample 8: Custom Thresholds");
    println!("----------------------------");
    let custom_config = RouterConfig {
        simple_task_threshold: 200,  // < 200 tokens = simple
        medium_task_threshold: 1000, // < 1000 tokens = medium
        ..Default::default()
    };
    let custom_router = SimpleRouter::new(custom_config);

    let medium = Request::new("a".repeat(600)); // 150 tokens
    println!("Request: 600 chars (150 tokens)");
    println!(
        "With custom thresholds: {:?}",
        custom_router.classify_complexity(&medium)
    );
    println!(
        "Selected model: {}\n",
        custom_router.route(&medium).short_name()
    );

    // Example 9: All complexity detection methods
    println!("\nExample 9: Complexity Indicators");
    println!("--------------------------------");
    let keywords = [
        "architecture",
        "design",
        "refactor",
        "debug",
        "analyze",
        "system",
        "framework",
        "algorithm",
        "optimize",
        "performance",
    ];

    for keyword in &keywords {
        let task = Request::new(format!("Please {} this code", keyword)).with_code(true);
        println!(
            "\"{}\" keyword -> {} (triggers Complex)",
            keyword,
            router.route(&task).short_name()
        );
    }
}

// Demonstration of trait usage
#[allow(dead_code)]
trait DemoRouter {
    fn demo(&self);
}

impl DemoRouter for SimpleRouter {
    fn demo(&self) {
        println!("\nRouter Strategy: {}", self.strategy_name());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_basic_routing() {
        let router = SimpleRouter::new(Default::default());
        let request = Request::new("Hello");
        let choice = router.route(&request);
        assert_eq!(choice, ModelChoice::Haiku);
    }

    #[test]
    fn example_code_detection() {
        let router = SimpleRouter::new(Default::default());
        let request = Request::new("Fix this").with_code(true);
        assert!(router.route(&request) >= ModelChoice::Sonnet);
    }

    #[test]
    fn example_user_override() {
        let router = SimpleRouter::new(Default::default());
        let request = Request::new("a".repeat(8000)).with_model_preference(ModelChoice::Haiku);
        assert_eq!(router.route(&request), ModelChoice::Haiku);
    }
}
