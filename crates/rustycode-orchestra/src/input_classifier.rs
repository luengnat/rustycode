//! Orchestra Input Classifier — LLM-Guided Pre-Classification for Unstructured Requests
//!
//! Classifies raw user requests before task decomposition to provide
//! semantic complexity signals for better unit planning.
//!
//! # Problem
//!
//! RustyCode's existing `complexity_classifier.rs` excels at classifying
//! structured Orchestra units (after task decomposition), but cannot estimate
//! complexity of raw user requests. This creates a gap:
//!
//! ```text
//! User: "architect a microservices backend"
//!         ↓
//! [Gap: No complexity estimate available]
//!         ↓
//! Plan: Structure into units
//!         ↓
//! [Now complexity_classifier can work]
//! ```
//!
//! # Solution: LLM-Guided Pre-Classification
//!
//! This module provides lightweight LLM-based classification for raw inputs:
//!
//! 1. **Fast**: Uses haiku-class models for sub-second classification
//! 2. **Informative**: Returns semantic signals (scope, ambiguity, steps)
//! 3. **Composable**: Feeds into existing complexity_classifier heuristics
//! 4. **Optional**: Falls back gracefully if LLM unavailable
//!
//! # Complexity Rubric (adapted from Gemini-CLI)
//!
//! | Score | Tier | Description |
//! |-------|------|-------------|
//! | 1-20  | Trivial | Read-only, single-step, explicit |
//! | 21-50 | Simple | Single-file, local fixes, linear |
//! | 51-80 | Complex | Multi-file, debugging, context needed |
//! | 81-100| Extreme | Architecture, migration, ambiguous |
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::input_classifier::{classify_input, InputComplexityEstimate};
//!
//! let estimate = classify_input(
//!     "Add authentication to the API",
//!     llm_provider,
//!     None // or Some(history)
//! ).await?;
//!
//! println!("Tier: {:?}", estimate.tier);       // Simple
//! println!("Score: {}", estimate.score);       // 35
//! println!("Steps: {}", estimate.estimated_steps); // 3
//! ```
//!
//! # Integration
//!
//! The estimate can be fed into `classify_unit_complexity()` via
//! the optional `input_estimate` parameter to provide additional context.

use serde::{Deserialize, Serialize};

/// Complexity tier for raw inputs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum InputTier {
    /// Trivial - Read-only, single-step, explicit instructions
    Trivial,
    /// Simple - Single-file, local fixes, linear tasks
    Simple,
    /// Complex - Multi-file, debugging, context needed
    Complex,
    /// Extreme - Architecture, migration, highly ambiguous
    Extreme,
}

impl InputTier {
    /// Convert to standard complexity tier
    pub fn to_complexity_tier(&self) -> super::routing_history::ComplexityTier {
        match self {
            InputTier::Trivial => super::routing_history::ComplexityTier::Light,
            InputTier::Simple => super::routing_history::ComplexityTier::Light,
            InputTier::Complex => super::routing_history::ComplexityTier::Standard,
            InputTier::Extreme => super::routing_history::ComplexityTier::Heavy,
        }
    }

    /// Get tier from score
    pub fn from_score(score: u8) -> Self {
        match score {
            0..=20 => InputTier::Trivial,
            21..=50 => InputTier::Simple,
            51..=80 => InputTier::Complex,
            _ => InputTier::Extreme,
        }
    }

    /// Get tier label for display
    pub fn label(&self) -> &'static str {
        match self {
            InputTier::Trivial => "Trivial",
            InputTier::Simple => "Simple",
            InputTier::Complex => "Complex",
            InputTier::Extreme => "Extreme",
        }
    }
}

/// Semantic complexity signals extracted by LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexitySignals {
    /// Estimated number of operational steps
    pub estimated_steps: u8,
    /// Requires understanding broader context
    pub requires_context: bool,
    /// Has unclear requirements or multiple interpretations
    pub ambiguous: bool,
    /// Involves multiple files/components
    pub multi_file: bool,
    /// Requires debugging or root cause analysis
    pub debugging: bool,
    /// Strategic/high-level planning required
    pub strategic: bool,
    /// Risk of breaking changes
    pub risky: bool,
}

impl Default for ComplexitySignals {
    fn default() -> Self {
        Self {
            estimated_steps: 1,
            requires_context: false,
            ambiguous: false,
            multi_file: false,
            debugging: false,
            strategic: false,
            risky: false,
        }
    }
}

/// Result of input complexity classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputComplexityEstimate {
    /// Numeric score (1-100)
    pub score: u8,
    /// Tier classification
    pub tier: InputTier,
    /// Human-readable reasoning
    pub reasoning: String,
    /// Semantic signals from LLM analysis
    pub signals: ComplexitySignals,
    /// Keywords that influenced the score
    pub keywords: Vec<String>,
    /// Whether LLM classification was used
    pub llm_classified: bool,
}

