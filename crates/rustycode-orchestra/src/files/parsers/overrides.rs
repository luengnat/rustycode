//! Overrides parser
//!
//! Parses Orchestra override sections:
//! - Active and resolved overrides
//! - Timestamp and scope tracking

use once_cell::sync::Lazy;
use regex::Regex;

static CHANGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^\*\*Change:\*\*\s*(.+)$"#).unwrap());
static SCOPE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^\*\*Scope:\*\*\s*(.+)$"#).unwrap());
static APPLIED_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\*\*Applied-at:\*\*\s*(.+)$"#).unwrap());

/// Override entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Override {
    pub timestamp: String,
    pub change: String,
    pub scope: OverrideScope,
    pub applied_at: String,
}

/// Override scope
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum OverrideScope {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "resolved")]
    Resolved,
}

/// Parse overrides
pub fn parse_overrides(content: &str) -> Vec<Override> {
    let mut overrides = Vec::new();
    let blocks: Vec<&str> = content.split("## Override: ").skip(1).collect();

    for block in blocks {
        let lines: Vec<&str> = block.lines().collect();
        let timestamp = lines.first().map(|s| s.trim()).unwrap_or("").to_string();
        let mut change = String::new();
        let mut scope = OverrideScope::Active;
        let mut applied_at = String::new();

        for line in &lines {
            if let Some(caps) = CHANGE_RE.captures(line) {
                change = caps
                    .get(1)
                    .map(|m| m.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }

            if let Some(caps) = SCOPE_RE.captures(line) {
                let scope_str = caps
                    .get(1)
                    .map(|m| m.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_lowercase();
                scope = if scope_str == "resolved" {
                    OverrideScope::Resolved
                } else {
                    OverrideScope::Active
                };
            }

            if let Some(caps) = APPLIED_RE.captures(line) {
                applied_at = caps
                    .get(1)
                    .map(|m| m.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }
        }

        if !change.is_empty() {
            overrides.push(Override {
                timestamp,
                change,
                scope,
                applied_at,
            });
        }
    }

    overrides
}

/// Format overrides section
pub fn format_overrides_section(overrides: &[Override]) -> String {
    if overrides.is_empty() {
        return String::new();
    }

    let entries: Vec<String> = overrides
        .iter()
        .enumerate()
        .map(|(i, o)| {
            format!(
                "{}. **{}**\n   _Issued: {} during {}_",
                i + 1,
                o.change,
                o.timestamp,
                o.applied_at
            )
        })
        .collect();

    format!(
        "## Active Overrides (supersede plan content)\n\n\
         The following overrides were issued by the user and supersede any conflicting content in plan documents below. Follow these overrides even if they contradict the inlined task plan.\n\n\
         {}\n\n",
        entries.join("\n")
    )
}

/// UAT type
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum UatType {
    ArtifactDriven,
    LiveRuntime,
    HumanExperience,
    Mixed,
}

/// Extract UAT type
pub fn extract_uat_type(content: &str) -> Option<UatType> {
    use crate::files::parsers::common::{extract_section, parse_bullets};

    let section = extract_section(content, "UAT Type", 2)?;
    let bullets = parse_bullets(&section);
    let mode_bullet = bullets.iter().find(|b| b.starts_with("UAT mode:"))?;

    let raw_value = mode_bullet["UAT mode:".len()..].trim().to_lowercase();

    if raw_value.starts_with("artifact-driven") {
        Some(UatType::ArtifactDriven)
    } else if raw_value.starts_with("live-runtime") {
        Some(UatType::LiveRuntime)
    } else if raw_value.starts_with("human-experience") {
        Some(UatType::HumanExperience)
    } else if raw_value.starts_with("mixed") {
        Some(UatType::Mixed)
    } else {
        None
    }
}

/// Parse context depends_on
pub fn parse_context_depends_on(content: Option<&str>) -> Vec<String> {
    use crate::files::parsers::common::{parse_frontmatter_map, split_frontmatter};

    let content = match content {
        Some(c) => c,
        None => return Vec::new(),
    };

    let (fm_lines, _) = split_frontmatter(content);
    let fm_lines = match fm_lines {
        Some(fm) => fm,
        None => return Vec::new(),
    };

    let fm = parse_frontmatter_map(&fm_lines);
    let raw = fm.get("depends_on");

    match raw {
        Some(serde_yaml::Value::Sequence(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_overrides() {
        let content = r#"# Orchestra Overrides

## Override: 2025-03-18T10:00:00Z

**Change:** Skip task T01
**Scope:** active
**Applied-at:** M01-S01
"#;

        let overrides = parse_overrides(content);

        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].change, "Skip task T01");
        assert!(matches!(overrides[0].scope, OverrideScope::Active));
    }

    #[test]
    fn test_format_overrides_section() {
        let overrides = vec![Override {
            timestamp: "2025-03-18T10:00:00Z".to_string(),
            change: "Skip task T01".to_string(),
            scope: OverrideScope::Active,
            applied_at: "M01-S01".to_string(),
        }];

        let formatted = format_overrides_section(&overrides);

        assert!(formatted.contains("Active Overrides"));
        assert!(formatted.contains("Skip task T01"));
    }

    #[test]
    fn test_extract_uat_type() {
        let content = "## UAT Type\n\n- UAT mode: artifact-driven";
        let uat_type = extract_uat_type(content);
        assert!(uat_type.is_some());
        assert!(matches!(uat_type.unwrap(), UatType::ArtifactDriven));
    }

    #[test]
    fn test_parse_context_depends_on() {
        let content = r#"---
depends_on:
  - M001
  - M002
---
"#;

        let deps = parse_context_depends_on(Some(content));

        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0], "M001");
    }

    // --- Serde roundtrips ---

    #[test]
    fn override_entry_serde_roundtrip() {
        let o = Override {
            timestamp: "2026-01-01T00:00:00Z".into(),
            change: "Skip tests".into(),
            scope: OverrideScope::Resolved,
            applied_at: "M01-S01".into(),
        };
        let json = serde_json::to_string(&o).unwrap();
        let decoded: Override = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.change, "Skip tests");
        assert!(matches!(decoded.scope, OverrideScope::Resolved));
    }

    #[test]
    fn override_scope_serde_variants() {
        let json1 = serde_json::to_string(&OverrideScope::Active).unwrap();
        let json2 = serde_json::to_string(&OverrideScope::Resolved).unwrap();
        assert_eq!(json1, "\"active\"");
        assert_eq!(json2, "\"resolved\"");
        let d1: OverrideScope = serde_json::from_str(&json1).unwrap();
        assert!(matches!(d1, OverrideScope::Active));
    }

    // --- Parse edge cases ---

    #[test]
    fn parse_overrides_empty() {
        assert!(parse_overrides("").is_empty());
    }

    #[test]
    fn parse_overrides_no_change() {
        // Override block without **Change:** line should be skipped
        let content = "## Override: 2025-01-01\n\n**Scope:** active\n";
        assert!(parse_overrides(content).is_empty());
    }

    #[test]
    fn parse_overrides_resolved_scope() {
        let content = "## Override: 2025-01-01\n\n**Change:** Remove task\n**Scope:** resolved\n**Applied-at:** M01\n";
        let overrides = parse_overrides(content);
        assert_eq!(overrides.len(), 1);
        assert!(matches!(overrides[0].scope, OverrideScope::Resolved));
    }

    #[test]
    fn format_overrides_empty() {
        assert!(format_overrides_section(&[]).is_empty());
    }

    #[test]
    fn format_overrides_multiple() {
        let overrides = vec![
            Override {
                timestamp: "t1".into(),
                change: "First".into(),
                scope: OverrideScope::Active,
                applied_at: "M01".into(),
            },
            Override {
                timestamp: "t2".into(),
                change: "Second".into(),
                scope: OverrideScope::Active,
                applied_at: "M02".into(),
            },
        ];
        let formatted = format_overrides_section(&overrides);
        assert!(formatted.contains("First"));
        assert!(formatted.contains("Second"));
        assert!(formatted.contains("Active Overrides"));
    }

    #[test]
    fn extract_uat_type_live_runtime() {
        let content = "## UAT Type\n\n- UAT mode: live-runtime";
        assert!(matches!(
            extract_uat_type(content),
            Some(UatType::LiveRuntime)
        ));
    }

    #[test]
    fn extract_uat_type_mixed() {
        let content = "## UAT Type\n\n- UAT mode: mixed";
        assert!(matches!(extract_uat_type(content), Some(UatType::Mixed)));
    }

    #[test]
    fn extract_uat_type_human_experience() {
        let content = "## UAT Type\n\n- UAT mode: human-experience";
        assert!(matches!(
            extract_uat_type(content),
            Some(UatType::HumanExperience)
        ));
    }

    #[test]
    fn extract_uat_type_missing() {
        assert!(extract_uat_type("## Other\n\nText").is_none());
        assert!(extract_uat_type("").is_none());
    }

    #[test]
    fn parse_context_depends_on_none() {
        assert!(parse_context_depends_on(None).is_empty());
    }

    #[test]
    fn parse_context_depends_on_empty() {
        assert!(parse_context_depends_on(Some("")).is_empty());
    }

    #[test]
    fn parse_context_depends_on_no_dep_field() {
        let content = "---\nother: value\n---\n";
        assert!(parse_context_depends_on(Some(content)).is_empty());
    }
}
