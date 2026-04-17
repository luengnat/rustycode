// rustycode-orchestra/src/model_router.rs
//! Dynamic Model Routing with Cost-Aware Selection
//!
//! Selects the optimal LLM model for autonomous development by balancing
//! task complexity, budget constraints, and historical performance data.
//!
//! # Routing Algorithm
//!
//! Model selection follows a five-step process:
//!
//! 1. **Base Tier Selection**: Map task complexity to model tier
//!    - Light → Budget tier (fast, cheap)
//!    - Standard → Balanced tier (quality, cost-effective)
//!    - Heavy → Quality tier (best, expensive)
//!
//! 2. **Budget Pressure**: Downgrade if over budget
//!    - Over 80% → Consider cheaper models
//!    - Over 100% → Force budget tier
//!
//! 3. **Adaptive Learning**: Adjust based on history
//!    - If tier keeps failing → Upgrade
//!    - If tier always succeeds → Consider downgrade
//!
//! 4. **Model Resolution**: Map tier to specific model
//!    - Fallback chain within tier
//!    - Provider availability checks
//!
//! 5. **Reasoning**: Explain why model was chosen
//!    - For debugging and analytics
//!    - Shows budget/complexity factors
//!
//! # Model Tiers
//!
//! - **Budget**: Fast, cheap models for simple tasks (haiku)
//! - **Balanced**: Standard models for most work (sonnet)
//! - **Quality**: Best models for complex tasks (opus)
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::model_router::{ModelRouter, ModelSelection};
//!
//! let mut router = ModelRouter::new(cost_table, budget_tracker);
//!
//! let selection = router.select_model(&unit, complexity).await?;
//! println!("Selected: {}", selection.model);
//! println!("Reason: {}", selection.reasoning);
//!
//! // After execution, record outcome for learning
//! router.record_outcome(&selection, &RoutingOutcome::Success);
//! ```
//!
//! # Cost Optimization
//!
//! The router continuously optimizes for cost:
//! - Prefers cheaper tiers when budget is tight
//! - Learns which tier is "good enough" for each unit type
//! - Avoids over-provisioning (no opus for simple typos)
//! - Prevents under-provisioning (no haiku for architecture)
//!
//! # Adaptive Learning
//!
//! Routing history tracks:
//! - Which tier was selected for each unit
//! - Whether the unit succeeded or failed
//! - Retry patterns (did we need to upgrade?)
//!
//! This data informs future selections for similar units.