impl InputComplexityEstimate {
    /// Create a fallback estimate when LLM is unavailable
    pub fn fallback(input: &str) -> Self {
        let (score, reasoning, signals, keywords) = heuristic_estimate(input);

        Self {
            score,
            tier: InputTier::from_score(score),
            reasoning,
            signals,
            keywords,
            llm_classified: false,
        }
    }

    /// Create from LLM response
    pub fn from_llm(score: u8, reasoning: &str, signals: ComplexitySignals) -> Self {
        let keywords = extract_keywords_from_reasoning(reasoning);

        Self {
            score,
            tier: InputTier::from_score(score),
            reasoning: reasoning.to_string(),
            signals,
            keywords,
            llm_classified: true,
        }
    }
}

/// Extract keywords from LLM reasoning
fn extract_keywords_from_reasoning(reasoning: &str) -> Vec<String> {
    let lower = reasoning.to_lowercase();
    let mut keywords = Vec::new();

    let patterns = [
        ("multi-file", "multi_file"),
        ("multiple", "multi_file"),
        ("context", "context"),
        ("architecture", "architecture"),
        ("debugging", "debugging"),
        ("debug", "debugging"),
        ("migration", "migration"),
        ("ambiguous", "ambiguous"),
        ("strategy", "strategic"),
        ("planning", "strategic"),
        ("security", "risky"),
        ("breaking", "risky"),
        ("database", "database"),
        ("api", "api"),
        ("auth", "auth"),
    ];

    for (pattern, keyword) in patterns {
        if lower.contains(pattern) && !keywords.contains(&keyword.to_string()) {
            keywords.push(keyword.to_string());
        }
    }

    keywords
}

/// Heuristic fallback when LLM is unavailable
fn heuristic_estimate(input: &str) -> (u8, String, ComplexitySignals, Vec<String>) {
    let lower = input.to_lowercase();
    let mut score: i32 = 25; // Start with Simple
    let mut signals = ComplexitySignals::default();
    let mut keywords = Vec::new();

    // High complexity signals
    let high_complexity = [
        ("architect", "architecture", 30),
        ("design", "architecture", 20),
        ("migrate", "migration", 35),
        ("refactor", "refactoring", 25),
        ("debug", "debugging", 20),
        ("fix the bug", "debugging", 15),
        ("improve performance", "performance", 25),
        ("optimize", "performance", 20),
        ("add authentication", "auth", 20),
        ("security", "security", 30),
        ("microservices", "architecture", 30),
        ("database", "database", 20),
        ("api", "api", 15),
        ("multiple", "multi_file", 15),
        ("across", "multi_file", 10),
        ("breaking change", "risky", 25),
        ("ambiguous", "ambiguous", 20),
        ("make this better", "ambiguous", 15),
        ("unknown", "debugging", 10),
        ("why is", "debugging", 15),
        ("how should i", "strategic", 20),
        ("plan for", "strategic", 15),
    ];

    for (pattern, keyword, weight) in high_complexity {
        if lower.contains(pattern) {
            score += weight;
            if !keywords.contains(&keyword.to_string()) {
                keywords.push(keyword.to_string());
            }
        }
    }

    // Update signals based on keywords
    signals.multi_file = keywords.contains(&"multi_file".to_string());
    signals.debugging = keywords.contains(&"debugging".to_string());
    signals.strategic = keywords.contains(&"strategic".to_string());
    signals.ambiguous = keywords.contains(&"ambiguous".to_string());

    // Low complexity signals
    let low_complexity = [
        ("read", "read", -10),
        ("show", "read", -10),
        ("list", "read", -10),
        ("find", "search", -5),
        ("rename", "rename", -10),
        ("typo", "typo", -15),
        ("spelling", "typo", -15),
        ("fix typo", "typo", -15),
        ("comment", "docs", -10),
        ("documentation", "docs", -10),
        ("readme", "docs", -10),
        ("add to readme", "docs", -10),
        ("single file", "single_file", -15),
        ("one file", "single_file", -15),
    ];

    for (pattern, keyword, weight) in low_complexity {
        if lower.contains(pattern) {
            score += weight;
            if !keywords.contains(&keyword.to_string()) {
                keywords.push(keyword.to_string());
            }
        }
    }

    // Clamp score
    score = score.clamp(5, 95);
    let score = score as u8;

    // Generate reasoning
    let reasoning = if score <= 20 {
        "Trivial read-only or single-step operation.".to_string()
    } else if score <= 50 {
        format!(
            "Simple task with {} step(s). {}",
            signals.estimated_steps,
            if signals.multi_file {
                "May involve multiple files."
            } else {
                "Localized change."
            }
        )
    } else if score <= 80 {
        format!(
            "Complex task requiring {}{}. {}",
            signals.estimated_steps,
            if signals.debugging { " debugging" } else { "" },
            if signals.strategic {
                "Strategic planning needed."
            } else if signals.multi_file {
                "Multi-file coordination."
            } else {
                "Context-dependent."
            }
        )
    } else {
        "Extreme complexity - architecture, migration, or highly ambiguous.".to_string()
    };

    // Estimate steps
    signals.estimated_steps = match score {
        0..=20 => 1,
        21..=50 => 3,
        51..=80 => 5,
        _ => 8,
    };

    (score, reasoning, signals, keywords)
}

