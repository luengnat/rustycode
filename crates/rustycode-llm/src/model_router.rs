//! # Model Routing - Dynamic Model Selection Based on Task Complexity
//!
//! This module provides a simple model router that selects the appropriate Claude model
//! based on task complexity, cost optimization, and user preferences.
//!
//! ## Overview
//!
//! The model router maps task characteristics to model tiers:
//! - **Haiku** (Budget): Fast, cost-effective for straightforward work
//! - **Sonnet** (Balanced): Default choice for most tasks
//! - **Opus** (Premium): Highest capability for complex reasoning
//!
//! ## Routing Logic
//!
//! The router classifies task complexity based on:
//! - Estimated token count (message/context length)
//! - Presence of code/analysis requirements
//! - Conversation complexity (multi-turn, nested context)
//! - User model preferences (overrides)
//!
//! ## Example
//!
//! ```rust
//! use rustycode_llm::model_router::{ModelRouter, SimpleRouter, Request};
//!
//! let router = SimpleRouter::new(Default::default());
//! let request = Request {
//!     text: "Fix this bug in my code".to_string(),
//!     has_code: true,
//!     conversation_turn: 1,
//!     user_model_preference: None,
//! };
//!
//! let choice = router.route(&request);
//! println!("Selected model: {:?}", choice); // ModelChoice::Sonnet
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// Model choice for routing decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ModelChoice {
    /// Claude 3 Haiku - Fast, cost-effective for simple tasks
    Haiku,
    /// Claude 3.5 Sonnet - Balanced performance and cost (default)
    Sonnet,
    /// Claude 3 Opus - Highest capability for complex reasoning
    Opus,
}

impl fmt::Display for ModelChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelChoice::Haiku => write!(f, "haiku"),
            ModelChoice::Sonnet => write!(f, "sonnet"),
            ModelChoice::Opus => write!(f, "opus"),
            #[allow(unreachable_patterns)]
            _ => write!(f, "unknown"),
        }
    }
}

impl ModelChoice {
    /// Get the full model identifier for API calls (defaults).
    /// Prefer `model_id_from_config()` when a config map is available.
    pub fn model_id(&self) -> &'static str {
        match self {
            ModelChoice::Haiku => "claude-haiku-4-5-20251001",
            ModelChoice::Sonnet => "claude-sonnet-4-6",
            ModelChoice::Opus => "claude-opus-4-6",
            #[allow(unreachable_patterns)]
            _ => "claude-sonnet-4-6",
        }
    }

    /// Get model ID from a config map (intent_name → model_id).
    /// Falls back to `model_id()` defaults if not in the map.
    pub fn model_id_from_config(
        &self,
        config_models: &std::collections::HashMap<String, String>,
    ) -> String {
        let key = match self {
            ModelChoice::Haiku => "explanation",
            ModelChoice::Sonnet => "implementation",
            ModelChoice::Opus => "planning",
            #[allow(unreachable_patterns)]
            _ => "implementation",
        };
        config_models
            .get(key)
            .cloned()
            .unwrap_or_else(|| self.model_id().to_string())
    }

    /// Get a short display name
    pub fn short_name(&self) -> &'static str {
        match self {
            ModelChoice::Haiku => "Haiku",
            ModelChoice::Sonnet => "Sonnet",
            ModelChoice::Opus => "Opus",
            #[allow(unreachable_patterns)]
            _ => "Unknown",
        }
    }

    /// Estimate relative cost compared to Haiku (Haiku = 1.0)
    pub fn relative_cost(&self) -> f32 {
        match self {
            ModelChoice::Haiku => 1.0,
            ModelChoice::Sonnet => 5.0, // ~5x more expensive
            ModelChoice::Opus => 20.0,  // ~20x more expensive
            #[allow(unreachable_patterns)]
            _ => 5.0,
        }
    }
}

/// Request information for routing decisions
#[derive(Debug, Clone)]
pub struct Request {
    /// User's message or query text
    pub text: String,
    /// Whether the request contains code or code-related analysis
    pub has_code: bool,
    /// Which turn in a conversation (1 = first, 2+ = multi-turn)
    pub conversation_turn: usize,
    /// User's explicit model preference (overrides routing)
    pub user_model_preference: Option<ModelChoice>,
}

