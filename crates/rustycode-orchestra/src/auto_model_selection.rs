//! Orchestra Auto Model Selection — Model Selection and Dynamic Routing
//!
//! Selects the appropriate model for a unit dispatch.
//! Handles: per-unit-type model preferences, dynamic complexity routing,
//! provider/model resolution, fallback chains, and start-model re-application.
//!
//! Critical for cost-effective autonomous development.

use serde::{Deserialize, Serialize};

use crate::complexity_classifier::{classify_unit_complexity, ClassificationResult};
use crate::metrics::MetricsLedger;
use crate::routing_history::ComplexityTier;

// ─── Types ──────────────────────────────────────────────────────────────────────

/// Model selection result
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelectionResult {
    /// Routing metadata for metrics recording
    pub routing: Option<RoutingMetadata>,
}

/// Routing metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingMetadata {
    pub tier: ComplexityTier,
    pub model_downgraded: bool,
}

/// Model configuration with fallbacks
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub primary: String,
    pub fallbacks: Vec<String>,
}

impl PartialEq for ModelConfig {
    fn eq(&self, other: &Self) -> bool {
        self.primary == other.primary && self.fallbacks == other.fallbacks
    }
}

/// Dynamic routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicRoutingConfig {
    pub enabled: bool,
    pub budget_pressure: bool,
    pub hooks: bool,
}

impl PartialEq for DynamicRoutingConfig {
    fn eq(&self, other: &Self) -> bool {
        self.enabled == other.enabled
            && self.budget_pressure == other.budget_pressure
            && self.hooks == other.hooks
    }
}

impl Default for DynamicRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            budget_pressure: true,
            hooks: false,
        }
    }
}

/// Model resolution result
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResolution {
    pub model_id: String,
    pub was_downgraded: bool,
    pub fallbacks: Vec<String>,
}

/// Available model
#[derive(Debug, Clone)]
pub struct AvailableModel {
    pub id: String,
    pub provider: String,
}

impl PartialEq for AvailableModel {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.provider == other.provider
    }
}

impl Eq for AvailableModel {}

/// Orchestra preferences (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestraPreferences {
    pub budget_ceiling: Option<f64>,
}

impl PartialEq for OrchestraPreferences {
    fn eq(&self, other: &Self) -> bool {
        match (self.budget_ceiling, other.budget_ceiling) {
            (Some(a), Some(b)) => (a - b).abs() < 0.001,
            (None, None) => true,
            _ => false,
        }
    }
}

// ─── Unit Type → Model Config Mapping ───────────────────────────────────────────

/// Default model configurations for unit types
fn get_model_config_for_unit(unit_type: &str) -> Option<ModelConfig> {
    match unit_type {
        // Light units - use fast models
        "complete-slice" | "run-uat" => Some(ModelConfig {
            primary: "haiku-4.5".to_string(),
            fallbacks: vec!["sonnet-4.6".to_string()],
        }),

        // Standard units - use balanced models
        "research-milestone" | "research-slice" | "plan-milestone" | "plan-slice" => {
            Some(ModelConfig {
                primary: "sonnet-4.6".to_string(),
                fallbacks: vec!["haiku-4.5".to_string(), "opus-4.6".to_string()],
            })
        }

        // Heavy units - use quality models
        "replan-slice" | "reassess-roadmap" => Some(ModelConfig {
            primary: "opus-4.6".to_string(),
            fallbacks: vec!["sonnet-4.6".to_string()],
        }),

        // Execution tasks - dynamic based on complexity
        "execute-task" => Some(ModelConfig {
            primary: "sonnet-4.6".to_string(), // default, will be adjusted by complexity
            fallbacks: vec!["haiku-4.5".to_string(), "opus-4.6".to_string()],
        }),

        // Hook units - use fastest
        unit_type if unit_type.starts_with("hook/") => Some(ModelConfig {
            primary: "haiku-4.5".to_string(),
            fallbacks: vec!["sonnet-4.6".to_string()],
        }),

        _ => None,
    }
}

