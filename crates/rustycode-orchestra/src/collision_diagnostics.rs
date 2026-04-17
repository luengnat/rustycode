// rustycode-orchestra/src/collision_diagnostics.rs
//! Collision Diagnostics Module
//!
//! Bridges NamespacedRegistry collision data and NamespacedResolver ambiguous
//! resolution into a classified diagnostic taxonomy. Provides two functions:
//! - analyze_collisions: Scans registry and resolver state to produce classified diagnostics
//! - doctor_report: Formats diagnostics into human-readable output with severity and remediation
//!
//! This module implements R010 (collision reporting) and R011 (doctor advice) for the
//! namespaced component system.

use std::collections::HashMap;

// ============================================================================
// Type Definitions
// ============================================================================

/// Classification of collision type.
/// - canonical-conflict: Two plugins registered the same canonical name (hard error)
/// - shorthand-overlap: Same bare name exists in multiple namespaces (ambiguity)
/// - alias-conflict: Alias shadows a canonical name or bare component name
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CollisionClass {
    CanonicalConflict,
    ShorthandOverlap,
    AliasConflict,
}

/// Severity level for diagnostics.
/// - error: Hard collision that prevents correct resolution
/// - warning: Ambiguity that may cause surprising behavior
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

/// A classified diagnostic with full context for remediation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedDiagnostic {
    /// The collision classification
    pub class: CollisionClass,

    /// Severity level
    pub severity: DiagnosticSeverity,

    /// All canonical names involved in the collision
    pub involved_canonical_names: Vec<String>,

    /// File paths to the conflicting components
    pub file_paths: Vec<String>,

    /// Human-readable remediation advice
    pub remediation: String,

    /// Optional: the bare name causing ambiguity (shorthand-overlap only)
    pub ambiguous_bare_name: Option<String>,

    /// Optional: the alias string (alias-conflict only)
    pub alias: Option<String>,

    /// Optional: the canonical name the alias points to (alias-conflict only)
    pub alias_target: Option<String>,

    /// Optional: type of alias conflict
    pub alias_conflict_type: Option<AliasConflictType>,
}

/// Type of alias conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AliasConflictType {
    ShadowsCanonical,
    ShadowsBareName,
}

/// Collision doctor report with summary statistics and formatted entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionDoctorReport {
    /// Summary counts by class
    pub summary: CollisionDoctorSummary,

    /// Formatted report entries
    pub entries: Vec<String>,
}

/// Summary statistics for collision diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionDoctorSummary {
    /// Total diagnostics
    pub total: usize,

    /// Canonical conflicts (errors)
    pub canonical_conflicts: usize,

    /// Shorthand overlaps (warnings)
    pub shorthand_overlaps: usize,

    /// Alias conflicts (warnings)
    pub alias_conflicts: usize,
}

// ============================================================================
// Public API
// ============================================================================