impl Request {
    /// Create a new request
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            has_code: false,
            conversation_turn: 1,
            user_model_preference: None,
        }
    }

    /// Add code detection
    pub fn with_code(mut self, has_code: bool) -> Self {
        self.has_code = has_code;
        self
    }

    /// Add conversation turn info
    pub fn with_conversation_turn(mut self, turn: usize) -> Self {
        self.conversation_turn = turn.max(1); // Minimum 1
        self
    }

    /// Add user model preference
    pub fn with_model_preference(mut self, preference: ModelChoice) -> Self {
        self.user_model_preference = Some(preference);
        self
    }

    /// Estimate token count from text (conservative heuristic)
    /// Uses 4 characters ≈ 1 token approximation
    pub fn estimate_tokens(&self) -> usize {
        (self.text.len() / 4).max(1)
    }

    /// Detect complexity indicators in text
    fn has_complexity_indicators(&self) -> bool {
        let text_lower = self.text.to_lowercase();

        // Keywords indicating complex tasks
        let complex_keywords = [
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

        complex_keywords.iter().any(|kw| text_lower.contains(kw))
    }
}

/// Trait for model routing implementations
pub trait ModelRouter: Send + Sync {
    /// Route a request to an appropriate model choice
    fn route(&self, request: &Request) -> ModelChoice;

    /// Get routing strategy name (for debugging/logging)
    fn strategy_name(&self) -> &'static str {
        "unknown"
    }
}

/// Configuration for routing strategies
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Prefer cost optimization over capability
    pub prefer_cost: bool,
    /// Prefer speed over cost
    pub prefer_speed: bool,
    /// Prefer capability over cost
    pub prefer_capability: bool,
    /// Force a specific model (disables routing)
    pub force_model: Option<ModelChoice>,
    /// Token threshold for simple tasks (below this = simple)
    pub simple_task_threshold: usize,
    /// Token threshold for medium tasks (below this = medium, else = complex)
    pub medium_task_threshold: usize,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            prefer_cost: false,
            prefer_speed: false,
            prefer_capability: false,
            force_model: None,
            simple_task_threshold: 500,  // < 500 tokens = simple
            medium_task_threshold: 2000, // < 2000 tokens = medium, else complex
        }
    }
}

/// Simple router implementation based on task complexity
pub struct SimpleRouter {
    config: RouterConfig,
}

impl SimpleRouter {
    /// Create a new simple router with default config
    pub fn new(config: RouterConfig) -> Self {
        Self { config }
    }

    /// Classify task complexity
    pub fn classify_complexity(&self, request: &Request) -> TaskComplexity {
        let token_count = request.estimate_tokens();

        // Check if it's complex first (has strong indicators)
        if token_count >= self.config.medium_task_threshold
            || (request.conversation_turn > 3)
            || request.has_complexity_indicators()
        {
            return TaskComplexity::Complex;
        }

        // Check if it's a simple task (light load, no code)
        if token_count < self.config.simple_task_threshold && !request.has_code {
            return TaskComplexity::Simple;
        }

        // Default to medium for everything else
        TaskComplexity::Medium
    }

    /// Select model based on complexity and strategy
    fn select_by_complexity(&self, complexity: TaskComplexity) -> ModelChoice {
        // Handle forced model first
        if let Some(forced) = self.config.force_model {
            return forced;
        }

        // Apply strategy preferences
        if self.config.prefer_cost {
            return self.select_cost_optimized(complexity);
        }

        if self.config.prefer_speed {
            return self.select_speed_optimized(complexity);
        }

        if self.config.prefer_capability {
            return self.select_capability_optimized(complexity);
        }

        // Default balanced strategy
        self.select_balanced(complexity)
    }

