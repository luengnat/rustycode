//! Orchestra Auto Prompts — Dispatch Prompt Builders
//!
//! Constructs prompts for each autonomous unit type:
//! * File inlining with path headers
//! * Smart file inlining with semantic chunking
//! * Dependency summary aggregation
//! * Budget-aware truncation
//! * Orchestra root file inlining
//!
//! Critical for autonomous development with rich context.

use crate::paths::OrchestraRootFile;
use std::path::Path;

// ─── File Inlining ─────────────────────────────────────────────────────────────

/// Load and format a file for inlining into a prompt
///
/// # Arguments
/// * `abs_path` - Absolute path to file
/// * `rel_path` - Relative display path
/// * `label` - Section label
///
/// # Returns
/// Formatted content with header
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_prompts::*;
///
/// let content = inline_file(
///     Some("/project/README.md"),
///     "README.md",
///     "Documentation"
/// );
/// assert!(content.contains("### Documentation"));
/// ```
pub fn inline_file(abs_path: Option<&str>, rel_path: &str, label: &str) -> String {
    let content = if let Some(path) = abs_path {
        std::fs::read_to_string(path).ok()
    } else {
        None
    };

    if let Some(text) = content {
        format!("### {}\nSource: `{}`\n\n{}", label, rel_path, text.trim())
    } else {
        format!(
            "### {}\nSource: `{}`\n\n_(not found — file does not exist yet)_",
            label, rel_path
        )
    }
}

/// Load a file for optional inlining (omits entirely if absent)
///
/// # Arguments
/// * `abs_path` - Absolute path to file
/// * `rel_path` - Relative display path
/// * `label` - Section label
///
/// # Returns
/// Formatted content if file exists, None otherwise
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_prompts::*;
///
/// let content = inline_file_optional(
///     Some("/project/CONFIG.md"),
///     "CONFIG.md",
///     "Configuration"
/// );
/// // Returns None if file doesn't exist
/// ```
pub fn inline_file_optional(abs_path: Option<&str>, rel_path: &str, label: &str) -> Option<String> {
    let content = if let Some(path) = abs_path {
        std::fs::read_to_string(path).ok()
    } else {
        None
    };

    content.map(|text| format!("### {}\nSource: `{}`\n\n{}", label, rel_path, text.trim()))
}

/// Smart file inlining with semantic chunking for large files
///
/// # Arguments
/// * `abs_path` - Absolute path to file
/// * `rel_path` - Relative display path
/// * `label` - Section label
/// * `query` - Task description for relevance scoring (optional)
/// * `threshold` - Character threshold for chunking (default 3000)
///
/// # Returns
/// Formatted content with optional chunking
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_prompts::*;
///
/// let content = inline_file_smart(
///     Some("/project/large_file.rs"),
///     "large_file.rs",
///     "Implementation",
///     Some("authentication logic")
/// );
/// ```
pub fn inline_file_smart(
    abs_path: Option<&str>,
    rel_path: &str,
    label: &str,
    query: Option<&str>,
    threshold: usize,
) -> String {
    use crate::semantic_chunker::*;

    let content = if let Some(path) = abs_path {
        std::fs::read_to_string(path).ok()
    } else {
        None
    };

    let text = match content {
        Some(t) => t,
        None => {
            return format!(
                "### {}\nSource: `{}`\n\n_(not found — file does not exist yet)_",
                label, rel_path
            )
        }
    };

    // For small files or no query, include full content
    if text.len() <= threshold || query.is_none() {
        return format!("### {}\nSource: `{}`\n\n{}", label, rel_path, text.trim());
    }

    // Use semantic chunking for large files
    let result = chunk_by_relevance(&text, query.unwrap_or(""), None);

    // If chunking didn't save much (< 20%), just include full content
    if result.savings_percent < 20 {
        return format!("### {}\nSource: `{}`\n\n{}", label, rel_path, text.trim());
    }

    let formatted = format_chunks(&result, rel_path);
    format!(
        "### {} ({} sections omitted for relevance)\nSource: `{}`\n\n{}",
        label, result.omitted_chunks, rel_path, formatted
    )
}

