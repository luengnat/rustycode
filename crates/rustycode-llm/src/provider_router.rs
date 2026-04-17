//! Multi-Provider LLM Router
//!
//! Routes LLM requests across multiple providers with configurable strategies.
//! Supports failover, cost optimization, and capability-based routing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Re-export provider types for convenience
pub use crate::provider_v2::{CompletionRequest, CompletionResponse, LLMProvider, ProviderError};

/// Routing strategy for selecting providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
#[non_exhaustive]
pub enum RoutingStrategy {
    /// Always use the primary provider
    Primary,
    /// Round-robin across available providers
    RoundRobin,
    /// Route to the cheapest provider
    CostOptimized,
    /// Route based on required capabilities (streaming, tools, etc.)
    Capability,
    /// Try primary, fall back to secondary on failure
    #[default]
    Failover,
}

/// Provider configuration for the router
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider name (e.g., "anthropic", "openai")
    pub name: String,
    /// Priority (lower = higher priority)
    pub priority: u32,
    /// Whether this provider is enabled
    pub enabled: bool,
    /// Cost per 1K input tokens (in cents)
    pub cost_per_1k_input: Option<f32>,
    /// Cost per 1K output tokens (in cents)
    pub cost_per_1k_output: Option<f32>,
    /// Supported capabilities
    pub capabilities: Vec<String>,
    /// Maximum context window
    pub max_context: Option<usize>,
}

impl ProviderConfig {
    /// Check if this provider supports a specific capability
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    /// Check if this provider supports all required capabilities
    pub fn supports_capabilities(&self, required: &[String]) -> bool {
        required.iter().all(|cap| self.has_capability(cap))
    }

    /// Get total cost per 1K tokens (input + output average)
    pub fn cost_per_1k_total(&self) -> Option<f32> {
        match (self.cost_per_1k_input, self.cost_per_1k_output) {
            (Some(input), Some(output)) => Some((input + output) / 2.0),
            (Some(input), None) => Some(input),
            (None, Some(output)) => Some(output),
            (None, None) => None,
        }
    }
}

/// Result of provider selection
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Selected provider name
    pub provider: String,
    /// Why this provider was chosen
    pub reason: String,
    /// Strategy used
    pub strategy: RoutingStrategy,
}

/// Requirements for a routing decision
#[derive(Debug, Clone, Default)]
pub struct RoutingRequirements {
    pub required_capabilities: Vec<String>,
    pub max_context_needed: Option<usize>,
    pub prefer_low_cost: bool,
}

impl RoutingRequirements {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.required_capabilities.push(cap.into());
        self
    }

    pub fn with_context_size(mut self, size: usize) -> Self {
        self.max_context_needed = Some(size);
        self
    }

    pub fn prefer_low_cost(mut self, prefer: bool) -> Self {
        self.prefer_low_cost = prefer;
        self
    }
}

/// The multi-provider router
#[derive(Debug)]
pub struct ProviderRouter {
    providers: HashMap<String, ProviderConfig>,
    strategy: RoutingStrategy,
    round_robin_index: AtomicUsize,
}

impl ProviderRouter {
    /// Create a new router with the specified strategy
    pub fn new(strategy: RoutingStrategy) -> Self {
        Self {
            providers: HashMap::new(),
            strategy,
            round_robin_index: AtomicUsize::new(0),
        }
    }

    /// Add a provider to the router
    pub fn add_provider(&mut self, config: ProviderConfig) {
        self.providers.insert(config.name.clone(), config);
    }

    /// Add multiple providers to the router
    pub fn add_providers(&mut self, configs: impl IntoIterator<Item = ProviderConfig>) {
        for config in configs {
            self.add_provider(config);
        }
    }

    /// Select a provider based on the current strategy
    pub fn select(&self, requirements: &RoutingRequirements) -> Option<RoutingDecision> {
        let available: Vec<_> = self
            .providers
            .values()
            .filter(|p| p.enabled)
            .filter(|p| {
                // Filter by context size if required
                if let Some(needed) = requirements.max_context_needed {
                    p.max_context.is_none_or(|max| max >= needed)
                } else {
                    true
                }
            })
            .collect();

        if available.is_empty() {
            return None;
        }

        match self.strategy {
            RoutingStrategy::Primary => self.select_primary(&available),
            RoutingStrategy::RoundRobin => self.select_round_robin(&available),
            RoutingStrategy::CostOptimized => self.select_cheapest(&available),
            RoutingStrategy::Capability => self.select_by_capability(&available, requirements),
            RoutingStrategy::Failover => self.select_primary(&available),
            #[allow(unreachable_patterns)]
            _ => self.select_primary(&available),
        }
    }