// ─── Public API ────────────────────────────────────────────────────────────────

/// Select and apply the appropriate model for a unit dispatch
///
/// Handles: per-unit-type model preferences, dynamic complexity routing,
/// provider/model resolution, fallback chains, and start-model re-application.
///
/// # Arguments
/// * `unit_type` - The type of unit being dispatched
/// * `unit_id` - The unit ID (e.g. "M01/S01/T01")
/// * `base_path` - Project base path
/// * `prefs` - Optional Orchestra preferences
/// * `routing_config` - Dynamic routing configuration
/// * `available_models` - List of available models
/// * `current_model` - Currently active model (for fallback)
/// * `ledger` - Optional metrics ledger for budget tracking
///
/// # Returns
/// Model selection result with routing metadata
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_model_selection::*;
///
/// let result = select_model_for_unit(
///     "execute-task",
///     "M01/S01/T01",
///     "/project",
///     None,
///     &DynamicRoutingConfig::default(),
///     &[],
///     None,
///     None
/// );
/// ```
pub fn select_model_for_unit(
    unit_type: &str,
    unit_id: &str,
    base_path: &str,
    prefs: Option<&OrchestraPreferences>,
    routing_config: &DynamicRoutingConfig,
    available_models: &[AvailableModel],
    current_model: Option<&AvailableModel>,
    ledger: Option<&MetricsLedger>,
) -> ModelSelectionResult {
    let model_config = match get_model_config_for_unit(unit_type) {
        Some(config) => config,
        None => {
            // No model preference for this unit type
            return ModelSelectionResult { routing: None };
        }
    };

    let mut routing: Option<RoutingMetadata> = None;
    let mut effective_model_config = model_config.clone();

    // ─── Dynamic Model Routing ─────────────────────────────────────────────────
    if routing_config.enabled {
        let budget_pct = if routing_config.budget_pressure {
            calculate_budget_percentage(prefs, ledger)
        } else {
            None
        };

        let is_hook = unit_type.starts_with("hook/");
        let should_classify = !is_hook || routing_config.hooks;

        if should_classify {
            let classification =
                classify_unit_complexity(unit_type, unit_id, base_path, budget_pct, None);
            let available_model_ids: Vec<String> =
                available_models.iter().map(|m| m.id.clone()).collect();

            let routing_result =
                resolve_model_for_complexity(&classification, &model_config, &available_model_ids);

            if routing_result.was_downgraded {
                effective_model_config = ModelConfig {
                    primary: routing_result.model_id.clone(),
                    fallbacks: routing_result.fallbacks.clone(),
                };
            }

            routing = Some(RoutingMetadata {
                tier: classification.tier,
                model_downgraded: routing_result.was_downgraded,
            });
        }
    }

    // Try primary and fallbacks
    let models_to_try = vec![&effective_model_config.primary]
        .into_iter()
        .chain(effective_model_config.fallbacks.iter())
        .collect::<Vec<_>>();

    for model_id in models_to_try {
        if let Some(_model) = resolve_model_id(model_id, available_models, current_model) {
            // Model found and would be set here
            // In actual implementation, this would call the LLM provider
            return ModelSelectionResult { routing };
        }
    }

    // All models failed - return with routing metadata anyway
    ModelSelectionResult { routing }
}

/// Calculate budget usage as percentage
///
/// # Arguments
/// * `prefs` - Orchestra preferences with optional budget ceiling
/// * `ledger` - Optional metrics ledger
///
/// # Returns
/// Budget usage as percentage (0.0-1.0+), or None if no budget ceiling set
fn calculate_budget_percentage(
    prefs: Option<&OrchestraPreferences>,
    ledger: Option<&MetricsLedger>,
) -> Option<f64> {
    let budget_ceiling = prefs?.budget_ceiling?;
    let budget_ceiling = if budget_ceiling > 0.0 {
        budget_ceiling
    } else {
        return None;
    };

    // Calculate total cost from ledger units
    let total_cost = match ledger {
        Some(l) => l.units.iter().map(|u| u.cost).sum::<f64>(),
        None => 0.0,
    };

    Some(total_cost / budget_ceiling)
}