    /// Cost-optimized selection (prefer cheaper models)
    fn select_cost_optimized(&self, complexity: TaskComplexity) -> ModelChoice {
        match complexity {
            TaskComplexity::Simple => ModelChoice::Haiku,
            TaskComplexity::Medium => ModelChoice::Haiku, // Even medium can use Haiku
            TaskComplexity::Complex => ModelChoice::Sonnet, // Fall back to Sonnet only if necessary
            #[allow(unreachable_patterns)]
            _ => ModelChoice::Sonnet,
        }
    }

    /// Speed-optimized selection (prefer fastest models)
    fn select_speed_optimized(&self, complexity: TaskComplexity) -> ModelChoice {
        match complexity {
            TaskComplexity::Simple => ModelChoice::Haiku,
            TaskComplexity::Medium => ModelChoice::Sonnet,
            TaskComplexity::Complex => ModelChoice::Opus,
            #[allow(unreachable_patterns)]
            _ => ModelChoice::Sonnet,
        }
    }

    /// Capability-optimized selection (prefer most capable)
    fn select_capability_optimized(&self, complexity: TaskComplexity) -> ModelChoice {
        match complexity {
            TaskComplexity::Simple => ModelChoice::Sonnet, // Even simple gets premium treatment
            TaskComplexity::Medium => ModelChoice::Opus,
            TaskComplexity::Complex => ModelChoice::Opus,
            #[allow(unreachable_patterns)]
            _ => ModelChoice::Opus,
        }
    }

    /// Default balanced selection
    fn select_balanced(&self, complexity: TaskComplexity) -> ModelChoice {
        match complexity {
            TaskComplexity::Simple => ModelChoice::Haiku,
            TaskComplexity::Medium => ModelChoice::Sonnet,
            TaskComplexity::Complex => ModelChoice::Opus,
            #[allow(unreachable_patterns)]
            _ => ModelChoice::Sonnet,
        }
    }
}

impl ModelRouter for SimpleRouter {
    fn route(&self, request: &Request) -> ModelChoice {
        // User preference overrides everything
        if let Some(preference) = request.user_model_preference {
            return preference;
        }

        let complexity = self.classify_complexity(request);
        self.select_by_complexity(complexity)
    }

    fn strategy_name(&self) -> &'static str {
        if self.config.prefer_cost {
            "cost-optimized"
        } else if self.config.prefer_speed {
            "speed-optimized"
        } else if self.config.prefer_capability {
            "capability-optimized"
        } else {
            "balanced"
        }
    }
}

