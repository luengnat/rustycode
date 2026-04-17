// rustycode-orchestra/src/complexity.rs
//! Complexity classification for Orchestra v2
//!
//! Classifies units (phases/slices/tasks) as Light, Standard, or Heavy
//! to determine appropriate model tier selection.

use serde::{Deserialize, Serialize};

/// Complexity level of a unit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Complexity {
    /// Light - Simple, well-scoped changes
    Light,
    /// Standard - Moderate complexity
    Standard,
    /// Heavy - Complex, multi-file changes
    Heavy,
}

/// Model tier for complexity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ModelTier {
    /// Budget tier - Fast, cheap (Haiku)
    Budget,
    /// Balanced tier - Balanced quality/cost (Sonnet)
    Balanced,
    /// Quality tier - Highest quality (Opus)
    Quality,
}

/// Complexity classifier
pub struct ComplexityClassifier;

impl ComplexityClassifier {
    /// Classify a unit based on its characteristics
    pub fn classify(unit: &Unit) -> Complexity {
        let mut score = 0i32;

        // File count (0-30 points)
        score += (unit.file_count.min(10) * 3) as i32;

        // Lines changed (0-30 points)
        score += (unit.lines_changed.min(1000) / 40) as i32;

        // Dependencies (0-15 points)
        score += (unit.dependencies.len() * 5) as i32;

        // Test requirements (0-10 points)
        score += (unit.test_requirements.len() * 3) as i32;

        // Integration points (0-10 points)
        score += (unit.integration_points.len() * 2) as i32;

        // Risk level (0-5 points)
        score += match unit.risk_level {
            RiskLevel::Low => 0,
            RiskLevel::Medium => 2,
            RiskLevel::High => 5,
        };

        // Classify based on score
        match score {
            s if s <= 30 => Complexity::Light,
            s if s <= 70 => Complexity::Standard,
            _ => Complexity::Heavy,
        }
    }

    /// Select model tier based on complexity
    pub fn select_model_tier(complexity: Complexity) -> ModelTier {
        match complexity {
            Complexity::Light => ModelTier::Budget,
            Complexity::Standard => ModelTier::Balanced,
            Complexity::Heavy => ModelTier::Quality,
        }
    }

    /// Get recommended model for complexity level
    pub fn recommended_model(complexity: Complexity) -> &'static str {
        match complexity {
            Complexity::Light => "haiku-4.5",
            Complexity::Standard => "sonnet-4.6",
            Complexity::Heavy => "opus-4.6",
        }
    }
}

/// Unit to be classified
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    /// Unit ID (e.g., "01-foundation", "01-01")
    pub id: String,
    /// Unit type (phase, slice, or task)
    pub unit_type: UnitType,
    /// Number of files to modify
    pub file_count: usize,
    /// Lines of code to change
    pub lines_changed: usize,
    /// Dependencies on other units
    pub dependencies: Vec<String>,
    /// Test requirements
    pub test_requirements: Vec<String>,
    /// Integration points (APIs, services, etc.)
    pub integration_points: Vec<String>,
    /// Risk level
    pub risk_level: RiskLevel,
    /// Unit description
    pub description: String,
}

/// Unit type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnitType {
    /// Phase - High-level project phase
    Phase,
    /// Slice - Subdivision of a phase
    Slice,
    /// Task - Individual task
    Task,
    /// Research phase
    Research,
    /// Planning phase
    Planning,
    /// Completion phase
    Completion,
    /// Validation phase
    Validation,
}

/// Risk level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum RiskLevel {
    /// Low risk - Simple, isolated changes
    Low,
    /// Medium risk - Moderate complexity or integration
    Medium,
    /// High risk - Complex, critical path, or high integration
    High,
}

impl Unit {
    /// Create a new unit
    pub fn new(id: String, unit_type: UnitType, description: String) -> Self {
        Self {
            id,
            unit_type,
            file_count: 0,
            lines_changed: 0,
            dependencies: Vec::new(),
            test_requirements: Vec::new(),
            integration_points: Vec::new(),
            risk_level: RiskLevel::Low,
            description,
        }
    }

    /// Estimate complexity from file count
    pub fn estimate_from_files(mut self, file_count: usize) -> Self {
        self.file_count = file_count;
        // Rough estimate: 100 lines per file
        self.lines_changed = file_count * 100;
        self
    }

    /// Set risk level
    pub fn with_risk(mut self, risk: RiskLevel) -> Self {
        self.risk_level = risk;
        self
    }

    /// Add dependency
    pub fn with_dependency(mut self, dep: String) -> Self {
        self.dependencies.push(dep);
        self
    }

    /// Add integration point
    pub fn with_integration(mut self, integration: String) -> Self {
        self.integration_points.push(integration);
        self
    }

    /// Add test requirement
    pub fn with_test(mut self, test: String) -> Self {
        self.test_requirements.push(test);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_light_unit() {
        let unit = Unit::new(
            "01-01".to_string(),
            UnitType::Task,
            "Add simple function".to_string(),
        )
        .estimate_from_files(1);

        let complexity = ComplexityClassifier::classify(&unit);
        assert_eq!(complexity, Complexity::Light);
    }

    #[test]
    fn test_classify_standard_unit() {
        let unit = Unit::new(
            "01-02".to_string(),
            UnitType::Task,
            "Implement feature with integration".to_string(),
        )
        .estimate_from_files(5)
        .with_integration("API".to_string())
        .with_dependency("01-01".to_string());

        let complexity = ComplexityClassifier::classify(&unit);
        assert_eq!(complexity, Complexity::Standard);
    }

    #[test]
    fn test_classify_heavy_unit() {
        let unit = Unit::new(
            "02-01".to_string(),
            UnitType::Slice,
            "Refactor core architecture".to_string(),
        )
        .estimate_from_files(25) // Increased from 20 to push over Heavy threshold
        .with_risk(RiskLevel::High)
        .with_integration("Database".to_string())
        .with_integration("API".to_string())
        .with_integration("Auth".to_string()) // Added third integration
        .with_dependency("01-03".to_string())
        .with_dependency("01-02".to_string()); // Added second dependency

        let complexity = ComplexityClassifier::classify(&unit);
        assert_eq!(complexity, Complexity::Heavy);
    }

    #[test]
    fn test_model_tier_selection() {
        assert_eq!(
            ComplexityClassifier::select_model_tier(Complexity::Light),
            ModelTier::Budget
        );
        assert_eq!(
            ComplexityClassifier::select_model_tier(Complexity::Standard),
            ModelTier::Balanced
        );
        assert_eq!(
            ComplexityClassifier::select_model_tier(Complexity::Heavy),
            ModelTier::Quality
        );
    }

    #[test]
    fn test_recommended_model() {
        assert_eq!(
            ComplexityClassifier::recommended_model(Complexity::Light),
            "haiku-4.5"
        );
        assert_eq!(
            ComplexityClassifier::recommended_model(Complexity::Standard),
            "sonnet-4.6"
        );
        assert_eq!(
            ComplexityClassifier::recommended_model(Complexity::Heavy),
            "opus-4.6"
        );
    }
}
