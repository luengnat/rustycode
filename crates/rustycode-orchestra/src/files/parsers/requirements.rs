//! Requirements parser
//!
//! Parses requirement counts from REQUIREMENTS.md files:
//! - Active, validated, deferred, out of scope counts
//! - Blocked requirement detection

use crate::files::parsers::common::extract_section;
use regex::Regex;
use std::sync::LazyLock;

static H3_REQ_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^###\s+[A-Z][\w-]*\d+\s+[—-]").unwrap());
static BLOCKED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^-\s+Status:\s+blocked\s*$"#).unwrap());

/// Requirement counts
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequirementCounts {
    pub active: usize,
    pub validated: usize,
    pub deferred: usize,
    pub out_of_scope: usize,
    pub blocked: usize,
    pub total: usize,
}

/// Task plan must-have item
#[derive(Debug, Clone)]
pub struct MustHaveItem {
    pub text: String,
    pub checked: bool,
}

/// Parse requirement counts
pub fn parse_requirement_counts(content: Option<&str>) -> RequirementCounts {
    let mut counts = RequirementCounts {
        active: 0,
        validated: 0,
        deferred: 0,
        out_of_scope: 0,
        blocked: 0,
        total: 0,
    };

    let content = match content {
        Some(c) => c,
        None => return counts,
    };

    let sections = [
        ("active", "Active"),
        ("validated", "Validated"),
        ("deferred", "Deferred"),
        ("out_of_scope", "Out of Scope"),
    ];

    for (key, heading) in sections {
        if let Some(section) = extract_section(content, heading, 2) {
            let matches = H3_REQ_RE.find_iter(&section).count();
            match key {
                "active" => counts.active = matches,
                "validated" => counts.validated = matches,
                "deferred" => counts.deferred = matches,
                "out_of_scope" => counts.out_of_scope = matches,
                _ => {}
            }
        }
    }

    counts.blocked = BLOCKED_RE.find_iter(content).count();
    counts.total = counts.active + counts.validated + counts.deferred + counts.out_of_scope;

    counts
}

/// Parse task plan must-haves
pub fn parse_task_plan_must_haves(content: &str) -> Vec<MustHaveItem> {
    let (_fm_lines, body) = crate::files::parsers::common::split_frontmatter(content);
    let section = match extract_section(&body, "Must-Haves", 2) {
        Some(s) => s,
        None => return Vec::new(),
    };

    let bullets = crate::files::parsers::common::parse_bullets(&section);
    if bullets.is_empty() {
        return Vec::new();
    }

    bullets
        .iter()
        .map(|line| {
            let cb_re = regex::Regex::new(r#"^\[([xX ])\]\s+(.+)"#).unwrap();
            if let Some(caps) = cb_re.captures(line) {
                MustHaveItem {
                    text: caps
                        .get(2)
                        .map(|m| m.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                    checked: caps.get(1).map(|m| m.as_str()) == Some("x")
                        || caps.get(1).map(|m| m.as_str()) == Some("X"),
                }
            } else {
                MustHaveItem {
                    text: line.trim().to_string(),
                    checked: false,
                }
            }
        })
        .collect()
}

/// Count must-haves mentioned in summary
pub fn count_must_haves_mentioned_in_summary(
    must_haves: &[MustHaveItem],
    summary_content: &str,
) -> usize {
    if summary_content.is_empty() || must_haves.is_empty() {
        return 0;
    }

    let summary_lower = summary_content.to_lowercase();
    let common_words: std::collections::HashSet<&str> = [
        "the", "and", "for", "are", "but", "not", "you", "all", "can", "had", "her", "was", "one",
        "our", "out", "has", "its", "let", "say", "she", "too", "use", "with", "have", "from",
        "this", "that", "they", "been", "each", "when", "will", "does", "into", "also", "than",
        "them", "then", "some", "what", "only", "just", "more", "make", "like", "made", "over",
        "such", "take", "most", "very", "must", "file", "test", "tests", "task", "new", "add",
        "added", "existing",
    ]
    .iter()
    .cloned()
    .collect();

    let mut count = 0;

    for mh in must_haves {
        let code_tokens = extract_code_tokens(&mh.text);

        if !code_tokens.is_empty() {
            if code_tokens
                .iter()
                .any(|token| summary_lower.contains(&token.to_lowercase()))
            {
                count += 1;
            }
        } else {
            let normalized: String = mh
                .text
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c.is_whitespace() {
                        c
                    } else {
                        ' '
                    }
                })
                .collect();
            let words: Vec<&str> = normalized
                .split_whitespace()
                .filter(|w| w.len() >= 4 && !common_words.contains(*w))
                .collect();

            if words
                .iter()
                .any(|word| summary_lower.contains(&word.to_lowercase()))
            {
                count += 1;
            }
        }
    }

    count
}