/// Analyze a registry and resolver to produce classified diagnostics.
///
/// This function:
/// 1. Reads registry diagnostics for canonical conflicts (→ error severity)
/// 2. Groups registry components by bare component.name
/// 3. For groups with 2+ entries, checks resolver to confirm ambiguity
/// 4. Produces warning diagnostics for ambiguous shorthand resolution
///
/// # Arguments
/// * `registry_diagnostics` - Diagnostics from the namespaced registry
/// * `all_components` - All registered components
/// * `resolve_fn` - Function to test if a bare name resolves ambiguously
///
/// # Returns
/// Array of classified diagnostics
pub fn analyze_collisions<F>(
    registry_diagnostics: &[RegistryDiagnostic],
    all_components: &[NamespacedComponent],
    resolve_fn: F,
) -> Vec<ClassifiedDiagnostic>
where
    F: Fn(&str) -> ResolutionResult,
{
    let mut diagnostics = Vec::new();

    // Step 1: Process canonical conflicts from registry diagnostics
    for diag in registry_diagnostics {
        if diag.diagnostic_type == "collision" {
            if let Some(collision) = &diag.collision {
                diagnostics.push(ClassifiedDiagnostic {
                    class: CollisionClass::CanonicalConflict,
                    severity: DiagnosticSeverity::Error,
                    involved_canonical_names: vec![collision.canonical_name.clone()],
                    file_paths: vec![
                        collision.winner_path.clone(),
                        collision.loser_path.clone(),
                    ],
                    remediation: format!(
                        "Canonical name \"{}\" registered multiple times. \
                        The first registration ({}) took precedence over subsequent registration ({}). \
                        Rename one of the conflicting components to resolve.",
                        collision.canonical_name,
                        collision.winner_source.as_deref().unwrap_or("unknown source"),
                        collision.loser_source.as_deref().unwrap_or("unknown source")
                    ),
                    ambiguous_bare_name: None,
                    alias: None,
                    alias_target: None,
                    alias_conflict_type: None,
                });
            }
        }
    }

    // Step 2: Find shorthand overlaps by grouping components by bare name
    let mut by_bare_name: HashMap<String, Vec<&NamespacedComponent>> = HashMap::new();

    for component in all_components {
        let bare_name = &component.name;
        by_bare_name
            .entry(bare_name.clone())
            .or_default()
            .push(component);
    }

    // Step 3: For groups with 2+ entries, check if resolver confirms ambiguity
    for (bare_name, candidates) in by_bare_name {
        if candidates.len() >= 2 {
            // Use resolver to confirm ambiguity
            let result = resolve_fn(&bare_name);

            if result.resolution == "ambiguous" {
                // This is a shorthand overlap
                let canonical_names: Vec<String> = candidates
                    .iter()
                    .map(|c| c.canonical_name.clone())
                    .collect();
                let file_paths: Vec<String> =
                    candidates.iter().map(|c| c.file_path.clone()).collect();

                diagnostics.push(ClassifiedDiagnostic {
                    class: CollisionClass::ShorthandOverlap,
                    severity: DiagnosticSeverity::Warning,
                    involved_canonical_names: canonical_names.clone(),
                    file_paths,
                    remediation: format_shorthand_remediation(&bare_name, &canonical_names),
                    ambiguous_bare_name: Some(bare_name),
                    alias: None,
                    alias_target: None,
                    alias_conflict_type: None,
                });
            }
            // If resolution is 'shorthand' or 'local-first', the overlap is resolved
            // unambiguously by the resolver, so we don't warn
        }
    }

    // Step 4: Check for alias conflicts (if we had alias data)
    // This would require passing alias data as a parameter

    diagnostics
}

/// Format diagnostics into a human-readable doctor report.
///
/// Each diagnostic is formatted with:
/// - Severity icon (❌ error / ⚠️ warning)
/// - Description of the issue
/// - Involved file paths
/// - Remediation advice
///
/// # Arguments
/// * `diagnostics` - Array of classified diagnostics
///
/// # Returns
/// Collision doctor report with summary and formatted entries
pub fn doctor_report(diagnostics: &[ClassifiedDiagnostic]) -> CollisionDoctorReport {
    let summary = CollisionDoctorSummary {
        total: diagnostics.len(),
        canonical_conflicts: diagnostics
            .iter()
            .filter(|d| d.class == CollisionClass::CanonicalConflict)
            .count(),
        shorthand_overlaps: diagnostics
            .iter()
            .filter(|d| d.class == CollisionClass::ShorthandOverlap)
            .count(),
        alias_conflicts: diagnostics
            .iter()
            .filter(|d| d.class == CollisionClass::AliasConflict)
            .count(),
    };

    let entries = diagnostics.iter().map(format_diagnostic_entry).collect();

    CollisionDoctorReport { summary, entries }
}

// ============================================================================
// Private Helpers
// ============================================================================

/// Format remediation advice for shorthand overlap.
fn format_shorthand_remediation(bare_name: &str, canonical_names: &[String]) -> String {
    let suggestions = canonical_names
        .iter()
        .map(|cn| format!("`{}`", cn))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "Bare name \"{}\" is ambiguous across {} namespaces. \
        Use a canonical name ({}) to avoid ambiguity.",
        bare_name,
        canonical_names.len(),
        suggestions
    )
}