/// Task complexity classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum TaskComplexity {
    /// Simple tasks: short messages, basic queries
    Simple,
    /// Medium tasks: moderate code, analysis, conversation
    Medium,
    /// Complex tasks: large context, deep analysis, multi-step reasoning
    Complex,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_choice_display() {
        assert_eq!(ModelChoice::Haiku.to_string(), "haiku");
        assert_eq!(ModelChoice::Sonnet.to_string(), "sonnet");
        assert_eq!(ModelChoice::Opus.to_string(), "opus");
    }

    #[test]
    fn test_model_choice_model_id() {
        assert_eq!(ModelChoice::Haiku.model_id(), "claude-haiku-4-5-20251001");
        assert_eq!(ModelChoice::Sonnet.model_id(), "claude-sonnet-4-6");
        assert_eq!(ModelChoice::Opus.model_id(), "claude-opus-4-6");
    }

    #[test]
    fn test_model_choice_relative_cost() {
        assert_eq!(ModelChoice::Haiku.relative_cost(), 1.0);
        assert_eq!(ModelChoice::Sonnet.relative_cost(), 5.0);
        assert_eq!(ModelChoice::Opus.relative_cost(), 20.0);
    }

    #[test]
    fn test_request_creation() {
        let req = Request::new("Hello");
        assert_eq!(req.text, "Hello");
        assert!(!req.has_code);
        assert_eq!(req.conversation_turn, 1);
        assert!(req.user_model_preference.is_none());
    }

    #[test]
    fn test_request_builder() {
        let req = Request::new("Show me the code")
            .with_code(true)
            .with_conversation_turn(3)
            .with_model_preference(ModelChoice::Opus);

        assert!(req.has_code);
        assert_eq!(req.conversation_turn, 3);
        assert_eq!(req.user_model_preference, Some(ModelChoice::Opus));
    }

    #[test]
    fn test_estimate_tokens() {
        let short = Request::new("Hi");
        assert_eq!(short.estimate_tokens(), 1); // 2 chars / 4 = 0, but min 1

        let medium = Request::new("a".repeat(400));
        assert_eq!(medium.estimate_tokens(), 100); // 400 / 4 = 100

        let long = Request::new("a".repeat(10000));
        assert_eq!(long.estimate_tokens(), 2500); // 10000 / 4 = 2500
    }

    #[test]
    fn test_complexity_detection_simple() {
        let router = SimpleRouter::new(Default::default());

        let simple = Request::new("What is 2+2?");
        assert_eq!(router.classify_complexity(&simple), TaskComplexity::Simple);
    }

    #[test]
    fn test_complexity_detection_code() {
        let router = SimpleRouter::new(Default::default());

        let with_code = Request::new("Fix my code").with_code(true);
        assert!(router.classify_complexity(&with_code) >= TaskComplexity::Medium);
    }

    #[test]
    fn test_complexity_detection_long() {
        let router = SimpleRouter::new(Default::default());

        // 8000 chars = 2000 tokens (hits the medium_task_threshold of 2000)
        let long = Request::new("a".repeat(8000));
        assert_eq!(router.classify_complexity(&long), TaskComplexity::Complex);
    }

    #[test]
    fn test_complexity_detection_conversation() {
        let router = SimpleRouter::new(Default::default());

        // Multi-turn (turn > 3) is automatically complex
        let multi_turn = Request::new("Next step?").with_conversation_turn(5);
        assert_eq!(
            router.classify_complexity(&multi_turn),
            TaskComplexity::Complex
        );
    }

    #[test]
    fn test_complexity_indicators() {
        let router = SimpleRouter::new(Default::default());

        // Keywords alone trigger complexity regardless of length
        let arch_request = Request::new("Design the system architecture");
        assert_eq!(
            router.classify_complexity(&arch_request),
            TaskComplexity::Complex
        );

        let optimize_request = Request::new("Optimize this for performance");
        assert_eq!(
            router.classify_complexity(&optimize_request),
            TaskComplexity::Complex
        );
    }

    #[test]
    fn test_routing_user_preference_override() {
        let router = SimpleRouter::new(Default::default());

        let request = Request::new("a".repeat(2000)).with_model_preference(ModelChoice::Haiku);

        assert_eq!(router.route(&request), ModelChoice::Haiku);
    }

    #[test]
    fn test_routing_simple_task_balanced() {
        let router = SimpleRouter::new(Default::default());

        let request = Request::new("What's the capital of France?");
        assert_eq!(router.route(&request), ModelChoice::Haiku);
    }

    #[test]
    fn test_routing_medium_task_balanced() {
        let router = SimpleRouter::new(Default::default());

        // 2000 chars = 500 tokens (between 500 and 2000, so Medium)
        let request = Request::new("a".repeat(2000)).with_code(false);
        assert_eq!(router.route(&request), ModelChoice::Sonnet);
    }

    #[test]
    fn test_routing_complex_task_balanced() {
        let router = SimpleRouter::new(Default::default());

        // 8000 chars = 2000 tokens (hits the medium_task_threshold, triggers Complex)
        let request = Request::new("a".repeat(8000));
        assert_eq!(router.route(&request), ModelChoice::Opus);
    }

    #[test]
    fn test_routing_cost_optimized() {
        let config = RouterConfig {
            prefer_cost: true,
            ..Default::default()
        };
        let router = SimpleRouter::new(config);

        // Even complex (8000 chars = 2000 tokens) should prefer cheaper
        let request = Request::new("a".repeat(8000));
        assert_eq!(router.route(&request), ModelChoice::Sonnet);
    }

    #[test]
    fn test_routing_speed_optimized() {
        let config = RouterConfig {
            prefer_speed: true,
            ..Default::default()
        };
        let router = SimpleRouter::new(config);

        let simple = Request::new("Quick question");
        assert_eq!(router.route(&simple), ModelChoice::Haiku);

        // 8000 chars = 2000 tokens = Complex in speed-optimized
        let complex = Request::new("a".repeat(8000));
        assert_eq!(router.route(&complex), ModelChoice::Opus);
    }

    #[test]
    fn test_routing_capability_optimized() {
        let config = RouterConfig {
            prefer_capability: true,
            ..Default::default()
        };
        let router = SimpleRouter::new(config);

        // Even simple should get premium
        let simple = Request::new("Quick question");
        assert_eq!(router.route(&simple), ModelChoice::Sonnet);

        let complex = Request::new("a".repeat(2500));
        assert_eq!(router.route(&complex), ModelChoice::Opus);
    }

    #[test]
    fn test_routing_force_model() {
        let config = RouterConfig {
            force_model: Some(ModelChoice::Sonnet),
            ..Default::default()
        };
        let router = SimpleRouter::new(config);

        let simple = Request::new("Quick question");
        assert_eq!(router.route(&simple), ModelChoice::Sonnet);

        let complex = Request::new("a".repeat(2500));
        assert_eq!(router.route(&complex), ModelChoice::Sonnet);
    }

    #[test]
    fn test_router_strategy_name() {
        let balanced = SimpleRouter::new(Default::default());
        assert_eq!(balanced.strategy_name(), "balanced");

        let cost = SimpleRouter::new(RouterConfig {
            prefer_cost: true,
            ..Default::default()
        });
        assert_eq!(cost.strategy_name(), "cost-optimized");

        let speed = SimpleRouter::new(RouterConfig {
            prefer_speed: true,
            ..Default::default()
        });
        assert_eq!(speed.strategy_name(), "speed-optimized");

        let capability = SimpleRouter::new(RouterConfig {
            prefer_capability: true,
            ..Default::default()
        });
        assert_eq!(capability.strategy_name(), "capability-optimized");
    }

    #[test]
    fn test_custom_thresholds() {
        let config = RouterConfig {
            simple_task_threshold: 100, // < 100 tokens = simple
            medium_task_threshold: 500, // < 500 tokens = medium, else complex
            ..Default::default()
        };
        let router = SimpleRouter::new(config);

        // 400 chars = 100 tokens, exactly at simple_task_threshold, so Medium
        let request = Request::new("a".repeat(400));
        assert_eq!(router.classify_complexity(&request), TaskComplexity::Medium);

        // 2000 chars = 500 tokens, exactly at medium_task_threshold, so Complex
        let request2 = Request::new("a".repeat(2000));
        assert_eq!(
            router.classify_complexity(&request2),
            TaskComplexity::Complex
        );
    }

    #[test]
    fn test_edge_case_empty_request() {
        let router = SimpleRouter::new(Default::default());
        let empty = Request::new("");
        assert_eq!(router.route(&empty), ModelChoice::Haiku);
    }

    #[test]
    fn test_edge_case_very_large_request() {
        let router = SimpleRouter::new(Default::default());
        let huge = Request::new("a".repeat(100000));
        assert_eq!(router.route(&huge), ModelChoice::Opus);
    }

    #[test]
    fn test_integration_code_detection() {
        let router = SimpleRouter::new(Default::default());

        // Code snippet - should be at least medium
        let code_request = Request::new(
            r#"
            fn hello_world() {
                println!("Hello, world!");
            }
            "#,
        )
        .with_code(true);

        assert!(router.route(&code_request) >= ModelChoice::Sonnet);
    }

    #[test]
    fn test_multiple_complexity_factors() {
        let router = SimpleRouter::new(Default::default());

        // Long + code + multi-turn = definitely complex
        let request = Request::new("a".repeat(1500))
            .with_code(true)
            .with_conversation_turn(4);

        assert_eq!(router.route(&request), ModelChoice::Opus);
    }

    #[test]
    fn test_task_complexity_comparison() {
        assert!(TaskComplexity::Simple < TaskComplexity::Medium);
        assert!(TaskComplexity::Medium < TaskComplexity::Complex);
        assert_eq!(TaskComplexity::Simple, TaskComplexity::Simple);
    }
}