    /// Get the next fallback provider after a failure
    pub fn get_fallback(&self, failed_provider: &str) -> Option<RoutingDecision> {
        let available: Vec<_> = self
            .providers
            .values()
            .filter(|p| p.enabled && p.name != failed_provider)
            .collect();

        available
            .iter()
            .min_by_key(|p| p.priority)
            .map(|p| RoutingDecision {
                provider: p.name.clone(),
                reason: format!("Fallback after {} failure", failed_provider),
                strategy: RoutingStrategy::Failover,
            })
    }

    /// Get a provider by name
    pub fn get_provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }

    /// List all enabled providers
    pub fn enabled_providers(&self) -> Vec<&ProviderConfig> {
        let mut providers: Vec<_> = self.providers.values().filter(|p| p.enabled).collect();
        providers.sort_by_key(|p| p.priority);
        providers
    }

    /// Get the current strategy
    pub fn strategy(&self) -> &RoutingStrategy {
        &self.strategy
    }

    /// Set the strategy
    pub fn set_strategy(&mut self, strategy: RoutingStrategy) {
        self.strategy = strategy;
    }

    /// Check if a provider exists and is enabled
    pub fn is_provider_enabled(&self, name: &str) -> bool {
        self.providers.get(name).map(|p| p.enabled).unwrap_or(false)
    }

    /// Enable or disable a provider
    pub fn set_provider_enabled(&mut self, name: &str, enabled: bool) -> bool {
        if let Some(provider) = self.providers.get_mut(name) {
            provider.enabled = enabled;
            true
        } else {
            false
        }
    }

    fn select_primary(&self, available: &[&ProviderConfig]) -> Option<RoutingDecision> {
        available
            .iter()
            .min_by_key(|p| p.priority)
            .map(|p| RoutingDecision {
                provider: p.name.clone(),
                reason: "Primary provider (highest priority)".into(),
                strategy: self.strategy.clone(),
            })
    }

    fn select_round_robin(&self, available: &[&ProviderConfig]) -> Option<RoutingDecision> {
        let mut sorted: Vec<_> = available.iter().collect();
        sorted.sort_by_key(|p| p.priority);

        let idx = self
            .round_robin_index
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |x| {
                Some((x + 1) % sorted.len().max(1))
            })
            .ok()?;

        sorted.get(idx).map(|p| RoutingDecision {
            provider: p.name.clone(),
            reason: format!("Round-robin selection (index {})", idx),
            strategy: RoutingStrategy::RoundRobin,
        })
    }

    fn select_cheapest(&self, available: &[&ProviderConfig]) -> Option<RoutingDecision> {
        available
            .iter()
            .filter(|p| p.cost_per_1k_input.is_some())
            .min_by(|a, b| {
                a.cost_per_1k_total()
                    .partial_cmp(&b.cost_per_1k_total())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| RoutingDecision {
                provider: p.name.clone(),
                reason: format!(
                    "Cost optimized (${:.2}/1K tokens)",
                    p.cost_per_1k_total().unwrap_or(0.0)
                ),
                strategy: RoutingStrategy::CostOptimized,
            })
            .or_else(|| self.select_primary(available))
    }

    fn select_by_capability(
        &self,
        available: &[&ProviderConfig],
        requirements: &RoutingRequirements,
    ) -> Option<RoutingDecision> {
        // Filter providers that support all required capabilities
        let capable: Vec<_> = available
            .iter()
            .filter(|p| p.supports_capabilities(&requirements.required_capabilities))
            .collect();

        if capable.is_empty() {
            // No provider supports all capabilities, fall back to primary
            return self.select_primary(available);
        }

        capable
            .iter()
            .min_by_key(|p| p.priority)
            .map(|p| RoutingDecision {
                provider: p.name.clone(),
                reason: format!(
                    "Capability match for: {:?}",
                    requirements.required_capabilities
                ),
                strategy: RoutingStrategy::Capability,
            })
    }
}