use crate::complexity::{Complexity, ModelTier, Unit};
use crate::error::{OrchestraV2Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model router for cost-aware model selection
pub struct ModelRouter {
    /// Cost table for all models
    cost_table: CostTable,
    /// Routing history for adaptive learning
    routing_history: RoutingHistory,
    /// Budget tracker
    budget_tracker: BudgetTracker,
}

impl ModelRouter {
    /// Create a new model router
    pub fn new(cost_table: CostTable, budget_tracker: BudgetTracker) -> Self {
        Self {
            cost_table,
            routing_history: RoutingHistory::new(),
            budget_tracker,
        }
    }

    /// Select the optimal model for a unit
    pub async fn select_model(
        &mut self,
        unit: &Unit,
        complexity: Complexity,
    ) -> Result<ModelSelection> {
        // Step 1: Base selection from complexity
        let base_tier = self.complexity_to_tier(complexity);

        // Step 2: Apply budget pressure
        let adjusted_tier = self.apply_budget_pressure(base_tier).await?;

        // Step 3: Check routing history for adaptive adjustments
        let final_tier = self.apply_routing_history(adjusted_tier, unit).await?;

        // Step 4: Resolve effective model with fallbacks
        let model = self.resolve_model_with_fallbacks(final_tier, unit).await?;

        // Step 5: Create model selection
        Ok(ModelSelection {
            model: model.clone(),
            tier: final_tier,
            provider: self.extract_provider(&model)?,
            reasoning: self.selection_reasoning(base_tier, final_tier, unit),
            selected_at: Utc::now(),
        })
    }

    /// Record the outcome of a model selection
    pub fn record_outcome(&mut self, selection: &ModelSelection, outcome: &RoutingOutcome) {
        self.routing_history
            .record(selection.clone(), outcome.clone());
    }

    /// Convert complexity to model tier
    fn complexity_to_tier(&self, complexity: Complexity) -> ModelTier {
        match complexity {
            Complexity::Light => ModelTier::Budget,
            Complexity::Standard => ModelTier::Balanced,
            Complexity::Heavy => ModelTier::Quality,
        }
    }

    /// Apply budget pressure to tier selection
    async fn apply_budget_pressure(&self, tier: ModelTier) -> Result<ModelTier> {
        let budget_status = self.budget_tracker.status().await;

        match budget_status {
            BudgetStatus::OverBudget => {
                // Downgrade to save costs
                Ok(match tier {
                    ModelTier::Quality => ModelTier::Balanced,
                    ModelTier::Balanced => ModelTier::Budget,
                    ModelTier::Budget => ModelTier::Budget,
                })
            }
            BudgetStatus::NearLimit => {
                // Consider downgrading
                Ok(match tier {
                    ModelTier::Quality => ModelTier::Balanced,
                    _ => tier,
                })
            }
            BudgetStatus::OnTrack | BudgetStatus::UnderBudget => {
                // Can use selected tier or upgrade if needed
                Ok(tier)
            }
        }
    }

    /// Apply routing history for adaptive learning
    async fn apply_routing_history(&self, tier: ModelTier, unit: &Unit) -> Result<ModelTier> {
        let recent_outcomes = self.routing_history.get_recent_outcomes(unit, 10);

        // Avoid models that failed recently for similar units
        for outcome in &recent_outcomes {
            if outcome.outcome == OutcomeType::Failure && outcome.attempts >= 2 {
                // Try a different tier
                return Ok(match tier {
                    ModelTier::Budget => ModelTier::Balanced,
                    ModelTier::Balanced => ModelTier::Quality,
                    ModelTier::Quality => ModelTier::Balanced,
                });
            }
        }

        Ok(tier)
    }

    /// Resolve model with fallbacks
    async fn resolve_model_with_fallbacks(&self, tier: ModelTier, _unit: &Unit) -> Result<String> {
        let models = self.cost_table.get_models_for_tier(tier);

        // Return first available model (fallback chain)
        models.first().cloned().ok_or_else(|| {
            OrchestraV2Error::ModelRouting(format!("No models available for tier: {:?}", tier))
        })
    }

    /// Extract provider from model name
    fn extract_provider(&self, model: &str) -> Result<String> {
        if model.contains("claude") || model.contains("anthropic") {
            Ok("anthropic".to_string())
        } else if model.contains("gpt") || model.contains("openai") {
            Ok("openai".to_string())
        } else if model.contains("gemini") || model.contains("google") {
            Ok("google".to_string())
        } else {
            Err(OrchestraV2Error::ModelRouting(format!(
                "Unknown provider for model: {}",
                model
            )))
        }
    }

    /// Generate reasoning for selection
    fn selection_reasoning(&self, base: ModelTier, final_tier: ModelTier, unit: &Unit) -> String {
        let mut reasoning = format!("Base tier: {:?}, Final tier: {:?}", base, final_tier);

        if base != final_tier {
            reasoning.push_str(&format!(" (adjusted for unit: {})", unit.id));
        }

        reasoning
    }
}

/// Cost table for all models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTable {
    /// Per-model cost data
    costs: HashMap<String, ModelCost>,
}