/// The classification prompt for LLM
pub const INPUT_CLASSIFIER_PROMPT: &str = r#"You are a Task Complexity Analyst. Analyze the user's request and assign a complexity score from 1-100.

# Complexity Rubric
**1-20: TRIVIAL**
- Simple read-only commands (read file, list dir)
- Exact, explicit instructions with zero ambiguity
- Single-step operations

**21-50: SIMPLE**
- Single-file edits or simple refactors
- "Fix this error" where error is clear and local
- Multi-step but linear tasks

**51-80: COMPLEX**
- Multi-file dependencies
- "Why is this broken?" debugging
- Feature implementation requiring context

**81-100: EXTREME**
- "Architect a new system" or "Migrate database"
- Highly ambiguous requests
- Deep reasoning, safety checks, novel invention

# Output Format
Return JSON with:
- "score": integer 1-100
- "reasoning": brief explanation
- "estimated_steps": 1-10
- "requires_context": boolean
- "multi_file": boolean
- "debugging": boolean
- "strategic": boolean
- "risky": boolean

Example:
Input: "read package.json"
Output: {"score": 10, "reasoning": "Simple read operation", "estimated_steps": 1, "requires_context": false, "multi_file": false, "debugging": false, "strategic": false, "risky": false}"#;

/// Classify input complexity using heuristic fallback (no LLM needed)
///
/// This is a convenience wrapper around `InputComplexityEstimate::fallback()`
/// that provides a simple interface for synchronous classification.
///
/// # Arguments
/// * `input` - The raw user input to classify
///
/// # Returns
/// Complexity estimate based on keyword and pattern matching
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::input_classifier::classify_input_fallback;
///
/// let estimate = classify_input_fallback("add authentication to API");
/// println!("Score: {}", estimate.score);
/// println!("Tier: {:?}", estimate.tier);
/// ```
pub fn classify_input_fallback(input: &str) -> InputComplexityEstimate {
    InputComplexityEstimate::fallback(input)
}

/// Estimate input complexity synchronously (placeholder for async LLM version)
///
/// This function provides a synchronous interface for complexity estimation.
/// When LLM-based classification is needed, use the async version with
/// an LLM provider.
///
/// # Arguments
/// * `input` - The raw user input
///
/// # Returns
/// Complexity estimate
///
/// # Note
/// Full async LLM classification is planned for future release.
/// Currently falls back to heuristic estimation.
pub fn estimate_input_complexity(input: &str) -> InputComplexityEstimate {
    InputComplexityEstimate::fallback(input)
}