fn extract_code_tokens(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"`([^`]+)`").unwrap();
    re.captures_iter(text)
        .map(|caps| caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_requirement_counts() {
        let content = r#"# Requirements

## Active

### REQ001 — First requirement

### REQ002 — Second requirement

## Validated

### REQ003 — Third requirement
"#;

        let counts = parse_requirement_counts(Some(content));

        assert_eq!(counts.active, 2);
        assert_eq!(counts.validated, 1);
        assert_eq!(counts.total, 3);
    }

    #[test]
    fn test_parse_task_plan_must_haves() {
        let content = r#"---
id: T01
---

## Must-Haves

- [x] First done item
- [ ] Second todo item
- Third plain item
"#;

        let items = parse_task_plan_must_haves(content);

        assert_eq!(items.len(), 3);
        assert!(items[0].checked);
        assert!(!items[1].checked);
        assert!(!items[2].checked);
    }

    #[test]
    fn test_count_must_haves_mentioned_in_summary() {
        let must_haves = vec![
            MustHaveItem {
                text: "Implement `fooBar` function".to_string(),
                checked: false,
            },
            MustHaveItem {
                text: "Add tests for baz".to_string(),
                checked: false,
            },
        ];

        let summary = "Implemented the fooBar function successfully.";
        let count = count_must_haves_mentioned_in_summary(&must_haves, summary);

        assert_eq!(count, 1);
    }

    // --- Serde roundtrips ---

    #[test]
    fn requirement_counts_serde_roundtrip() {
        let counts = RequirementCounts {
            active: 5,
            validated: 3,
            deferred: 1,
            out_of_scope: 0,
            blocked: 2,
            total: 8,
        };
        let json = serde_json::to_string(&counts).unwrap();
        let decoded: RequirementCounts = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.active, 5);
        assert_eq!(decoded.total, 8);
        assert_eq!(decoded.blocked, 2);
    }

    // --- Parse edge cases ---

    #[test]
    fn parse_requirement_counts_none() {
        let counts = parse_requirement_counts(None);
        assert_eq!(counts.active, 0);
        assert_eq!(counts.total, 0);
    }

    #[test]
    fn parse_requirement_counts_empty() {
        let counts = parse_requirement_counts(Some(""));
        assert_eq!(counts.active, 0);
    }

    #[test]
    fn count_must_haves_empty_summary() {
        let items = vec![MustHaveItem {
            text: "do thing".into(),
            checked: false,
        }];
        assert_eq!(count_must_haves_mentioned_in_summary(&items, ""), 0);
    }

    #[test]
    fn count_must_haves_no_match() {
        let items = vec![MustHaveItem {
            text: "Implement auth".into(),
            checked: false,
        }];
        assert_eq!(
            count_must_haves_mentioned_in_summary(&items, "Fixed a typo"),
            0
        );
    }

    #[test]
    fn count_must_haves_all_mentioned() {
        let items = vec![
            MustHaveItem {
                text: "Add login".into(),
                checked: true,
            },
            MustHaveItem {
                text: "Add logout".into(),
                checked: true,
            },
        ];
        assert_eq!(
            count_must_haves_mentioned_in_summary(&items, "Added login and logout"),
            2
        );
    }

    #[test]
    fn parse_task_plan_must_haves_no_section() {
        let content = "# No must-haves here\n\nJust text.\n";
        let items = parse_task_plan_must_haves(content);
        assert!(items.is_empty());
    }
}