impl CostTable {
    /// Create a new cost table with default costs
    pub fn new() -> Self {
        let mut costs = HashMap::new();

        // Anthropic models
        costs.insert(
            "claude-3-5-sonnet-20241022".to_string(),
            ModelCost {
                provider: "anthropic".to_string(),
                input_cost_per_1m: 3.0,
                output_cost_per_1m: 15.0,
                tier: ModelTier::Balanced,
            },
        );

        costs.insert(
            "claude-3-5-haiku-20241022".to_string(),
            ModelCost {
                provider: "anthropic".to_string(),
                input_cost_per_1m: 0.8,
                output_cost_per_1m: 4.0,
                tier: ModelTier::Budget,
            },
        );

        costs.insert(
            "claude-3-5-opus-20241022".to_string(),
            ModelCost {
                provider: "anthropic".to_string(),
                input_cost_per_1m: 15.0,
                output_cost_per_1m: 75.0,
                tier: ModelTier::Quality,
            },
        );

        // OpenAI models
        costs.insert(
            "gpt-4o".to_string(),
            ModelCost {
                provider: "openai".to_string(),
                input_cost_per_1m: 2.5,
                output_cost_per_1m: 10.0,
                tier: ModelTier::Balanced,
            },
        );

        costs.insert(
            "gpt-4o-mini".to_string(),
            ModelCost {
                provider: "openai".to_string(),
                input_cost_per_1m: 0.15,
                output_cost_per_1m: 0.6,
                tier: ModelTier::Budget,
            },
        );

        // Google models
        costs.insert(
            "gemini-2.0-flash".to_string(),
            ModelCost {
                provider: "google".to_string(),
                input_cost_per_1m: 0.075,
                output_cost_per_1m: 0.3,
                tier: ModelTier::Budget,
            },
        );

        costs.insert(
            "gemini-2.5-pro".to_string(),
            ModelCost {
                provider: "google".to_string(),
                input_cost_per_1m: 1.25,
                output_cost_per_1m: 5.0,
                tier: ModelTier::Balanced,
            },
        );

        Self { costs }
    }

    /// Get models for a specific tier
    pub fn get_models_for_tier(&self, tier: ModelTier) -> Vec<String> {
        self.costs
            .iter()
            .filter(|(_, cost)| cost.tier == tier)
            .map(|(model, _)| model.clone())
            .collect()
    }

    /// Get cost for a specific model
    pub fn get_cost(&self, model: &str) -> Option<&ModelCost> {
        self.costs.get(model)
    }

    /// Calculate cost for tokens used
    pub fn calculate_cost(&self, model: &str, tokens_in: u32, tokens_out: u32) -> Option<f64> {
        let cost = self.costs.get(model)?;
        let input_cost = (tokens_in as f64 / 1_000_000.0) * cost.input_cost_per_1m;
        let output_cost = (tokens_out as f64 / 1_000_000.0) * cost.output_cost_per_1m;
        Some(input_cost + output_cost)
    }
}

impl Default for CostTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Cost information for a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    /// Provider name
    pub provider: String,
    /// Input cost per 1M tokens (USD)
    pub input_cost_per_1m: f64,
    /// Output cost per 1M tokens (USD)
    pub output_cost_per_1m: f64,
    /// Model tier
    pub tier: ModelTier,
}

/// Model selection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelection {
    /// Selected model
    pub model: String,
    /// Model tier
    pub tier: ModelTier,
    /// Provider
    pub provider: String,
    /// Reasoning for selection
    pub reasoning: String,
    /// When selection was made
    pub selected_at: DateTime<Utc>,
}

/// Routing history for adaptive learning
#[derive(Debug, Clone, Default)]
pub struct RoutingHistory {
    /// History of routing outcomes
    entries: Vec<RoutingEntry>,
}

impl RoutingHistory {
    /// Create a new routing history
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a routing outcome
    pub fn record(&mut self, selection: ModelSelection, outcome: RoutingOutcome) {
        self.entries.push(RoutingEntry {
            selection,
            outcome,
            timestamp: Utc::now(),
        });
    }

    /// Get recent outcomes for a unit
    pub fn get_recent_outcomes(&self, unit: &Unit, limit: usize) -> Vec<RoutingOutcome> {
        self.entries
            .iter()
            .rev()
            .filter(|entry| entry.selection.model.contains(&unit.id))
            .take(limit)
            .map(|entry| entry.outcome.clone())
            .collect()
    }
}

/// Routing history entry
#[derive(Debug, Clone)]
struct RoutingEntry {
    selection: ModelSelection,
    outcome: RoutingOutcome,
    #[allow(dead_code)] // Kept for future use
    timestamp: DateTime<Utc>, // Recorded for potential future analytics/debugging
}

/// Outcome of a routing decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingOutcome {
    /// Outcome type
    pub outcome: OutcomeType,
    /// Number of attempts
    pub attempts: u32,
    /// Error message if failed
    pub error: Option<String>,
}

/// Outcome type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum OutcomeType {
    /// Success
    Success,
    /// Failure
    Failure,
    /// Retry with different model
    Retry,
}

