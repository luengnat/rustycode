//! Orchestra Complexity Classifier — Unit Complexity Classification for Dynamic Model Routing
//!
//! Classifies unit complexity to determine which model tier to use.
//! Pure heuristics + adaptive learning — no LLM calls. Sub-millisecond classification.
//!
//! # Complexity Tiers
//!
//! Units are classified into three tiers:
//!
//! - **Basic** (Tier 1): Simple tasks, fast models (haiku)
//!   - Typo fixes, config changes, documentation
//!   - Single-file edits, well-understood changes
//!
//! - **Standard** (Tier 2): Normal tasks, balanced models (sonnet)
//!   - Feature implementation, refactoring
//!   - Multi-file changes, moderate complexity
//!
//! - **Complex** (Tier 3): Hard tasks, best models (opus)
//!   - Architecture changes, migrations
//!   - Security, performance, concurrency
//!
//! # Classification Heuristics
//!
//! The classifier analyzes:
//! - **Unit Type**: execute-task vs plan-slice vs validate-milestone
//! - **File Count**: More files = higher complexity
//! - **Keywords**: "migration", "security", "performance" etc.
//! - **Tags**: "refactor", "test", "docs" affect tier
//! - **Estimates**: Line count estimates from plan
//!
//! # Adaptive Learning
//!
//! The system learns from past executions:
//! - If a unit is retried, upgrade tier next time
//! - Track success rates per tier
//! - Adjust future classifications based on outcomes
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::complexity_classifier::{classify_unit_complexity, Unit};
//!
//! let unit = Unit {
//!     unit_id: "T01".to_string(),
//!     unit_type: "execute-task".to_string(),
//!     plan_path: "path/to/T01-PLAN.md".into(),
//! };
//!
//! let result = classify_unit_complexity(&unit)?;
//! println!("Tier: {:?}", result.tier);
//! println!("Reason: {}", result.reason);
//! ```
//!
//! # Performance
//!
//! Classification is **sub-millisecond**:
//! - No LLM calls
//! - Pure regex heuristics
//! - File system reads are cached
//! - Suitable for hot-path invocation

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::paths::orchestra_root;
use crate::routing_history::{get_adaptive_tier_adjustment, ComplexityTier};

// ─── Regex Patterns ─────────────────────────────────────────────────────────────

/// Pattern to match file lines in task plans
static FILE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^\s*-?\s*files?\s*:\s*(.+)$").unwrap());

/// Pattern to match "new file" or "create" keywords
static NEW_FILE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(create|new file|scaffold|bootstrap)\b").unwrap());

/// Pattern to match complexity keywords
static COMPLEXITY_KEYWORDS: Lazy<[(Regex, &str); 7]> = Lazy::new(|| {
    [
        (
            Regex::new(r"\b(migration|migrate|schema change)\b").unwrap(),
            "migration",
        ),
        (
            Regex::new(r"\b(architect|design pattern|system design)\b").unwrap(),
            "architecture",
        ),
        (
            Regex::new(r"\b(security|auth|encrypt|credential|vulnerability)\b").unwrap(),
            "security",
        ),
        (
            Regex::new(r"\b(performance|optimize|cache|index)\b").unwrap(),
            "performance",
        ),
        (
            Regex::new(r"\b(concurrent|parallel|race condition|mutex|lock)\b").unwrap(),
            "concurrency",
        ),
        (
            Regex::new(r"\b(backward.?compat|breaking change|deprecat)\b").unwrap(),
            "compatibility",
        ),
        (
            Regex::new(r"(refactor|migration|architect)").unwrap(),
            "refactor",
        ),
    ]
});

/// Pattern to match tag keywords
static TAG_PATTERNS: Lazy<[(Regex, &str); 5]> = Lazy::new(|| {
    [
        (
            Regex::new(r"\b(refactor|migration|architect)\b").unwrap(),
            "refactor",
        ),
        (Regex::new(r"\b(test|spec|coverage)\b").unwrap(), "test"),
        (
            Regex::new(r"\b(doc|readme|comment|jsdoc)\b").unwrap(),
            "docs",
        ),
        (Regex::new(r"\b(config|env|setting)\b").unwrap(), "config"),
        (Regex::new(r"\b(rename|typo|spelling)\b").unwrap(), "rename"),
    ]
});

/// Pattern to match estimated lines
static ESTIMATE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"~?\s*(\d+)\s*lines?\b").unwrap());