// ─── Dependency Summary Aggregation ───────────────────────────────────────────

/// Simple dependency summary entry
#[derive(Debug, Clone)]
pub struct DependencySummary {
    pub slice_id: String,
    pub summary_file: String,
    pub rel_path: String,
    pub content: Option<String>,
}

/// Load dependency slice summaries with budget constraint
///
/// # Arguments
/// * `base_path` - Project base path
/// * `milestone_id` - Current milestone ID
/// * `slice_id` - Current slice ID
/// * `dependencies` - List of dependent slice IDs
/// * `budget_chars` - Optional budget constraint
///
/// # Returns
/// Formatted dependency summaries
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_prompts::*;
///
/// let deps = inline_dependency_summaries(
///     "/project",
///     "M01",
///     "S02",
///     &["S01"],
///     Some(5000)
/// );
/// ```
pub fn inline_dependency_summaries(
    base_path: &str,
    milestone_id: &str,
    _slice_id: &str,
    dependencies: &[String],
    budget_chars: Option<usize>,
) -> String {
    use crate::paths::*;
    use crate::summary_distiller::*;

    if dependencies.is_empty() {
        return "- (no dependencies)".to_string();
    }

    let mut sections = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for dep_id in dependencies {
        if seen.contains(dep_id) {
            continue;
        }
        seen.insert(dep_id);

        let summary_path =
            resolve_slice_file(Path::new(base_path), milestone_id, dep_id, "SUMMARY");

        let rel_path_buf = rel_slice_file(Path::new(base_path), milestone_id, dep_id, "SUMMARY");
        let rel_path = rel_path_buf.to_string_lossy();

        let content = summary_path
            .and_then(|p| std::fs::read_to_string(&p).ok())
            .map(|s| s.trim().to_string());

        if let Some(text) = content {
            sections.push(format!(
                "#### {} Summary\nSource: `{}`\n\n{}",
                dep_id, rel_path, text
            ));
        } else {
            sections.push(format!("- `{}` _(not found)_", rel_path));
        }
    }

    let result = sections.join("\n\n");

    // Apply budget constraint if specified
    if let Some(budget) = budget_chars {
        if result.len() > budget {
            if sections.len() >= 3 {
                // Try distillation for 3+ summaries
                let raw_summaries: Vec<String> = sections
                    .iter()
                    .map(|s| {
                        // Extract content after "Source:" line
                        if let Some(idx) = s.find("Source:") {
                            let after_source = &s[idx..];
                            if let Some(nl_idx) = after_source.find('\n') {
                                after_source[nl_idx + 1..].trim().to_string()
                            } else {
                                s.trim().to_string()
                            }
                        } else {
                            s.trim().to_string()
                        }
                    })
                    .collect();

                let distilled = distill_summaries(&raw_summaries, budget);

                if distilled.content.len() <= budget {
                    return distilled.content;
                }
            }

            // Fall back to simple truncation
            let truncate_at = budget.saturating_sub(30); // Leave room for truncation marker
            format!(
                "{}\n\n...[{} chars truncated]",
                &result[..truncate_at.min(result.len())],
                result.len().saturating_sub(truncate_at)
            )
        } else {
            result
        }
    } else {
        result
    }
}

/// Load a well-known .orchestra/ root file for optional inlining
///
/// # Arguments
/// * `base_path` - Project base path
/// * `file_key` - Which root file to load
/// * `label` - Section label
///
/// # Returns
/// Formatted content if file exists, None otherwise
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_prompts::*;
///
/// let content = inline_orchestra_root_file(
///     "/project",
///     OrchestraRootFile::Decisions,
///     "Project Decisions"
/// );
/// ```
pub fn inline_orchestra_root_file(
    base_path: &str,
    file_key: OrchestraRootFile,
    label: &str,
) -> Option<String> {
    use crate::paths::*;

    let abs_path = resolve_orchestra_root_file(Path::new(base_path), file_key);

    // Check if file exists
    if !abs_path.exists() {
        return None;
    }

    let rel_path = rel_orchestra_root_file(file_key);
    let rel_path_str = rel_path.to_string_lossy();

    inline_file_optional(abs_path.to_str(), &rel_path_str, label)
}