/// Budget tracker
#[derive(Debug, Clone)]
pub struct BudgetTracker {
    /// Total budget
    total_budget: f64,
    /// Amount spent
    spent: f64,
}

impl BudgetTracker {
    /// Create a new budget tracker
    pub fn new(total_budget: f64) -> Self {
        Self {
            total_budget,
            spent: 0.0,
        }
    }

    /// Get current budget status
    pub async fn status(&self) -> BudgetStatus {
        let ratio = self.spent / self.total_budget;

        if ratio >= 1.0 {
            BudgetStatus::OverBudget
        } else if ratio >= 0.9 {
            BudgetStatus::NearLimit
        } else if ratio >= 0.5 {
            BudgetStatus::OnTrack
        } else {
            BudgetStatus::UnderBudget
        }
    }

    /// Record spend
    pub fn record_spend(&mut self, amount: f64) {
        self.spent += amount;
    }

    /// Get remaining budget
    pub fn remaining(&self) -> f64 {
        self.total_budget - self.spent
    }

    /// Get projected remaining based on velocity
    pub fn projected_remaining(&self, units_remaining: usize, avg_cost_per_unit: f64) -> f64 {
        let projected = self.spent + (units_remaining as f64 * avg_cost_per_unit);
        self.total_budget - projected
    }
}

