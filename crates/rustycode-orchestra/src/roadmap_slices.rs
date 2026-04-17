//! Roadmap Slices Parser
//!
//! Parses roadmap slice definitions from markdown files.
//! Handles both machine-readable checklist format and prose-style headers.
//!
//! Matches orchestra-2's roadmap-slices.ts implementation.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::LazyLock;

use crate::complexity::RiskLevel;
use crate::error::Result;

static RANGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z]+)(\d+)(?:-|\.\.)+([A-Za-z]+)(\d+)$").unwrap());
static SLICES_HEADING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"##\s+Slices\b").unwrap());
static NEXT_HEADING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n##\s+").unwrap());
static CHECKBOX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*-\s+\[([ xX])\]\s+\*([\w.]+):\s*(.*)").unwrap());
static RISK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`risk:(\w+)`").unwrap());
static DEPS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`depends:\[([^\]]*)\]`").unwrap());
static PROSE_HEADER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"##\s+(?:Slice\s+)?(S\d+)[:\s—–\-]+\s*(.+)").unwrap());
static PROSE_DEPS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\*{0,2}Depends\s+on:?\*{0,2}\s*(.+)").unwrap());

// ─── Types ───────────────────────────────────────────────────────────────────

/// A roadmap slice entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadmapSliceEntry {
    /// Slice ID (e.g., "S01")
    pub id: String,
    /// Slice title
    pub title: String,
    /// Risk level
    pub risk: RiskLevel,
    /// Dependencies (slice IDs)
    #[serde(default)]
    pub depends: Vec<String>,
    /// Completion status
    pub done: bool,
    /// Demo text (from blockquotes)
    #[serde(default)]
    pub demo: String,
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Expand dependency shorthand into individual slice IDs.
///
/// Handles two common LLM-generated patterns:
/// - "S01-S04"  → ["S01", "S02", "S03", "S04"]  (range syntax)
/// - "S01..S04" → ["S01", "S02", "S03", "S04"]  (dot-range syntax)
///
/// Plain IDs ("S01", "S02") and empty strings pass through unchanged.
pub fn expand_dependencies(deps: &[String]) -> Vec<String> {
    let mut result = Vec::new();

    for dep in deps {
        let trimmed = dep.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(caps) = RANGE_RE.captures(trimmed) {
            let prefix_a = &caps[1];
            let start_num: i32 = caps[2].parse().unwrap_or(0);
            let prefix_b = &caps[3];
            let end_num: i32 = caps[4].parse().unwrap_or(0);

            // Only expand when both prefixes match and range is valid
            if prefix_a == prefix_b && start_num <= end_num {
                let width = caps[2].len(); // preserve zero-padding
                for i in start_num..=end_num {
                    result.push(format!("{}{:0width$}", prefix_a, i, width = width));
                }
                continue;
            }
        }

        result.push(trimmed.to_string());
    }

    result
}

/// Parse roadmap slices from markdown content.
///
/// Extracts the ## Slices section and parses checkbox-style entries.
/// Falls back to prose-style header parsing if no ## Slices section found.
pub fn parse_roadmap_slices(content: &str) -> Result<Vec<RoadmapSliceEntry>> {
    let slices_section = extract_slices_section(content);

    if slices_section.is_empty() {
        // Fallback: detect prose-style slice headers
        return parse_prose_slice_headers(content);
    }

    parse_checkbox_slices(&slices_section)
}

// ─── Internals ─────────────────────────────────────────────────────────────

/// Extract the ## Slices section from markdown content
fn extract_slices_section(content: &str) -> String {
    let start_index = if let Some(mat) = SLICES_HEADING_RE.find(content) {
        mat.start() + mat.as_str().len()
    } else {
        return String::new();
    };

    let rest = &content[start_index..];
    let rest = rest.trim_start_matches('\n').trim_start_matches('\r');

    if let Some(mat) = NEXT_HEADING_RE.find(rest) {
        rest[..mat.start()].to_string()
    } else {
        rest.trim_end().to_string()
    }
}