/// Resolve model for a given complexity classification
///
/// # Arguments
/// * `classification` - Complexity classification result
/// * `model_config` - Base model configuration
/// * `available_model_ids` - List of available model IDs
///
/// # Returns
/// Model resolution result
fn resolve_model_for_complexity(
    classification: &ClassificationResult,
    model_config: &ModelConfig,
    available_model_ids: &[String],
) -> ModelResolution {
    let target_tier = classification.tier;
    let is_available = |model_id: &str| available_model_ids.contains(&model_id.to_string());

    // Map complexity tier to preferred models
    let preferred_models = match target_tier {
        ComplexityTier::Light => vec!["haiku-4.5", "sonnet-4.6"],
        ComplexityTier::Standard => vec!["sonnet-4.6", "haiku-4.5"],
        ComplexityTier::Heavy => vec!["opus-4.6", "sonnet-4.6"],
    };

    // Try to find an available model from preferred list
    for preferred in preferred_models {
        if is_available(preferred) {
            let was_downgraded = !model_config.primary.contains(preferred);
            return ModelResolution {
                model_id: preferred.to_string(),
                was_downgraded,
                fallbacks: model_config.fallbacks.clone(),
            };
        }
    }

    // Fall back to primary config
    ModelResolution {
        model_id: model_config.primary.clone(),
        was_downgraded: false,
        fallbacks: model_config.fallbacks.clone(),
    }
}

/// Resolve a model ID string to a model object from the available models list
///
/// Handles formats: "provider/model", "bare-id", "org/model-name" (OpenRouter).
///
/// # Arguments
/// * `model_id` - Model ID to resolve
/// * `available_models` - List of available models
/// * `current_provider` - Current provider (for bare ID resolution)
///
/// # Returns
/// Resolved model, or None if not found
fn resolve_model_id<'a, T>(
    model_id: &str,
    available_models: &'a [T],
    current_provider: Option<&T>,
) -> Option<&'a T>
where
    T: ModelLike,
{
    let slash_idx = model_id.find('/');

    if let Some(slash_idx) = slash_idx {
        let maybe_provider = &model_id[..slash_idx];
        let id = &model_id[slash_idx + 1..];

        let known_providers: std::collections::HashSet<String> = available_models
            .iter()
            .map(|m| m.provider().to_lowercase())
            .collect();

        if known_providers.contains(&maybe_provider.to_lowercase()) {
            let match_ = available_models.iter().find(|m| {
                m.provider().to_lowercase() == maybe_provider.to_lowercase()
                    && m.id().to_lowercase() == id.to_lowercase()
            });
            if match_.is_some() {
                return match_;
            }
        }

        // Try matching the full string as a model ID (OpenRouter-style)
        let lower = model_id.to_lowercase();
        return available_models.iter().find(|m| {
            m.id().to_lowercase() == lower
                || format!("{}/{}", m.provider(), m.id()).to_lowercase() == lower
        });
    }

    // Bare ID — prefer current provider, then first available
    let current_provider_str = current_provider.map(|m| m.provider());
    let exact_provider_match = available_models
        .iter()
        .find(|m| m.id() == model_id && current_provider_str == Some(m.provider()));

    exact_provider_match.or_else(|| available_models.iter().find(|m| m.id() == model_id))
}

/// Trait for model-like objects
pub trait ModelLike {
    fn id(&self) -> &str;
    fn provider(&self) -> &str;
}

impl ModelLike for AvailableModel {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider(&self) -> &str {
        &self.provider
    }
}