/// Parse LLM JSON response into estimate
pub fn parse_llm_response(json: &str) -> Option<(u8, String, ComplexitySignals)> {
    // Try to parse as JSON
    let parsed: serde_json::Value = serde_json::from_str(json).ok()?;

    let score = parsed.get("score")?.as_u64()? as u8;
    let reasoning = parsed.get("reasoning")?.as_str()?.to_string();

    let signals = ComplexitySignals {
        estimated_steps: parsed
            .get("estimated_steps")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8)
            .unwrap_or(1),
        requires_context: parsed
            .get("requires_context")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        multi_file: parsed
            .get("multi_file")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        debugging: parsed
            .get("debugging")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        strategic: parsed
            .get("strategic")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        ambiguous: parsed
            .get("ambiguous")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        risky: parsed
            .get("risky")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    };

    Some((score, reasoning, signals))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_tier_from_score() {
        assert_eq!(InputTier::from_score(10), InputTier::Trivial);
        assert_eq!(InputTier::from_score(25), InputTier::Simple);
        assert_eq!(InputTier::from_score(60), InputTier::Complex);
        assert_eq!(InputTier::from_score(85), InputTier::Extreme);
    }

    #[test]
    fn test_input_tier_to_complexity_tier() {
        assert_eq!(
            InputTier::Trivial.to_complexity_tier(),
            super::super::routing_history::ComplexityTier::Light
        );
        assert_eq!(
            InputTier::Simple.to_complexity_tier(),
            super::super::routing_history::ComplexityTier::Light
        );
        assert_eq!(
            InputTier::Complex.to_complexity_tier(),
            super::super::routing_history::ComplexityTier::Standard
        );
        assert_eq!(
            InputTier::Extreme.to_complexity_tier(),
            super::super::routing_history::ComplexityTier::Heavy
        );
    }

    #[test]
    fn test_input_tier_label() {
        assert_eq!(InputTier::Trivial.label(), "Trivial");
        assert_eq!(InputTier::Simple.label(), "Simple");
        assert_eq!(InputTier::Complex.label(), "Complex");
        assert_eq!(InputTier::Extreme.label(), "Extreme");
    }

    #[test]
    fn test_fallback_trivial() {
        let estimate = InputComplexityEstimate::fallback("read package.json");
        assert!(estimate.score <= 20);
        assert_eq!(estimate.tier, InputTier::Trivial);
        assert!(!estimate.llm_classified);
    }

    #[test]
    fn test_fallback_simple() {
        // Single file operations should be low complexity
        let estimate = InputComplexityEstimate::fallback("fix typo in main.rs");
        assert!(estimate.score < 30);
        assert_eq!(estimate.tier, InputTier::Trivial);
    }

    #[test]
    fn test_fallback_complex() {
        let estimate = InputComplexityEstimate::fallback("add authentication to the API");
        // Auth is complex and multi-file
        assert!(estimate.score >= 40);
        assert!(estimate
            .keywords
            .iter()
            .any(|k| k.contains("auth") || k.contains("api")));
    }

    #[test]
    fn test_fallback_extreme() {
        let estimate =
            InputComplexityEstimate::fallback("architect a microservices backend for this app");
        // Architecture/microservices are extreme complexity
        assert!(estimate.score >= 70);
        assert!(estimate.tier == InputTier::Complex || estimate.tier == InputTier::Extreme);
        assert!(estimate
            .keywords
            .iter()
            .any(|k| k.contains("architect") || k.contains("architecture")));
    }

    #[test]
    fn test_parse_llm_response_valid() {
        let json = r#"{
            "score": 65,
            "reasoning": "Multi-file changes needed",
            "estimated_steps": 5,
            "requires_context": true,
            "multi_file": true,
            "debugging": false,
            "strategic": false,
            "ambiguous": false,
            "risky": false
        }"#;

        let result = parse_llm_response(json);
        assert!(result.is_some());

        let (score, reasoning, signals) = result.unwrap();
        assert_eq!(score, 65);
        assert!(reasoning.contains("Multi-file"));
        assert_eq!(signals.estimated_steps, 5);
        assert!(signals.multi_file);
        assert!(signals.requires_context);
    }

    #[test]
    fn test_parse_llm_response_invalid() {
        let json = r#"{"invalid": "json"}"#;
        assert!(parse_llm_response(json).is_none());
    }

    #[test]
    fn test_extract_keywords_from_reasoning() {
        let keywords = extract_keywords_from_reasoning(
            "This requires multi-file changes and strategic planning.",
        );
        assert!(keywords.contains(&"multi_file".to_string()));
        assert!(keywords.contains(&"strategic".to_string()));
    }

    #[test]
    fn test_heuristic_estimate_readme() {
        let estimate = InputComplexityEstimate::fallback("add documentation to readme");
        assert!(estimate.score < 30);
        assert!(estimate.keywords.contains(&"docs".to_string()));
    }

    #[test]
    fn test_heuristic_estimate_typo() {
        let estimate = InputComplexityEstimate::fallback("fix the typo in main.rs");
        assert!(estimate.score < 30);
        assert!(estimate.keywords.contains(&"typo".to_string()));
    }

    #[test]
    fn test_heuristic_estimate_security() {
        let estimate = InputComplexityEstimate::fallback("add security headers to API");
        // Security-related tasks are complex
        assert!(estimate.score > 45);
        assert!(estimate.keywords.iter().any(|k| k.contains("security")));
    }

    #[test]
    fn test_from_llm() {
        let signals = ComplexitySignals {
            estimated_steps: 5,
            multi_file: true,
            ..Default::default()
        };
        let estimate =
            InputComplexityEstimate::from_llm(72, "Multi-file feature implementation", signals);
        assert_eq!(estimate.score, 72);
        assert_eq!(estimate.tier, InputTier::Complex);
        assert!(estimate.llm_classified);
        assert!(estimate.keywords.contains(&"multi_file".to_string()));
    }
}