/// Parse checkbox-style slice entries from ## Slices section
fn parse_checkbox_slices(slices_section: &str) -> Result<Vec<RoadmapSliceEntry>> {
    let mut slices = Vec::new();
    let mut current_slice: Option<RoadmapSliceEntry> = None;

    for line in slices_section
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
    {
        if let Some(caps) = CHECKBOX_RE.captures(line) {
            if let Some(prev) = current_slice.take() {
                slices.push(prev);
            }

            let done = &caps[1] == "x" || &caps[1] == "X";
            let id = caps[2].to_string();
            let title_with_rest = caps[3].to_string();

            // Extract title (everything before the next ` or end of string)
            let title_end = title_with_rest.find('`').unwrap_or(title_with_rest.len());
            let mut title = title_with_rest[..title_end].trim().to_string();

            // Strip leading asterisk if present
            if title.starts_with('*') {
                title = title[1..].trim().to_string();
            }
            // Strip trailing asterisk if present
            if title.ends_with('*') {
                title.pop();
                title = title.trim().to_string();
            }

            let rest = &title_with_rest[title_end..];

            // Parse risk level
            let risk = if let Some(risk_caps) = RISK_RE.captures(rest) {
                match &risk_caps[1] {
                    "low" => RiskLevel::Low,
                    "high" => RiskLevel::High,
                    _ => RiskLevel::Medium,
                }
            } else {
                RiskLevel::Medium
            };

            // Parse dependencies
            let depends = if let Some(deps_caps) = DEPS_RE.captures(rest) {
                let deps_str = deps_caps[1].trim();
                if !deps_str.is_empty() {
                    expand_dependencies(
                        &deps_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .collect::<Vec<_>>(),
                    )
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            current_slice = Some(RoadmapSliceEntry {
                id,
                title,
                risk,
                depends,
                done,
                demo: String::new(),
            });
        } else if let Some(ref mut slice) = current_slice {
            if line.starts_with('>') {
                slice.demo = line
                    .trim_start_matches('>')
                    .trim()
                    .trim_start_matches("After this:")
                    .trim()
                    .to_string();
            }
        }
    }

    if let Some(prev) = current_slice {
        slices.push(prev);
    }

    Ok(slices)
}

/// Fallback parser for prose-style roadmaps with ## Slice S01: Title headers
fn parse_prose_slice_headers(content: &str) -> Result<Vec<RoadmapSliceEntry>> {
    let mut slices = Vec::new();

    // Match ## Slice S01: Title or ## S01: Title or ## S01 — Title
    let mut seen_ids = HashSet::new();

    for caps in PROSE_HEADER_RE.captures_iter(content) {
        let id = caps[1].to_string();
        let title = caps[2].trim().to_string();

        // Skip duplicates
        if seen_ids.contains(&id) {
            continue;
        }
        seen_ids.insert(id.clone());

        // Try to extract depends from prose
        let match_start = caps.get(0).map(|m| m.start()).unwrap_or(0);
        let match_len = caps.get(0).map(|m| m.as_str().len()).unwrap_or(0);
        let after_header = &content[match_start + match_len..];
        let next_header_pos = after_header
            .find("##")
            .unwrap_or(after_header.len().min(500));

        let section = &after_header[..next_header_pos];

        // Look for "Depends on: S01" or "**Depends on:** S01"
        let depends = if let Some(deps_caps) = PROSE_DEPS_RE.captures(section) {
            let raw_deps_temp = deps_caps[1].replace("none", "").replace("None", "");
            let raw_deps = raw_deps_temp.trim();
            if !raw_deps.is_empty() {
                expand_dependencies(
                    &raw_deps
                        .split([',', ';'])
                        .map(|s| s.trim().to_string())
                        .map(|s| s.chars().filter(|c: &char| c.is_alphanumeric()).collect())
                        .filter(|s: &String| !s.is_empty())
                        .collect::<Vec<_>>(),
                )
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        slices.push(RoadmapSliceEntry {
            id,
            title,
            risk: RiskLevel::Medium,
            depends,
            done: false,
            demo: String::new(),
        });
    }

    Ok(slices)
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_matching() {
        let line = "- [x] *S01:* Setup project";
        println!("Testing line: '{}'", line);

        // Try pattern without \s after colon
        let pattern1 = Regex::new(r"- \[([ xX])\] \*([\w.]+):(.*)").unwrap();
        if let Some(caps) = pattern1.captures(line) {
            println!("Pattern1 matched!");
            println!("  Done: '{}'", &caps[1]);
            println!("  ID: '{}'", &caps[2]);
            println!("  Title (raw): '{}'", &caps[3]);
        } else {
            println!("Pattern1 didn't match");
        }

        // Try pattern with .*? (non-greedy)
        let pattern2 = Regex::new(r"- \[([ xX])\] \*([\w.]+):\s*(.*)").unwrap();
        if let Some(caps) = pattern2.captures(line) {
            println!("Pattern2 matched!");
            println!("  Done: '{}'", &caps[1]);
            println!("  ID: '{}'", &caps[2]);
            println!("  Title: '{}'", &caps[3]);
        } else {
            println!("Pattern2 didn't match");
        }
    }

    #[test]
    fn test_expand_dependencies_plain_ids() {
        let deps = vec!["S01".to_string(), "S02".to_string()];
        let result = expand_dependencies(&deps);
        assert_eq!(result, vec!["S01", "S02"]);
    }

    #[test]
    fn test_expand_dependencies_range_syntax() {
        let deps = vec!["S01-S04".to_string()];
        let result = expand_dependencies(&deps);
        assert_eq!(result, vec!["S01", "S02", "S03", "S04"]);
    }

    #[test]
    fn test_expand_dependencies_dot_range_syntax() {
        let deps = vec!["S01..S04".to_string()];
        let result = expand_dependencies(&deps);
        assert_eq!(result, vec!["S01", "S02", "S03", "S04"]);
    }

    #[test]
    fn test_expand_dependencies_zero_padded() {
        let deps = vec!["S001-S003".to_string()];
        let result = expand_dependencies(&deps);
        assert_eq!(result, vec!["S001", "S002", "S003"]);
    }

    #[test]
    fn test_expand_dependencies_empty() {
        let deps = vec!["".to_string()];
        let result = expand_dependencies(&deps);
        assert!(result.is_empty());
    }

    #[test]
    fn test_expand_dependencies_mixed() {
        let deps = vec!["S01".to_string(), "S02-S04".to_string(), "S05".to_string()];
        let result = expand_dependencies(&deps);
        assert_eq!(result, vec!["S01", "S02", "S03", "S04", "S05"]);
    }

    #[test]
    fn test_expand_dependencies_prefix_mismatch() {
        let deps = vec!["S01-T02".to_string()];
        let result = expand_dependencies(&deps);
        // Prefixes don't match, so return as-is
        assert_eq!(result, vec!["S01-T02"]);
    }

    #[test]
    fn test_expand_dependencies_invalid_range() {
        let deps = vec!["S04-S01".to_string()];
        let result = expand_dependencies(&deps);
        // Start > end, so return as-is
        assert_eq!(result, vec!["S04-S01"]);
    }

    #[test]
    fn test_extract_slices_section_with_slices() {
        let content = r#"
## Overview
Some text here.

## Slices
- [x] *S01:* Setup project
- [ ] *S02:* Build feature
"#;
        let result = extract_slices_section(content);
        assert!(result.contains("- [x] *S01:* Setup project"));
        assert!(result.contains("- [ ] *S02:* Build feature"));
        assert!(!result.contains("## Overview"));
    }

    #[test]
    fn test_extract_slices_section_no_slices() {
        let content = r#"
## Overview
Some text here.

## Details
More text.
"#;
        let result = extract_slices_section(content);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_checkbox_slices_basic() {
        let content = r#"
- [x] *S01:* Setup project
- [ ] *S02:* Build feature `risk:high`
- [ ] *S03:* Deploy
"#;
        let result = parse_checkbox_slices(content).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, "S01");
        assert_eq!(result[0].title, "Setup project");
        assert!(result[0].done);
        assert_eq!(result[1].risk, RiskLevel::High);
        assert_eq!(result[2].id, "S03");
    }

    #[test]
    fn test_parse_checkbox_slices_with_dependencies() {
        let content = r#"
- [ ] *S01:* Setup project `depends:[S00]`
- [ ] *S02:* Build feature `depends:[S01,S03]`
"#;
        let result = parse_checkbox_slices(content).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].depends, vec!["S00"]);
        assert_eq!(result[1].depends, vec!["S01", "S03"]);
    }

    #[test]
    fn test_parse_checkbox_slices_with_demo() {
        let content = r#"
- [ ] *S01:* Setup project
> After this: verify the setup works
"#;
        let result = parse_checkbox_slices(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].demo, "verify the setup works");
    }

    #[test]
    fn test_parse_prose_slice_headers_basic() {
        let content = r#"
## Slice S01: Setup Project

Implement the initial project structure.
"#;
        let result = parse_prose_slice_headers(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "S01");
        assert_eq!(result[0].title, "Setup Project");
        assert!(!result[0].done);
    }

    #[test]
    fn test_parse_prose_slice_headers_with_depends() {
        let content = r#"
## S01: Setup Project

Implement the initial project structure.
Depends on: S00, S05
"#;
        let result = parse_prose_slice_headers(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].depends, vec!["S00", "S05"]);
    }

    #[test]
    fn test_parse_prose_slice_headers_double_dash() {
        let content = r#"
## S01 — Setup Project

Implement the initial project structure.
"#;
        let result = parse_prose_slice_headers(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "S01");
        assert_eq!(result[0].title, "Setup Project");
    }

    #[test]
    fn test_parse_roadmap_slices_full_flow() {
        let content = r#"
## Overview
Project overview

## Slices
- [x] *S01:* Setup project `risk:low`
- [ ] *S02:* Build feature `depends:[S01]` `risk:high`
> After this: test the build

## Details
More details
"#;
        let result = parse_roadmap_slices(content).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "S01");
        assert!(result[0].done);
        assert_eq!(result[0].risk, RiskLevel::Low);
        assert_eq!(result[1].id, "S02");
        assert!(!result[1].done);
        assert_eq!(result[1].depends, vec!["S01"]);
        assert_eq!(result[1].risk, RiskLevel::High);
        assert_eq!(result[1].demo, "test the build");
    }

    #[test]
    fn test_parse_roadmap_slices_prose_fallback() {
        let content = r#"
## Slice S01: Setup Project

Implement the initial project structure.

## Slice S02: Build Feature

Create the main feature implementation.
"#;
        let result = parse_roadmap_slices(content).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "S01");
        assert_eq!(result[0].title, "Setup Project");
        assert_eq!(result[1].id, "S02");
        assert_eq!(result[1].title, "Build Feature");
    }
}