// ─── Types ──────────────────────────────────────────────────────────────────────

/// Complexity classification result
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassificationResult {
    pub tier: ComplexityTier,
    pub reason: String,
    pub downgraded: bool,
}

/// Task metadata extracted from plan files
#[derive(Debug, Clone, Default)]
pub struct TaskMetadata {
    pub file_count: Option<usize>,
    pub dependency_count: Option<usize>,
    pub is_new_file: Option<bool>,
    pub tags: Vec<String>,
    pub estimated_lines: Option<usize>,
    pub code_block_count: Option<usize>,
    pub complexity_keywords: Vec<String>,
}

/// Unit type to default tier mapping
static UNIT_TYPE_TIERS: Lazy<HashMap<&'static str, ComplexityTier>> = Lazy::new(|| {
    let mut m = HashMap::new();
    // Tier 1 — Light: structured summaries, completion, UAT
    m.insert("complete-slice", ComplexityTier::Light);
    m.insert("run-uat", ComplexityTier::Light);

    // Tier 2 — Standard: research, routine planning
    m.insert("research-milestone", ComplexityTier::Standard);
    m.insert("research-slice", ComplexityTier::Standard);
    m.insert("plan-milestone", ComplexityTier::Standard);
    m.insert("plan-slice", ComplexityTier::Standard);

    // Tier 3 — Heavy: execution, replanning (requires deep reasoning)
    m.insert("execute-task", ComplexityTier::Standard); // default standard, upgraded by metadata
    m.insert("replan-slice", ComplexityTier::Heavy);
    m.insert("reassess-roadmap", ComplexityTier::Heavy);

    m
});

// ─── Public API ────────────────────────────────────────────────────────────────

/// Classify unit complexity to determine which model tier to use
///
/// # Arguments
/// * `unit_type` - The type of unit being dispatched
/// * `unit_id` - The unit ID (e.g. "M001/S01/T01")
/// * `base_path` - Project base path (for reading task plans)
/// * `budget_pct` - Current budget usage as fraction (0.0-1.0+), or None if no budget
/// * `metadata` - Optional pre-parsed task metadata
///
/// # Returns
/// Classification result with tier, reason, and downgrade flag
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::complexity_classifier::*;
///
/// let result = classify_unit_complexity(
///     "execute-task",
///     "M01/S01/T01",
///     "/project",
///     Some(0.6),
///     None
/// );
/// ```
pub fn classify_unit_complexity(
    unit_type: &str,
    unit_id: &str,
    base_path: &str,
    budget_pct: Option<f64>,
    metadata: Option<TaskMetadata>,
) -> ClassificationResult {
    // Hook units default to light
    if unit_type.starts_with("hook/") {
        let mut result = ClassificationResult {
            tier: ComplexityTier::Light,
            reason: "hook unit".to_string(),
            downgraded: false,
        };
        return apply_budget_pressure(&mut result, budget_pct);
    }

    // Start with the default tier for this unit type
    let mut tier = *UNIT_TYPE_TIERS
        .get(unit_type)
        .unwrap_or(&ComplexityTier::Standard);
    let mut reason = format!("unit type: {}", unit_type);

    // For execute-task, analyze task metadata for complexity signals
    // Clone tags for adaptive learning before metadata is moved
    let tags_for_adaptive = metadata.as_ref().map(|m| m.tags.clone());

    if unit_type == "execute-task" {
        let task_analysis = analyze_task_complexity(unit_id, base_path, metadata);
        tier = task_analysis.tier;
        reason = task_analysis.reason;
    }

    // For plan-slice, check if the slice has many tasks (complex planning)
    if unit_type == "plan-slice" || unit_type == "plan-milestone" {
        if let Some(plan_analysis) = analyze_plan_complexity(unit_id, base_path) {
            tier = plan_analysis.tier;
            reason = plan_analysis.reason;
        }
    }

    // Adaptive learning: check if history suggests bumping the tier
    let tags = tags_for_adaptive.unwrap_or_else(|| extract_task_metadata(unit_id, base_path).tags);

    if let Some(adaptive_adjustment) = get_adaptive_tier_adjustment(unit_type, tier, Some(&tags)) {
        if tier_ordinal(adaptive_adjustment) > tier_ordinal(tier) {
            reason = format!("{} (adaptive: high failure rate at {:?})", reason, tier);
            tier = adaptive_adjustment;
        }
    }

    let mut result = ClassificationResult {
        tier,
        reason,
        downgraded: false,
    };

    apply_budget_pressure(&mut result, budget_pct)
}