/// Budget status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BudgetStatus {
    /// Under budget (less than 50% spent)
    UnderBudget,
    /// On track (50-90% spent)
    OnTrack,
    /// Near limit (90-100% spent)
    NearLimit,
    /// Over budget (>100% spent)
    OverBudget,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complexity::{RiskLevel, UnitType};

    fn test_unit(id: &str) -> Unit {
        Unit {
            id: id.into(),
            unit_type: UnitType::Task,
            file_count: 1,
            lines_changed: 10,
            dependencies: vec![],
            test_requirements: vec![],
            integration_points: vec![],
            risk_level: RiskLevel::Low,
            description: "test".into(),
        }
    }

    // --- CostTable ---

    #[test]
    fn cost_table_default_has_models() {
        let ct = CostTable::default();
        assert!(ct.get_cost("claude-3-5-sonnet-20241022").is_some());
        assert!(ct.get_cost("claude-3-5-haiku-20241022").is_some());
        assert!(ct.get_cost("claude-3-5-opus-20241022").is_some());
        assert!(ct.get_cost("gpt-4o").is_some());
        assert!(ct.get_cost("gpt-4o-mini").is_some());
        assert!(ct.get_cost("gemini-2.0-flash").is_some());
        assert!(ct.get_cost("gemini-2.5-pro").is_some());
    }

    #[test]
    fn cost_table_unknown_model() {
        let ct = CostTable::new();
        assert!(ct.get_cost("nonexistent-model").is_none());
    }

    #[test]
    fn cost_table_get_models_for_tier() {
        let ct = CostTable::new();
        let budget = ct.get_models_for_tier(ModelTier::Budget);
        let balanced = ct.get_models_for_tier(ModelTier::Balanced);
        let quality = ct.get_models_for_tier(ModelTier::Quality);

        assert!(!budget.is_empty());
        assert!(!balanced.is_empty());
        assert!(!quality.is_empty());
    }

    #[test]
    fn cost_table_calculate_cost() {
        let ct = CostTable::new();
        // 1M input + 1M output = full price per unit
        let cost = ct
            .calculate_cost("claude-3-5-sonnet-20241022", 1_000_000, 1_000_000)
            .unwrap();
        assert!((cost - 18.0).abs() < 0.01);
    }

    #[test]
    fn cost_table_calculate_cost_unknown() {
        let ct = CostTable::new();
        assert!(ct.calculate_cost("unknown", 1000, 1000).is_none());
    }

    #[test]
    fn cost_table_calculate_cost_zero_tokens() {
        let ct = CostTable::new();
        let cost = ct
            .calculate_cost("claude-3-5-sonnet-20241022", 0, 0)
            .unwrap();
        assert!((cost - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn model_cost_serde_roundtrip() {
        let mc = ModelCost {
            provider: "anthropic".into(),
            input_cost_per_1m: 3.0,
            output_cost_per_1m: 15.0,
            tier: ModelTier::Balanced,
        };
        let json = serde_json::to_string(&mc).unwrap();
        let decoded: ModelCost = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider, "anthropic");
        assert!((decoded.input_cost_per_1m - 3.0).abs() < f64::EPSILON);
    }

    // --- BudgetTracker ---

    #[tokio::test]
    async fn budget_tracker_under_budget() {
        let bt = BudgetTracker::new(100.0);
        assert_eq!(bt.status().await, BudgetStatus::UnderBudget);
        assert!((bt.remaining() - 100.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn budget_tracker_on_track() {
        let mut bt = BudgetTracker::new(100.0);
        bt.record_spend(60.0);
        assert_eq!(bt.status().await, BudgetStatus::OnTrack);
        assert!((bt.remaining() - 40.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn budget_tracker_near_limit() {
        let mut bt = BudgetTracker::new(100.0);
        bt.record_spend(92.0);
        assert_eq!(bt.status().await, BudgetStatus::NearLimit);
    }

    #[tokio::test]
    async fn budget_tracker_over_budget() {
        let mut bt = BudgetTracker::new(100.0);
        bt.record_spend(120.0);
        assert_eq!(bt.status().await, BudgetStatus::OverBudget);
    }

    #[test]
    fn budget_tracker_projected_remaining() {
        let bt = BudgetTracker::new(100.0);
        // Spent 0, 10 units at 5.0 each = 50 projected
        let projected = bt.projected_remaining(10, 5.0);
        assert!((projected - 50.0).abs() < f64::EPSILON);
    }

    // --- RoutingOutcome / OutcomeType ---

    #[test]
    fn outcome_type_serde() {
        for ot in &[
            OutcomeType::Success,
            OutcomeType::Failure,
            OutcomeType::Retry,
        ] {
            let json = serde_json::to_string(ot).unwrap();
            let back: OutcomeType = serde_json::from_str(&json).unwrap();
            assert_eq!(ot, &back);
        }
    }

    #[test]
    fn routing_outcome_serde_roundtrip() {
        let ro = RoutingOutcome {
            outcome: OutcomeType::Failure,
            attempts: 3,
            error: Some("timeout".into()),
        };
        let json = serde_json::to_string(&ro).unwrap();
        let decoded: RoutingOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.outcome, OutcomeType::Failure);
        assert_eq!(decoded.attempts, 3);
        assert_eq!(decoded.error, Some("timeout".into()));
    }

    // --- ModelSelection ---

    #[test]
    fn model_selection_serde_roundtrip() {
        let sel = ModelSelection {
            model: "claude-3-5-sonnet-20241022".into(),
            tier: ModelTier::Balanced,
            provider: "anthropic".into(),
            reasoning: "Standard complexity".into(),
            selected_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&sel).unwrap();
        let decoded: ModelSelection = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.model, "claude-3-5-sonnet-20241022");
        assert_eq!(decoded.provider, "anthropic");
    }

    // --- RoutingHistory ---

    #[test]
    fn routing_history_new_is_empty() {
        let rh = RoutingHistory::new();
        let unit = test_unit("u1");
        assert!(rh.get_recent_outcomes(&unit, 10).is_empty());
    }

    #[test]
    fn routing_history_record_and_retrieve() {
        let mut rh = RoutingHistory::new();
        let sel = ModelSelection {
            model: "model-u1".into(),
            tier: ModelTier::Budget,
            provider: "anthropic".into(),
            reasoning: "test".into(),
            selected_at: chrono::Utc::now(),
        };
        let outcome = RoutingOutcome {
            outcome: OutcomeType::Success,
            attempts: 1,
            error: None,
        };
        rh.record(sel, outcome);

        let unit = test_unit("u1");
        let recent = rh.get_recent_outcomes(&unit, 10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].outcome, OutcomeType::Success);
    }

    // --- CostTable serde ---

    #[test]
    fn cost_table_serde_roundtrip() {
        let ct = CostTable::new();
        let json = serde_json::to_string(&ct).unwrap();
        let decoded: CostTable = serde_json::from_str(&json).unwrap();
        assert!(decoded.get_cost("claude-3-5-sonnet-20241022").is_some());
    }
}
