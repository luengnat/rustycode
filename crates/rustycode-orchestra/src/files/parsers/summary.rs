//! Summary file parser
//!
//! Parses SUMMARY.md files containing:
//! - Frontmatter with metadata
//! - What happened and deviations
//! - Files created/modified

use crate::files::parsers::common::{extract_section, parse_frontmatter_map, split_frontmatter};
use regex::Regex;

/// Summary structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    pub frontmatter: SummaryFrontmatter,
    pub title: String,
    pub one_liner: String,
    pub what_happened: String,
    pub deviations: String,
    pub files_modified: Vec<FileModified>,
}

/// Summary frontmatter
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SummaryFrontmatter {
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub parent: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub milestone: String,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub requires: Vec<RequiresEntry>,
    #[serde(default)]
    pub affects: Vec<String>,
    #[serde(default)]
    pub key_files: Vec<String>,
    #[serde(default)]
    pub key_decisions: Vec<String>,
    #[serde(default)]
    pub patterns_established: Vec<String>,
    #[serde(default)]
    pub drill_down_paths: Vec<String>,
    #[serde(default)]
    pub observability_surfaces: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub duration: String,
    #[serde(default = "default_verification_result")]
    pub verification_result: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub completed_at: String,
    #[serde(default)]
    pub blocker_discovered: bool,
}

fn default_verification_result() -> String {
    "untested".to_string()
}

/// Requires entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequiresEntry {
    pub slice: String,
    pub provides: String,
}

/// File modified entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileModified {
    pub path: String,
    pub description: String,
}

/// Parse summary
pub fn parse_summary(content: &str) -> Summary {
    let (fm_lines, body) = split_frontmatter(content);

    // Try to parse as structured YAML first, fall back to simple map
    let fm_value: serde_yaml::Value = if let Some(fm) = fm_lines {
        serde_yaml::from_str(&fm).unwrap_or_else(|_| {
            // Fall back to simple parser if structured parsing fails
            let map = parse_frontmatter_map(&fm);
            serde_yaml::Value::Mapping(
                map.into_iter()
                    .map(|(k, v)| (serde_yaml::Value::String(k), v))
                    .collect(),
            )
        })
    } else {
        serde_yaml::Value::Mapping(serde_yaml::mapping::Mapping::new())
    };

    let frontmatter = parse_summary_frontmatter(&fm_value);

    let body_lines: Vec<&str> = body.lines().collect();
    let h1 = body_lines.iter().find(|l| l.starts_with("# "));
    let title = h1.map(|s| &s[2..]).unwrap_or("").trim().to_string();

    let one_liner = extract_one_liner(&body_lines, h1);

    let what_happened = extract_section(&body, "What Happened", 2).unwrap_or_default();
    let deviations = extract_section(&body, "Deviations", 2).unwrap_or_default();

    let files_modified = parse_files_modified(&body);

    Summary {
        frontmatter,
        title,
        one_liner,
        what_happened,
        deviations,
        files_modified,
    }
}