/// Format a single diagnostic entry for display.
fn format_diagnostic_entry(diagnostic: &ClassifiedDiagnostic) -> String {
    let icon = if diagnostic.severity == DiagnosticSeverity::Error {
        "❌"
    } else {
        "⚠️"
    };
    let mut lines = Vec::new();

    // Header with severity and class
    let class_str = match diagnostic.class {
        CollisionClass::CanonicalConflict => "CANONICAL-CONFLICT",
        CollisionClass::ShorthandOverlap => "SHORTHAND-OVERLAP",
        CollisionClass::AliasConflict => "ALIAS-CONFLICT",
    };
    lines.push(format!("{} {}", icon, class_str));

    // Description
    match &diagnostic.class {
        CollisionClass::CanonicalConflict => {
            if let Some(name) = diagnostic.involved_canonical_names.first() {
                lines.push(format!("   Canonical name conflict: {}", name));
            }
        }
        CollisionClass::AliasConflict => {
            if let Some(conflict_type) = &diagnostic.alias_conflict_type {
                match conflict_type {
                    AliasConflictType::ShadowsCanonical => {
                        lines.push(format!(
                            "   Alias \"{}\" shadows canonical name (points to {})",
                            diagnostic.alias.as_deref().unwrap_or(""),
                            diagnostic.alias_target.as_deref().unwrap_or("")
                        ));
                    }
                    AliasConflictType::ShadowsBareName => {
                        lines.push(format!(
                            "   Alias \"{}\" shadows bare name (points to {})",
                            diagnostic.alias.as_deref().unwrap_or(""),
                            diagnostic.alias_target.as_deref().unwrap_or("")
                        ));
                    }
                }
            }
        }
        CollisionClass::ShorthandOverlap => {
            lines.push(format!(
                "   Shorthand overlap: \"{}\" matches {} components",
                diagnostic.ambiguous_bare_name.as_deref().unwrap_or(""),
                diagnostic.involved_canonical_names.len()
            ));
        }
    }

    // File paths
    lines.push("   Files:".to_string());
    for path in &diagnostic.file_paths {
        lines.push(format!("     - {}", path));
    }

    // Remediation
    lines.push(format!("   Remediation: {}", diagnostic.remediation));

    lines.join("\n")
}

// ============================================================================
// Input Types (simplified for portability)
// ============================================================================

/// Registry diagnostic entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryDiagnostic {
    /// Diagnostic type
    pub diagnostic_type: String,

    /// Human-readable message
    pub message: String,

    /// Collision details (if type is "collision")
    pub collision: Option<RegistryCollision>,
}

/// Collision information for registry diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryCollision {
    /// The canonical name that collided
    pub canonical_name: String,

    /// Path to the component that won (first registered)
    pub winner_path: String,

    /// Path to the component that lost (subsequent duplicate)
    pub loser_path: String,

    /// Source of the winning component
    pub winner_source: Option<String>,

    /// Source of the losing component
    pub loser_source: Option<String>,
}

/// A component entry in the namespaced registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespacedComponent {
    /// The component's local name (e.g., "code-review")
    pub name: String,

    /// The computed canonical identifier
    pub canonical_name: String,

    /// Absolute path to the component's definition file
    pub file_path: String,

    /// Source identifier (e.g., "plugin:my-plugin", "user", "project")
    pub source: String,
}