/// Build a router with default providers configured
///
/// This creates a router with common providers pre-configured with
/// realistic priorities, costs, and capabilities.
///
/// # Example
///
/// ```ignore
/// use rustycode_llm::provider_router::default_router;
///
/// let router = default_router();
/// let decision = router.select(&Default::default()).unwrap();
/// assert_eq!(decision.provider, "anthropic"); // Priority 1
/// ```
pub fn default_router() -> ProviderRouter {
    let mut router = ProviderRouter::new(RoutingStrategy::Failover);

    router.add_provider(ProviderConfig {
        name: "anthropic".into(),
        priority: 1,
        enabled: true,
        cost_per_1k_input: Some(0.25),
        cost_per_1k_output: Some(1.25),
        capabilities: vec![
            "streaming".into(),
            "tools".into(),
            "vision".into(),
            "extended_thinking".into(),
        ],
        max_context: Some(200_000),
    });

    router.add_provider(ProviderConfig {
        name: "openai".into(),
        priority: 2,
        enabled: true,
        cost_per_1k_input: Some(0.15),
        cost_per_1k_output: Some(0.60),
        capabilities: vec!["streaming".into(), "tools".into(), "vision".into()],
        max_context: Some(128_000),
    });

    router.add_provider(ProviderConfig {
        name: "gemini".into(),
        priority: 3,
        enabled: true,
        cost_per_1k_input: Some(0.075),
        cost_per_1k_output: Some(0.30),
        capabilities: vec!["streaming".into(), "tools".into(), "vision".into()],
        max_context: Some(1_000_000),
    });

    router.add_provider(ProviderConfig {
        name: "ollama".into(),
        priority: 10,
        enabled: false, // Disabled by default (local-only)
        cost_per_1k_input: Some(0.0),
        cost_per_1k_output: Some(0.0),
        capabilities: vec!["streaming".into(), "tools".into()],
        max_context: Some(32_000),
    });

    router
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_router() -> ProviderRouter {
        let mut router = ProviderRouter::new(RoutingStrategy::Primary);
        router.add_provider(ProviderConfig {
            name: "primary".into(),
            priority: 1,
            enabled: true,
            cost_per_1k_input: Some(0.25),
            cost_per_1k_output: Some(1.25),
            capabilities: vec!["streaming".into(), "tools".into()],
            max_context: Some(200_000),
        });
        router.add_provider(ProviderConfig {
            name: "secondary".into(),
            priority: 2,
            enabled: true,
            cost_per_1k_input: Some(0.10),
            cost_per_1k_output: Some(0.50),
            capabilities: vec!["streaming".into()],
            max_context: Some(128_000),
        });
        router
    }

    #[test]
    fn test_primary_selection() {
        let router = test_router();
        let decision = router.select(&RoutingRequirements::new()).unwrap();
        assert_eq!(decision.provider, "primary");
        assert_eq!(decision.strategy, RoutingStrategy::Primary);
    }

    #[test]
    fn test_failover() {
        let router = test_router();
        let fallback = router.get_fallback("primary").unwrap();
        assert_eq!(fallback.provider, "secondary");
        assert_eq!(fallback.strategy, RoutingStrategy::Failover);
    }

    #[test]
    fn test_capability_routing() {
        let mut router = test_router();
        router.set_strategy(RoutingStrategy::Capability);

        // Request tools capability - only primary has it
        let req = RoutingRequirements::new().with_capability("tools");
        let decision = router.select(&req).unwrap();
        assert_eq!(decision.provider, "primary");

        // Request streaming - both have it, should pick primary (lower priority)
        let req = RoutingRequirements::new().with_capability("streaming");
        let decision = router.select(&req).unwrap();
        assert_eq!(decision.provider, "primary");
    }

    #[test]
    fn test_capability_routing_missing_capability() {
        let mut router = test_router();
        router.set_strategy(RoutingStrategy::Capability);

        // Request a capability neither has - should fall back to primary
        let req = RoutingRequirements::new().with_capability("vision");
        let decision = router.select(&req).unwrap();
        assert_eq!(decision.provider, "primary");
    }

    #[test]
    fn test_cost_routing() {
        let mut router = test_router();
        router.set_strategy(RoutingStrategy::CostOptimized);

        let decision = router.select(&RoutingRequirements::new()).unwrap();
        assert_eq!(decision.provider, "secondary"); // cheaper
        assert_eq!(decision.strategy, RoutingStrategy::CostOptimized);
    }

    #[test]
    fn test_round_robin_routing() {
        let mut router = test_router();
        router.set_strategy(RoutingStrategy::RoundRobin);

        // First call
        let decision1 = router.select(&RoutingRequirements::new()).unwrap();
        assert_eq!(decision1.strategy, RoutingStrategy::RoundRobin);

        // Second call should get different index (mod 2)
        let decision2 = router.select(&RoutingRequirements::new()).unwrap();
        assert_eq!(decision2.strategy, RoutingStrategy::RoundRobin);
    }

    #[test]
    fn test_context_size_filtering() {
        let router = test_router();

        // Request small context - both providers should be available
        let req = RoutingRequirements::new().with_context_size(100_000);
        let decision = router.select(&req).unwrap();
        assert!(decision.provider == "primary" || decision.provider == "secondary");

        // Request large context - only primary supports it
        let req = RoutingRequirements::new().with_context_size(150_000);
        let decision = router.select(&req).unwrap();
        assert_eq!(decision.provider, "primary");
    }

    #[test]
    fn test_disabled_provider() {
        let mut router = test_router();
        router.set_provider_enabled("primary", false);

        let decision = router.select(&RoutingRequirements::new()).unwrap();
        assert_eq!(decision.provider, "secondary");
    }

    #[test]
    fn test_no_enabled_providers() {
        let mut router = test_router();
        router.set_provider_enabled("primary", false);
        router.set_provider_enabled("secondary", false);

        let decision = router.select(&RoutingRequirements::new());
        assert!(decision.is_none());
    }

    #[test]
    fn test_default_router() {
        let router = default_router();
        let providers = router.enabled_providers();

        // Should have 3 enabled providers (anthropic, openai, gemini)
        assert_eq!(providers.len(), 3);
        assert_eq!(providers[0].name, "anthropic");
        assert_eq!(providers[1].name, "openai");
        assert_eq!(providers[2].name, "gemini");

        // Ollama should be disabled
        assert!(!router.is_provider_enabled("ollama"));
    }

    #[test]
    fn test_provider_config_capabilities() {
        let config = ProviderConfig {
            name: "test".into(),
            priority: 1,
            enabled: true,
            cost_per_1k_input: Some(0.25),
            cost_per_1k_output: Some(1.25),
            capabilities: vec!["streaming".into(), "tools".into()],
            max_context: Some(200_000),
        };

        assert!(config.has_capability("streaming"));
        assert!(config.has_capability("tools"));
        assert!(!config.has_capability("vision"));

        assert!(config.supports_capabilities(&["streaming".into(), "tools".into()]));
        assert!(!config.supports_capabilities(&["streaming".into(), "vision".into()]));
    }

    #[test]
    fn test_provider_config_cost() {
        let config = ProviderConfig {
            name: "test".into(),
            priority: 1,
            enabled: true,
            cost_per_1k_input: Some(0.25),
            cost_per_1k_output: Some(1.25),
            capabilities: vec![],
            max_context: None,
        };

        let total = config.cost_per_1k_total().unwrap();
        assert!((total - 0.75).abs() < 0.01); // (0.25 + 1.25) / 2
    }

    #[test]
    fn test_routing_requirements_builder() {
        let req = RoutingRequirements::new()
            .with_capability("tools")
            .with_capability("streaming")
            .with_context_size(100_000)
            .prefer_low_cost(true);

        assert_eq!(req.required_capabilities.len(), 2);
        assert_eq!(req.max_context_needed, Some(100_000));
        assert!(req.prefer_low_cost);
    }

    #[test]
    fn test_get_provider() {
        let router = test_router();

        let provider = router.get_provider("primary");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name, "primary");

        let missing = router.get_provider("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_set_provider_enabled() {
        let mut router = test_router();

        assert!(router.is_provider_enabled("primary"));

        // Disable provider
        assert!(router.set_provider_enabled("primary", false));
        assert!(!router.is_provider_enabled("primary"));

        // Enable provider
        assert!(router.set_provider_enabled("primary", true));
        assert!(router.is_provider_enabled("primary"));

        // Try to modify non-existent provider
        assert!(!router.set_provider_enabled("nonexistent", true));
    }

    #[test]
    fn test_add_providers_batch() {
        let mut router = ProviderRouter::new(RoutingStrategy::Primary);

        let configs = vec![
            ProviderConfig {
                name: "provider1".into(),
                priority: 1,
                enabled: true,
                cost_per_1k_input: Some(0.25),
                cost_per_1k_output: Some(1.25),
                capabilities: vec![],
                max_context: None,
            },
            ProviderConfig {
                name: "provider2".into(),
                priority: 2,
                enabled: true,
                cost_per_1k_input: Some(0.10),
                cost_per_1k_output: Some(0.50),
                capabilities: vec![],
                max_context: None,
            },
        ];

        router.add_providers(configs);

        assert_eq!(router.enabled_providers().len(), 2);
    }

    #[test]
    fn test_routing_strategy_default() {
        let strategy = RoutingStrategy::default();
        assert_eq!(strategy, RoutingStrategy::Failover);
    }

    #[test]
    fn test_routing_strategy_serialization() {
        let strategy = RoutingStrategy::CostOptimized;
        let serialized = serde_json::to_string(&strategy).unwrap();
        assert_eq!(serialized, "\"cost_optimized\"");

        let deserialized: RoutingStrategy = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, RoutingStrategy::CostOptimized);
    }

    #[test]
    fn test_fallback_excludes_failed_provider() {
        let router = test_router();

        // Fallback from primary should not return primary
        let fallback = router.get_fallback("primary").unwrap();
        assert_ne!(fallback.provider, "primary");
        assert_eq!(fallback.provider, "secondary");
    }

    #[test]
    fn test_fallback_when_no_alternatives() {
        let mut router = ProviderRouter::new(RoutingStrategy::Failover);
        router.add_provider(ProviderConfig {
            name: "only".into(),
            priority: 1,
            enabled: true,
            cost_per_1k_input: Some(0.25),
            cost_per_1k_output: Some(1.25),
            capabilities: vec![],
            max_context: None,
        });

        let fallback = router.get_fallback("only");
        assert!(fallback.is_none());
    }
}