fn parse_summary_frontmatter(fm: &serde_yaml::Value) -> SummaryFrontmatter {
    let get_str = |key: &str| -> String {
        fm.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let get_str_array = |key: &str| -> Vec<String> {
        fm.get(key)
            .and_then(|v| v.as_sequence())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    };

    let get_requires = |key: &str| -> Vec<RequiresEntry> {
        fm.get(key)
            .and_then(|v| v.as_sequence())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        if let Some(obj) = v.as_mapping() {
                            let slice = obj
                                .get(serde_yaml::Value::String("slice".to_string()))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let provides = obj
                                .get(serde_yaml::Value::String("provides".to_string()))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            Some(RequiresEntry { slice, provides })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    SummaryFrontmatter {
        id: get_str("id"),
        parent: get_str("parent"),
        milestone: get_str("milestone"),
        provides: get_str_array("provides"),
        requires: get_requires("requires"),
        affects: get_str_array("affects"),
        key_files: get_str_array("key_files"),
        key_decisions: get_str_array("key_decisions"),
        patterns_established: get_str_array("patterns_established"),
        drill_down_paths: get_str_array("drill_down_paths"),
        observability_surfaces: get_str_array("observability_surfaces"),
        duration: get_str("duration"),
        verification_result: {
            let s = get_str("verification_result");
            if s.is_empty() {
                "untested".to_string()
            } else {
                s
            }
        },
        completed_at: get_str("completed_at"),
        blocker_discovered: fm
            .get("blocker_discovered")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

fn extract_one_liner(body_lines: &[&str], h1: Option<&&str>) -> String {
    if let Some(h1_line) = h1 {
        if let Some(idx) = body_lines.iter().position(|&l| l == *h1_line) {
            for line in &body_lines[idx + 1..] {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if trimmed.starts_with("**") && trimmed.ends_with("**") {
                        return trimmed[2..trimmed.len() - 2].to_string();
                    }
                    break;
                }
            }
        }
    }
    String::new()
}

fn parse_files_modified(body: &str) -> Vec<FileModified> {
    let mut files_modified = Vec::new();

    let files_section = extract_section(body, "Files Created/Modified", 2)
        .or_else(|| extract_section(body, "Files Modified", 2));

    if let Some(section) = files_section {
        let file_re = Regex::new(r#"^`([^`]+)`\s*[—–-]\s*(.+)"#).unwrap();
        for line in section.lines() {
            let trimmed = line
                .trim()
                .trim_start_matches('-')
                .trim_start_matches('*')
                .trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some(caps) = file_re.captures(trimmed) {
                files_modified.push(FileModified {
                    path: caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string(),
                    description: caps
                        .get(2)
                        .map(|m| m.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                });
            }
        }
    }

    files_modified
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_summary_basic() {
        let content = r#"---
id: S01
milestone: M001
provides:
  - Feature A
  - Feature B
---

# Slice Summary

**One liner here**

## What Happened

We built the feature.

## Deviations

None.

## Files Created/Modified

- `src/main.rs` — Added main function
- `src/lib.rs` — Added library
"#;

        let summary = parse_summary(content);

        assert_eq!(summary.frontmatter.id, "S01");
        assert_eq!(summary.frontmatter.milestone, "M001");
        assert_eq!(summary.frontmatter.provides.len(), 2);
        assert_eq!(summary.title, "Slice Summary");
        assert_eq!(summary.one_liner, "One liner here");
        assert_eq!(summary.what_happened, "We built the feature.");
        assert_eq!(summary.deviations, "None.");
        assert_eq!(summary.files_modified.len(), 2);
        assert_eq!(summary.files_modified[0].path, "src/main.rs");
        assert_eq!(summary.files_modified[0].description, "Added main function");
    }

    #[test]
    fn test_parse_summary_with_requires() {
        let content = r#"---
id: S02
requires:
  - slice: S01
    provides: Base Library
---

# Summary

## What Happened

Work done.
"#;

        let summary = parse_summary(content);

        assert_eq!(summary.frontmatter.requires.len(), 1);
        assert_eq!(summary.frontmatter.requires[0].slice, "S01");
        assert_eq!(summary.frontmatter.requires[0].provides, "Base Library");
    }

    #[test]
    fn test_parse_files_modified_alternative_heading() {
        let content = r#"# Test

## Files Modified

- `test.rs` — Test file
"#;

        let summary = parse_summary(content);

        assert_eq!(summary.files_modified.len(), 1);
        assert_eq!(summary.files_modified[0].path, "test.rs");
    }

    // --- Serde roundtrips ---

    #[test]
    fn summary_frontmatter_serde_roundtrip() {
        let fm = SummaryFrontmatter {
            id: "S01".into(),
            parent: "P01".into(),
            milestone: "M01".into(),
            provides: vec!["feature".into()],
            requires: vec![RequiresEntry {
                slice: "S00".into(),
                provides: "base".into(),
            }],
            affects: vec!["module_a".into()],
            key_files: vec!["src/main.rs".into()],
            key_decisions: vec!["use async".into()],
            patterns_established: vec![],
            drill_down_paths: vec![],
            observability_surfaces: vec![],
            duration: "5m".into(),
            verification_result: "passed".into(),
            completed_at: "2026-04-12".into(),
            blocker_discovered: false,
        };
        let json = serde_json::to_string(&fm).unwrap();
        let decoded: SummaryFrontmatter = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "S01");
        assert_eq!(decoded.provides.len(), 1);
        assert_eq!(decoded.requires.len(), 1);
        assert_eq!(decoded.verification_result, "passed");
    }

    #[test]
    fn summary_frontmatter_default_verification() {
        let fm = SummaryFrontmatter {
            id: "S02".into(),
            parent: String::new(),
            milestone: String::new(),
            provides: vec![],
            requires: vec![],
            affects: vec![],
            key_files: vec![],
            key_decisions: vec![],
            patterns_established: vec![],
            drill_down_paths: vec![],
            observability_surfaces: vec![],
            duration: String::new(),
            verification_result: default_verification_result(),
            completed_at: String::new(),
            blocker_discovered: false,
        };
        assert_eq!(fm.verification_result, "untested");
    }

    #[test]
    fn requires_entry_serde_roundtrip() {
        let re = RequiresEntry {
            slice: "S03".into(),
            provides: "auth".into(),
        };
        let json = serde_json::to_string(&re).unwrap();
        let decoded: RequiresEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.slice, "S03");
        assert_eq!(decoded.provides, "auth");
    }

    #[test]
    fn file_modified_serde_roundtrip() {
        let fm = FileModified {
            path: "src/lib.rs".into(),
            description: "added module".into(),
        };
        let json = serde_json::to_string(&fm).unwrap();
        let decoded: FileModified = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.path, "src/lib.rs");
    }

    #[test]
    fn summary_serde_roundtrip() {
        let s = Summary {
            frontmatter: SummaryFrontmatter {
                id: "S10".into(),
                parent: "P01".into(),
                milestone: "M01".into(),
                provides: vec!["feature".into()],
                requires: vec![],
                affects: vec![],
                key_files: vec!["src/a.rs".into()],
                key_decisions: vec![],
                patterns_established: vec![],
                drill_down_paths: vec![],
                observability_surfaces: vec![],
                duration: "3m".into(),
                verification_result: "passed".into(),
                completed_at: "2026-04-12".into(),
                blocker_discovered: true,
            },
            title: "Test Summary".into(),
            one_liner: "A quick summary".into(),
            what_happened: "Work was done".into(),
            deviations: "None".into(),
            files_modified: vec![FileModified {
                path: "a.rs".into(),
                description: "created".into(),
            }],
        };
        let json = serde_json::to_string(&s).unwrap();
        let decoded: Summary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.title, "Test Summary");
        assert!(decoded.frontmatter.blocker_discovered);
        assert_eq!(decoded.frontmatter.parent, "P01");
    }

    // --- Parse edge cases ---

    #[test]
    fn parse_summary_no_frontmatter() {
        let content = "# Title\n\n## What Happened\nSomething.\n";
        let summary = parse_summary(content);
        assert_eq!(summary.title, "Title");
        assert!(summary.frontmatter.id.is_empty());
    }

    #[test]
    fn parse_summary_empty() {
        let summary = parse_summary("");
        assert!(summary.title.is_empty());
        assert!(summary.frontmatter.id.is_empty());
        assert!(summary.files_modified.is_empty());
    }

    #[test]
    fn parse_summary_one_liner_bold() {
        let content = "# My Slice\n\n**This is the one liner**\n\n## What Happened\nDone.\n";
        let summary = parse_summary(content);
        assert_eq!(summary.one_liner, "This is the one liner");
    }

    #[test]
    fn parse_summary_no_one_liner() {
        let content = "# My Slice\n\n## What Happened\nDone.\n";
        let summary = parse_summary(content);
        assert!(summary.one_liner.is_empty());
    }
}