/// Get a short label for the tier (for dashboard display)
///
/// # Arguments
/// * `tier` - Complexity tier
///
/// # Returns
/// Single character label (L/S/H)
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::complexity_classifier::*;
/// use rustycode_orchestra::routing_history::ComplexityTier;
///
/// assert_eq!(tier_label(ComplexityTier::Light), "L");
/// ```
pub fn tier_label(tier: ComplexityTier) -> &'static str {
    match tier {
        ComplexityTier::Light => "L",
        ComplexityTier::Standard => "S",
        ComplexityTier::Heavy => "H",
    }
}

/// Get the tier ordering value (for comparison)
///
/// # Arguments
/// * `tier` - Complexity tier
///
/// # Returns
/// Numeric value (0=Light, 1=Standard, 2=Heavy)
pub fn tier_ordinal(tier: ComplexityTier) -> u8 {
    match tier {
        ComplexityTier::Light => 0,
        ComplexityTier::Standard => 1,
        ComplexityTier::Heavy => 2,
    }
}

// ─── Task Complexity Analysis ───────────────────────────────────────────────────

struct TaskAnalysis {
    tier: ComplexityTier,
    reason: String,
}

fn analyze_task_complexity(
    unit_id: &str,
    base_path: &str,
    metadata: Option<TaskMetadata>,
) -> TaskAnalysis {
    // Try to read task plan for complexity signals
    let meta = metadata.unwrap_or_else(|| extract_task_metadata(unit_id, base_path));

    // Heavy signals
    if let Some(dep_count) = meta.dependency_count {
        if dep_count >= 3 {
            return TaskAnalysis {
                tier: ComplexityTier::Heavy,
                reason: format!("{} dependencies", dep_count),
            };
        }
    }

    if let Some(file_count) = meta.file_count {
        if file_count >= 6 {
            return TaskAnalysis {
                tier: ComplexityTier::Heavy,
                reason: format!("{} files to modify", file_count),
            };
        }
    }

    if let Some(estimated_lines) = meta.estimated_lines {
        if estimated_lines >= 500 {
            return TaskAnalysis {
                tier: ComplexityTier::Heavy,
                reason: format!("~{} lines estimated", estimated_lines),
            };
        }
    }

    // Heavy signals from complexity keywords (Phase 4)
    if meta.complexity_keywords.len() >= 2 {
        return TaskAnalysis {
            tier: ComplexityTier::Heavy,
            reason: format!("complex: {}", meta.complexity_keywords.join(", ")),
        };
    }

    if let Some(code_block_count) = meta.code_block_count {
        if code_block_count >= 5 {
            return TaskAnalysis {
                tier: ComplexityTier::Heavy,
                reason: format!("{} code blocks in plan", code_block_count),
            };
        }
    }

    // Standard signals from single complexity keyword
    if meta.complexity_keywords.len() == 1 {
        return TaskAnalysis {
            tier: ComplexityTier::Standard,
            reason: format!("{} task", meta.complexity_keywords[0]),
        };
    }

    // Light signals (simple tasks)
    let light_tags = ["docs", "readme", "comment", "config", "typo", "rename"];
    if meta.tags.iter().any(|t| light_tags.contains(&t.as_str())) {
        return TaskAnalysis {
            tier: ComplexityTier::Light,
            reason: format!("simple task: {}", meta.tags.join(", ")),
        };
    }

    if let Some(file_count) = meta.file_count {
        if file_count <= 1 && meta.is_new_file == Some(false) {
            return TaskAnalysis {
                tier: ComplexityTier::Light,
                reason: "single file modification".to_string(),
            };
        }
    }

    // Standard by default
    TaskAnalysis {
        tier: ComplexityTier::Standard,
        reason: "standard execution task".to_string(),
    }
}