// ─── Prompt Section Builders ───────────────────────────────────────────────────

/// Build the context section for a prompt
///
/// # Arguments
/// * `files` - Vector of (abs_path, rel_path, label) tuples
///
/// # Returns
/// Formatted context section
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_prompts::*;
///
/// let context = build_context_section(&vec![
///     (Some("/project/README.md"), "README.md", "Readme"),
/// ]);
/// assert!(context.contains("## Context"));
/// ```
pub fn build_context_section(files: &[(Option<&str>, &str, &str)]) -> String {
    let mut sections = Vec::new();
    sections.push("## Context\n".to_string());

    for (abs_path, rel_path, label) in files {
        sections.push(inline_file(*abs_path, rel_path, label));
    }

    sections.join("\n\n")
}

/// Build the dependencies section for a prompt
///
/// # Arguments
/// * `base_path` - Project base path
/// * `milestone_id` - Current milestone ID
/// * `slice_id` - Current slice ID
/// * `dependencies` - List of dependent slice IDs
/// * `budget_chars` - Optional budget constraint
///
/// # Returns
/// Formatted dependencies section
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_prompts::*;
///
/// let deps = build_dependencies_section(
///     "/project",
///     "M01",
///     "S02",
///     &["S01".to_string()],
///     None
/// );
/// assert!(deps.contains("## Dependencies"));
/// ```
pub fn build_dependencies_section(
    base_path: &str,
    milestone_id: &str,
    slice_id: &str,
    dependencies: &[String],
    budget_chars: Option<usize>,
) -> String {
    let summaries = inline_dependency_summaries(
        base_path,
        milestone_id,
        slice_id,
        dependencies,
        budget_chars,
    );

    format!(
        "## Dependencies\n\n\
         Dependencies: {} needs to complete first:\n\n\
         {}\n",
        slice_id, summaries
    )
}

/// Build the instructions section for a task
///
/// # Arguments
/// * `task_description` - Task description
/// * `must_haves` - Required deliverables
///
/// # Returns
/// Formatted instructions section
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_prompts::*;
///
/// let instructions = build_task_instructions(
///     "Implement authentication",
///     &["Login form".to_string(), "JWT tokens".to_string()]
/// );
/// assert!(instructions.contains("## Task"));
/// ```
pub fn build_task_instructions(task_description: &str, must_haves: &[String]) -> String {
    let mut sections = Vec::new();
    sections.push("## Task\n".to_string());
    sections.push(format!("{}\n", task_description));

    if !must_haves.is_empty() {
        sections.push("\n### Must-Haves\n".to_string());
        for (i, item) in must_haves.iter().enumerate() {
            sections.push(format!("{}. {}\n", i + 1, item));
        }
    }

    sections.join("")
}