/// Get phase label for unit type (for UI display)
///
/// # Arguments
/// * `unit_type` - The unit type
///
/// # Returns
/// Human-readable phase label
pub fn unit_phase_label(unit_type: &str) -> &'static str {
    match unit_type {
        "research-milestone" => "Research",
        "research-slice" => "Research",
        "plan-milestone" => "Plan",
        "plan-slice" => "Plan",
        "execute-task" => "Execute",
        "complete-slice" => "Complete",
        "replan-slice" => "Replan",
        "reassess-roadmap" => "Reassess",
        "run-uat" => "UAT",
        ut if ut.starts_with("hook/") => "Hook",
        _ => "Unknown",
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_model_config_for_unit() {
        let config = get_model_config_for_unit("execute-task");
        assert!(config.is_some());
        assert_eq!(config.unwrap().primary, "sonnet-4.6");

        let config = get_model_config_for_unit("complete-slice");
        assert!(config.is_some());
        assert_eq!(config.unwrap().primary, "haiku-4.5");

        let config = get_model_config_for_unit("unknown-unit");
        assert!(config.is_none());
    }

    #[test]
    fn test_get_model_config_for_hook() {
        let config = get_model_config_for_unit("hook/pre-commit");
        assert!(config.is_some());
        assert_eq!(config.unwrap().primary, "haiku-4.5");
    }

    #[test]
    fn test_unit_phase_label() {
        assert_eq!(unit_phase_label("execute-task"), "Execute");
        assert_eq!(unit_phase_label("plan-slice"), "Plan");
        assert_eq!(unit_phase_label("complete-slice"), "Complete");
        assert_eq!(unit_phase_label("hook/pre-commit"), "Hook");
        assert_eq!(unit_phase_label("unknown"), "Unknown");
    }

    #[test]
    fn test_dynamic_routing_config_default() {
        let config = DynamicRoutingConfig::default();
        assert!(config.enabled);
        assert!(config.budget_pressure);
        assert!(!config.hooks);
    }

    #[test]
    fn test_calculate_budget_percentage_no_ceiling() {
        let prefs = OrchestraPreferences {
            budget_ceiling: None,
        };
        let result = calculate_budget_percentage(Some(&prefs), None);
        assert!(result.is_none());
    }

    #[test]
    fn test_calculate_budget_percentage_with_ceiling() {
        let prefs = OrchestraPreferences {
            budget_ceiling: Some(100.0),
        };
        let ledger = MetricsLedger {
            version: 1,
            project_started_at: 0,
            units: vec![],
        };

        let result = calculate_budget_percentage(Some(&prefs), Some(&ledger));
        // With empty ledger, cost should be 0
        assert_eq!(result.unwrap(), 0.0);
    }

    #[test]
    fn test_resolve_model_id_with_slash() {
        let models = vec![AvailableModel {
            id: "claude-3-5-sonnet-20241022".to_string(),
            provider: "anthropic".to_string(),
        }];

        let result = resolve_model_id("anthropic/claude-3-5-sonnet-20241022", &models, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().provider(), "anthropic");
    }

    #[test]
    fn test_resolve_model_id_bare() {
        let models = vec![AvailableModel {
            id: "gpt-4".to_string(),
            provider: "openai".to_string(),
        }];

        let result = resolve_model_id("gpt-4", &models, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id(), "gpt-4");
    }

    #[test]
    fn test_resolve_model_id_not_found() {
        let models = vec![AvailableModel {
            id: "gpt-4".to_string(),
            provider: "openai".to_string(),
        }];

        let result = resolve_model_id("claude-3", &models, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_model_selection_result() {
        let result = ModelSelectionResult {
            routing: Some(RoutingMetadata {
                tier: ComplexityTier::Standard,
                model_downgraded: false,
            }),
        };

        assert!(result.routing.is_some());
        assert_eq!(result.routing.unwrap().tier, ComplexityTier::Standard);
    }

    #[test]
    fn test_routing_metadata_serialize() {
        let metadata = RoutingMetadata {
            tier: ComplexityTier::Heavy,
            model_downgraded: true,
        };

        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("Heavy"));
        assert!(json.contains("true"));
    }
}