fn analyze_plan_complexity(unit_id: &str, base_path: &str) -> Option<TaskAnalysis> {
    // Check if this is a milestone-level plan (more complex) vs single slice
    let parts: Vec<&str> = unit_id.split('/').collect();
    if parts.len() == 1 {
        // Milestone-level planning is always at least standard
        return Some(TaskAnalysis {
            tier: ComplexityTier::Standard,
            reason: "milestone-level planning".to_string(),
        });
    }

    // For slice planning, try to read the context/research to gauge complexity
    // If research exists and is large, bump to heavy
    let [mid, sid] = [parts[0], parts[1]];
    let research_path = Path::new(base_path)
        .join(orchestra_root(Path::new(base_path)))
        .join(mid)
        .join("slices")
        .join(sid)
        .join("RESEARCH.md");

    if let Ok(content) = fs::read_to_string(&research_path) {
        let line_count = content.lines().count();
        if line_count > 200 {
            return Some(TaskAnalysis {
                tier: ComplexityTier::Heavy,
                reason: format!("complex slice: {}-line research", line_count),
            });
        }
    }

    None // Use default tier
}

/// Extract task metadata from the task plan file on disk
///
/// # Arguments
/// * `unit_id` - The unit ID (e.g. "M01/S01/T01")
/// * `base_path` - Project base path
///
/// # Returns
/// Extracted task metadata
fn extract_task_metadata(unit_id: &str, base_path: &str) -> TaskMetadata {
    let mut meta = TaskMetadata::default();

    let parts: Vec<&str> = unit_id.split('/').collect();
    if parts.len() != 3 {
        return meta;
    }

    let [mid, sid, tid] = [parts[0], parts[1], parts[2]];
    let task_plan_path = Path::new(base_path)
        .join(orchestra_root(Path::new(base_path)))
        .join(mid)
        .join("slices")
        .join(sid)
        .join("tasks")
        .join(format!("{}-PLAN.md", tid));

    let content = match fs::read_to_string(&task_plan_path) {
        Ok(c) => c,
        Err(_) => return meta,
    };

    let lines: Vec<&str> = content.lines().collect();

    // Count files mentioned in "Files:" or "- Files:" lines
    let mut all_files: HashSet<String> = HashSet::new();
    for line in &lines {
        if let Some(caps) = FILE_RE.captures(line) {
            let files_str = &caps[1];
            for file in files_str.split([',', ';']) {
                let file = file.trim();
                if !file.is_empty() {
                    all_files.insert(file.to_string());
                }
            }
        }
    }
    if !all_files.is_empty() {
        meta.file_count = Some(all_files.len());
    }

    // Check for "new file" or "create" keywords
    meta.is_new_file = lines
        .iter()
        .any(|l| NEW_FILE_RE.is_match(l))
        .then_some(true);

    // Look for tags/labels in frontmatter or content
    let mut tags: Vec<String> = Vec::new();
    for line in &lines {
        for (re, tag) in TAG_PATTERNS.iter() {
            if re.is_match(line) && !tags.contains(&tag.to_string()) {
                tags.push(tag.to_string());
            }
        }
    }
    meta.tags = tags;

    // Try to extract estimated lines from content
    for line in &lines {
        if let Some(caps) = ESTIMATE_RE.captures(line) {
            if let Ok(lines) = caps[1].parse::<usize>() {
                meta.estimated_lines = Some(lines);
                break;
            }
        }
    }

    // Phase 4: Deeper introspection signals

    // Count fenced code blocks (```) — more code blocks = more complex implementation
    let code_block_count = content.matches("```").count() / 2;
    if code_block_count > 0 {
        meta.code_block_count = Some(code_block_count);
    }

    // Detect complexity keywords that suggest harder tasks
    let mut complexity_keywords: Vec<String> = Vec::new();
    for (re, keyword) in COMPLEXITY_KEYWORDS.iter() {
        if re.is_match(&content) && !complexity_keywords.contains(&keyword.to_string()) {
            complexity_keywords.push(keyword.to_string());
        }
    }
    meta.complexity_keywords = complexity_keywords;

    meta
}

// ─── Budget Pressure ───────────────────────────────────────────────────────────