/// Build complete dispatch prompt for a task
///
/// # Arguments
/// * `context_files` - Context files to inline
/// * `dependencies` - Optional dependencies section
/// * `task_description` - Task description
/// * `must_haves` - Required deliverables
/// * `orchestra_files` - Optional Orchestra root files to include
///
/// # Returns
/// Complete dispatch prompt
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::auto_prompts::*;
///
/// let prompt = build_task_prompt(
///     &[(Some("/project/README.md"), "README.md", "Readme")],
///     None,
///     "Add authentication",
///     &["Login".to_string()],
///     None
/// );
/// assert!(prompt.contains("## Task"));
/// ```
pub fn build_task_prompt(
    context_files: &[(Option<&str>, &str, &str)],
    dependencies: Option<&str>,
    task_description: &str,
    must_haves: &[String],
    orchestra_files: Option<&[(OrchestraRootFile, &str)]>,
) -> String {
    let mut sections = Vec::new();

    // Context
    sections.push(build_context_section(context_files));

    // Dependencies (if provided)
    if let Some(deps) = dependencies {
        sections.push(deps.to_string());
    }

    // Orchestra Root Files (if provided)
    if let Some(files) = orchestra_files {
        let mut orchestra_sections = Vec::new();
        orchestra_sections.push("## Project Knowledge\n".to_string());

        for (file_key, label) in files {
            if let Some(content) = inline_orchestra_root_file("/project", *file_key, label) {
                orchestra_sections.push(content);
            }
        }

        if orchestra_sections.len() > 1 {
            sections.push(orchestra_sections.join("\n\n"));
        }
    }

    // Task
    sections.push(build_task_instructions(task_description, must_haves));

    sections.join("\n\n")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_file_with_content() {
        // This test requires a file to exist, so we'll test the fallback
        let content = inline_file(None, "test.md", "Test");
        assert!(content.contains("not found"));
    }

    #[test]
    fn test_inline_file_optional_none() {
        let content = inline_file_optional(None, "test.md", "Test");
        assert!(content.is_none());
    }

    #[test]
    fn test_inline_file_smart_no_query() {
        let content = inline_file_smart(None, "test.md", "Test", None, 3000);
        assert!(content.contains("### Test"));
        assert!(content.contains("not found"));
    }

    #[test]
    fn test_inline_dependency_summaries_empty() {
        let deps = inline_dependency_summaries("/project", "M01", "S01", &[], None);
        assert!(deps.contains("no dependencies"));
    }

    #[test]
    fn test_inline_dependency_summaries_with_deps() {
        // Test with dependencies (files likely don't exist)
        let deps =
            inline_dependency_summaries("/project", "M01", "S01", &[String::from("S02")], None);
        assert!(deps.contains("S02"));
    }

    #[test]
    fn test_inline_orchestra_root_file_nonexistent() {
        let content = inline_orchestra_root_file(
            "/nonexistent",
            OrchestraRootFile::Project,
            "Project Overview",
        );
        assert!(content.is_none());
    }

    #[test]
    fn test_build_context_section() {
        let context = build_context_section(&[(None, "test.md", "Test File")]);
        assert!(context.contains("## Context"));
        assert!(context.contains("### Test File"));
    }

    #[test]
    fn test_build_dependencies_section() {
        let deps =
            build_dependencies_section("/project", "M01", "S01", &[String::from("S02")], None);
        assert!(deps.contains("## Dependencies"));
        assert!(deps.contains("S01 needs to complete"));
    }

    #[test]
    fn test_build_task_instructions() {
        let instructions = build_task_instructions(
            "Implement feature",
            &["Requirement 1".to_string(), "Requirement 2".to_string()],
        );
        assert!(instructions.contains("## Task"));
        assert!(instructions.contains("Implement feature"));
        assert!(instructions.contains("1. Requirement 1"));
    }

    #[test]
    fn test_build_task_instructions_no_must_haves() {
        let instructions = build_task_instructions("Implement feature", &[]);
        assert!(instructions.contains("## Task"));
        assert!(instructions.contains("Implement feature"));
        assert!(!instructions.contains("Must-Haves"));
    }

    #[test]
    fn test_build_task_prompt() {
        let prompt = build_task_prompt(
            &[(None, "test.md", "Test")],
            None,
            "Do work",
            &["Item 1".to_string()],
            None,
        );
        assert!(prompt.contains("## Context"));
        assert!(prompt.contains("## Task"));
        assert!(prompt.contains("Do work"));
    }

    #[test]
    fn test_orchestra_root_file_display() {
        // Test that Orchestra root file enum is displayed correctly
        let _file = OrchestraRootFile::Decisions;
        let _label = "Decisions";
        // Just ensure it compiles
    }

    #[test]
    fn test_dependency_summary_structure() {
        let summary = DependencySummary {
            slice_id: "S01".to_string(),
            summary_file: "/path/S01-SUMMARY.md".to_string(),
            rel_path: ".orchestra/milestones/M01/S01-SUMMARY.md".to_string(),
            content: Some("Summary content".to_string()),
        };
        assert_eq!(summary.slice_id, "S01");
    }
}