/// Result of resolving a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionResult {
    /// Resolution type: "ambiguous", "shorthand", "local-first", etc.
    pub resolution: String,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_shorthand_remediation() {
        let bare_name = "review";
        let canonical_names = vec!["plugin1:review".to_string(), "plugin2:review".to_string()];

        let remediation = format_shorthand_remediation(bare_name, &canonical_names);

        assert!(remediation.contains("Bare name \"review\" is ambiguous"));
        assert!(remediation.contains("2 namespaces"));
        assert!(remediation.contains("`plugin1:review`"));
        assert!(remediation.contains("`plugin2:review`"));
    }

    #[test]
    fn test_format_diagnostic_entry_canonical_conflict() {
        let diagnostic = ClassifiedDiagnostic {
            class: CollisionClass::CanonicalConflict,
            severity: DiagnosticSeverity::Error,
            involved_canonical_names: vec!["my-plugin:code-review".to_string()],
            file_paths: vec![
                "/path/to/first/skill.md".to_string(),
                "/path/to/second/skill.md".to_string(),
            ],
            remediation: "Canonical name conflict detected".to_string(),
            ambiguous_bare_name: None,
            alias: None,
            alias_target: None,
            alias_conflict_type: None,
        };

        let entry = format_diagnostic_entry(&diagnostic);

        assert!(entry.contains("❌"));
        assert!(entry.contains("CANONICAL-CONFLICT"));
        assert!(entry.contains("Canonical name conflict: my-plugin:code-review"));
        assert!(entry.contains("/path/to/first/skill.md"));
        assert!(entry.contains("/path/to/second/skill.md"));
        assert!(entry.contains("Remediation:"));
    }

    #[test]
    fn test_format_diagnostic_entry_shorthand_overlap() {
        let diagnostic = ClassifiedDiagnostic {
            class: CollisionClass::ShorthandOverlap,
            severity: DiagnosticSeverity::Warning,
            involved_canonical_names: vec![
                "plugin1:review".to_string(),
                "plugin2:review".to_string(),
            ],
            file_paths: vec![
                "/path/to/plugin1/review.md".to_string(),
                "/path/to/plugin2/review.md".to_string(),
            ],
            remediation: "Use canonical names to avoid ambiguity".to_string(),
            ambiguous_bare_name: Some("review".to_string()),
            alias: None,
            alias_target: None,
            alias_conflict_type: None,
        };

        let entry = format_diagnostic_entry(&diagnostic);

        assert!(entry.contains("⚠️"));
        assert!(entry.contains("SHORTHAND-OVERLAP"));
        assert!(entry.contains("Shorthand overlap: \"review\""));
        assert!(entry.contains("matches 2 components"));
        assert!(entry.contains("/path/to/plugin1/review.md"));
        assert!(entry.contains("/path/to/plugin2/review.md"));
    }

    #[test]
    fn test_format_diagnostic_entry_alias_conflict() {
        let diagnostic = ClassifiedDiagnostic {
            class: CollisionClass::AliasConflict,
            severity: DiagnosticSeverity::Warning,
            involved_canonical_names: vec![
                "code-review".to_string(),
                "python-tools:code-reviewer".to_string(),
            ],
            file_paths: vec![
                "/path/to/canonical.md".to_string(),
                "/path/to/target.md".to_string(),
            ],
            remediation: "Alias shadows canonical name".to_string(),
            ambiguous_bare_name: None,
            alias: Some("cr".to_string()),
            alias_target: Some("python-tools:code-reviewer".to_string()),
            alias_conflict_type: Some(AliasConflictType::ShadowsCanonical),
        };

        let entry = format_diagnostic_entry(&diagnostic);

        assert!(entry.contains("⚠️"));
        assert!(entry.contains("ALIAS-CONFLICT"));
        assert!(entry.contains("Alias \"cr\" shadows canonical name"));
        assert!(entry.contains("points to python-tools:code-reviewer"));
    }

    #[test]
    fn test_doctor_report_summary() {
        let diagnostics = vec![
            ClassifiedDiagnostic {
                class: CollisionClass::CanonicalConflict,
                severity: DiagnosticSeverity::Error,
                involved_canonical_names: vec!["test:name".to_string()],
                file_paths: vec!["/path1".to_string(), "/path2".to_string()],
                remediation: "Fix it".to_string(),
                ambiguous_bare_name: None,
                alias: None,
                alias_target: None,
                alias_conflict_type: None,
            },
            ClassifiedDiagnostic {
                class: CollisionClass::ShorthandOverlap,
                severity: DiagnosticSeverity::Warning,
                involved_canonical_names: vec!["a:name".to_string(), "b:name".to_string()],
                file_paths: vec!["/path3".to_string(), "/path4".to_string()],
                remediation: "Be explicit".to_string(),
                ambiguous_bare_name: Some("name".to_string()),
                alias: None,
                alias_target: None,
                alias_conflict_type: None,
            },
        ];

        let report = doctor_report(&diagnostics);

        assert_eq!(report.summary.total, 2);
        assert_eq!(report.summary.canonical_conflicts, 1);
        assert_eq!(report.summary.shorthand_overlaps, 1);
        assert_eq!(report.summary.alias_conflicts, 0);
        assert_eq!(report.entries.len(), 2);
    }

    #[test]
    fn test_analyze_collisions_canonical_conflict() {
        let registry_diagnostics = vec![RegistryDiagnostic {
            diagnostic_type: "collision".to_string(),
            message: "Collision detected".to_string(),
            collision: Some(RegistryCollision {
                canonical_name: "my-plugin:skill".to_string(),
                winner_path: "/first/skill.md".to_string(),
                loser_path: "/second/skill.md".to_string(),
                winner_source: Some("plugin:my-plugin".to_string()),
                loser_source: Some("plugin:other-plugin".to_string()),
            }),
        }];

        let components = vec![];
        let resolve_fn = |_name: &str| ResolutionResult {
            resolution: "unambiguous".to_string(),
        };

        let diagnostics = analyze_collisions(&registry_diagnostics, &components, resolve_fn);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].class, CollisionClass::CanonicalConflict);
        assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
        assert_eq!(
            diagnostics[0].involved_canonical_names,
            vec!["my-plugin:skill".to_string()]
        );
    }

    #[test]
    fn test_analyze_collisions_shorthand_overlap() {
        let registry_diagnostics = vec![];

        let components = vec![
            NamespacedComponent {
                name: "review".to_string(),
                canonical_name: "plugin1:review".to_string(),
                file_path: "/plugin1/review.md".to_string(),
                source: "plugin:plugin1".to_string(),
            },
            NamespacedComponent {
                name: "review".to_string(),
                canonical_name: "plugin2:review".to_string(),
                file_path: "/plugin2/review.md".to_string(),
                source: "plugin:plugin2".to_string(),
            },
        ];

        let resolve_fn = |_name: &str| ResolutionResult {
            resolution: "ambiguous".to_string(),
        };

        let diagnostics = analyze_collisions(&registry_diagnostics, &components, resolve_fn);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].class, CollisionClass::ShorthandOverlap);
        assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Warning);
        assert_eq!(
            diagnostics[0].ambiguous_bare_name,
            Some("review".to_string())
        );
    }

    #[test]
    fn test_analyze_collisions_no_overlap_when_resolved() {
        let registry_diagnostics = vec![];

        let components = vec![
            NamespacedComponent {
                name: "review".to_string(),
                canonical_name: "plugin1:review".to_string(),
                file_path: "/plugin1/review.md".to_string(),
                source: "plugin:plugin1".to_string(),
            },
            NamespacedComponent {
                name: "review".to_string(),
                canonical_name: "plugin2:review".to_string(),
                file_path: "/plugin2/review.md".to_string(),
                source: "plugin:plugin2".to_string(),
            },
        ];

        // Resolver says it's unambiguous (e.g., local-first resolution)
        let resolve_fn = |_name: &str| ResolutionResult {
            resolution: "shorthand".to_string(),
        };

        let diagnostics = analyze_collisions(&registry_diagnostics, &components, resolve_fn);

        // Should not warn if resolver can handle it unambiguously
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_collision_class_display() {
        // Test that all collision classes can be formatted
        let classes = vec![
            CollisionClass::CanonicalConflict,
            CollisionClass::ShorthandOverlap,
            CollisionClass::AliasConflict,
        ];

        for class in classes {
            let diagnostic = ClassifiedDiagnostic {
                class: class.clone(),
                severity: DiagnosticSeverity::Error,
                involved_canonical_names: vec![],
                file_paths: vec![],
                remediation: "Test".to_string(),
                ambiguous_bare_name: None,
                alias: None,
                alias_target: None,
                alias_conflict_type: None,
            };

            let entry = format_diagnostic_entry(&diagnostic);
            assert!(!entry.is_empty());
        }
    }
}