/// Apply budget pressure to a classification result
///
/// As budget usage increases, more aggressively downgrade tiers:
/// - <50%:   Normal classification (no change)
/// - 50-75%: Tier 2 → Tier 1 where possible
/// - 75-90%: Only heavy tasks keep configured model
/// - >90%:   Everything except replan-slice gets cheapest model
fn apply_budget_pressure(
    result: &mut ClassificationResult,
    budget_pct: Option<f64>,
) -> ClassificationResult {
    let budget_pct = match budget_pct {
        Some(p) if p >= 0.5 => p,
        _ => return result.clone(),
    };

    let original = result.tier;

    if budget_pct >= 0.9 {
        // >90%: almost everything goes to light
        if result.tier != ComplexityTier::Heavy {
            result.tier = ComplexityTier::Light;
        } else {
            // Even heavy gets downgraded to standard
            result.tier = ComplexityTier::Standard;
        }
    } else if budget_pct >= 0.75 {
        // 75-90%: only heavy stays, everything else goes to light
        if result.tier == ComplexityTier::Standard {
            result.tier = ComplexityTier::Light;
        }
    } else {
        // 50-75%: standard → light
        if result.tier == ComplexityTier::Standard {
            result.tier = ComplexityTier::Light;
        }
    }

    if result.tier != original {
        result.downgraded = true;
        result.reason = format!(
            "{} (budget pressure: {}%)",
            result.reason,
            (budget_pct * 100.0).round()
        );
    }

    result.clone()
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_label() {
        assert_eq!(tier_label(ComplexityTier::Light), "L");
        assert_eq!(tier_label(ComplexityTier::Standard), "S");
        assert_eq!(tier_label(ComplexityTier::Heavy), "H");
    }

    #[test]
    fn test_tier_ordinal() {
        assert_eq!(tier_ordinal(ComplexityTier::Light), 0);
        assert_eq!(tier_ordinal(ComplexityTier::Standard), 1);
        assert_eq!(tier_ordinal(ComplexityTier::Heavy), 2);
    }

    #[test]
    fn test_classify_hook_unit() {
        let result = classify_unit_complexity("hook/pre-commit", "H01", "/project", None, None);
        assert_eq!(result.tier, ComplexityTier::Light);
        assert!(result.reason.contains("hook"));
    }

    #[test]
    fn test_classify_complete_slice() {
        let result = classify_unit_complexity("complete-slice", "M01/S01", "/project", None, None);
        assert_eq!(result.tier, ComplexityTier::Light);
    }

    #[test]
    fn test_classify_execute_task_default() {
        let result =
            classify_unit_complexity("execute-task", "M01/S01/T01", "/nonexistent", None, None);
        // Should be standard by default when no metadata available
        assert_eq!(result.tier, ComplexityTier::Standard);
    }

    #[test]
    fn test_budget_pressure_below_50() {
        let mut result = ClassificationResult {
            tier: ComplexityTier::Standard,
            reason: "test".to_string(),
            downgraded: false,
        };

        let result = apply_budget_pressure(&mut result, Some(0.4));
        assert_eq!(result.tier, ComplexityTier::Standard);
        assert!(!result.downgraded);
    }

    #[test]
    fn test_budget_pressure_50_to_75() {
        let mut result = ClassificationResult {
            tier: ComplexityTier::Standard,
            reason: "test".to_string(),
            downgraded: false,
        };

        let result = apply_budget_pressure(&mut result, Some(0.6));
        assert_eq!(result.tier, ComplexityTier::Light);
        assert!(result.downgraded);
        assert!(result.reason.contains("budget pressure"));
    }

    #[test]
    fn test_budget_pressure_75_to_90() {
        let mut result = ClassificationResult {
            tier: ComplexityTier::Standard,
            reason: "test".to_string(),
            downgraded: false,
        };

        let result = apply_budget_pressure(&mut result, Some(0.8));
        assert_eq!(result.tier, ComplexityTier::Light);
        assert!(result.downgraded);
    }

    #[test]
    fn test_budget_pressure_above_90() {
        let mut result = ClassificationResult {
            tier: ComplexityTier::Heavy,
            reason: "test".to_string(),
            downgraded: false,
        };

        let result = apply_budget_pressure(&mut result, Some(0.95));
        assert_eq!(result.tier, ComplexityTier::Standard);
        assert!(result.downgraded);
    }

    #[test]
    fn test_task_metadata_default() {
        let meta = TaskMetadata::default();
        assert!(meta.file_count.is_none());
        assert!(meta.tags.is_empty());
    }

    #[test]
    fn test_classification_result_clone() {
        let result = ClassificationResult {
            tier: ComplexityTier::Standard,
            reason: "test".to_string(),
            downgraded: false,
        };

        let cloned = result.clone();
        assert_eq!(cloned.tier, result.tier);
        assert_eq!(cloned.reason, result.reason);
    }
}
