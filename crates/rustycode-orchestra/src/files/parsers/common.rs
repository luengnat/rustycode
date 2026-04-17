//! Common parsing utilities for Autonomous Mode file parsing
//!
//! Shared functions used across multiple parsers:
//! - Markdown section extraction
//! - Frontmatter parsing
//! - Bullet list parsing
//! - Field extraction

use indexmap::IndexMap;
use regex::Regex;
use std::collections::HashMap;

/// Extract a section after a heading
///
/// # Arguments
/// * `body` - Full markdown content
/// * `heading` - Section heading text (without the # prefix)
/// * `level` - Heading level (1-6, where 1 = #, 2 = ##, etc.)
///
/// # Returns
/// The section content between the heading and the next heading of the same or higher level
///
/// # Example
/// ```ignore
/// let content = "# Introduction\n\nSome text\n\n## Details\n\nMore text";
/// let section = extract_section(content, "Introduction", 1);
/// assert_eq!(section.unwrap(), "Some text");
/// ```
pub fn extract_section(body: &str, heading: &str, level: usize) -> Option<String> {
    let prefix = "#".repeat(level) + " ";
    let pattern = format!("(?m)^{}{}\\s*$", prefix, regex_escape(heading));
    let re = Regex::new(&pattern).unwrap();

    let mat = re.find(body)?;
    let start = mat.end();

    let rest = &body[start..];
    // Match any heading at same or higher level (fewer #'s)
    let next_heading_re = Regex::new(&format!("(?m)^#{{1,{}}}\\s", level)).unwrap();
    let end = next_heading_re
        .find(rest)
        .map(|m| m.start())
        .unwrap_or(rest.len());

    Some(rest[..end].trim().to_string())
}

/// Extract all sections at a given heading level
///
/// Returns an IndexMap mapping section headings to their content (preserves order)
pub fn extract_all_sections(body: &str, level: usize) -> IndexMap<String, String> {
    let prefix = "#".repeat(level) + " ";
    let pattern = format!("(?m)^{}(.+)$", prefix);
    let re = Regex::new(&pattern).unwrap();

    let mut sections = IndexMap::new();
    let matches: Vec<_> = re.find_iter(body).collect();

    for (i, mat) in matches.iter().enumerate() {
        let heading = mat.as_str()[prefix.len()..].trim().to_string();
        let start = mat.end();
        let end = if i + 1 < matches.len() {
            matches[i + 1].start()
        } else {
            body.len()
        };
        sections.insert(heading, body[start..end].trim().to_string());
    }

    sections
}

/// Extract a subsection by name within a parent section
///
/// # Arguments
/// * `content` - Parent section content
/// * `subsection` - Subsection heading to find (H3 subsection within an H2 section)
///
/// # Returns
/// Content of the subsection, or None if not found
pub fn extract_subsection(content: &str, subsection: &str) -> Option<String> {
    extract_section(content, subsection, 3)
}

/// Parse bullet list items from markdown text
///
/// Extracts items starting with `- ` or `* `
pub fn parse_bullets(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                trimmed[2..].trim().to_string()
            } else {
                trimmed.to_string()
            }
        })
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Extract a bold field value from markdown
///
/// Looks for `**key:** value` patterns and returns the value
pub fn extract_bold_field(text: &str, key: &str) -> Option<String> {
    let pattern = format!("(?m)^\\*\\*{}:\\*\\*\\s*(.+)$", regex_escape(key));
    let re = Regex::new(&pattern).unwrap();
    re.captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string())
}

/// Split YAML frontmatter from markdown body
///
/// Returns (frontmatter, body) tuple. Frontmatter is None if not present.
pub fn split_frontmatter(content: &str) -> (Option<String>, String) {
    let re = Regex::new(r"(?m)^---\r?\n([\s\S]*?)\r?\n---\r?\n?").unwrap();
    if let Some(caps) = re.captures(content) {
        let fm = caps.get(1).map(|m| m.as_str().to_string());
        let body = match caps.get(0) {
            Some(m) => &content[m.end()..],
            None => content,
        };
        (fm, body.trim().to_string())
    } else {
        (None, content.to_string())
    }
}

/// Parse YAML frontmatter into a HashMap
///
/// Supports scalar values and array items (indented with `- `)
pub fn parse_frontmatter_map(fm_lines: &str) -> HashMap<String, serde_yaml::Value> {
    let mut result = HashMap::new();
    let lines: Vec<&str> = fm_lines.lines().collect();
    let mut current_key: Option<String> = None;
    let mut current_array: Vec<String> = Vec::new();

    // Scalar value pattern: key: value
    let scalar_re = Regex::new(r"^(\w[\w_]*):\s*(.+)$").unwrap();
    // Array start pattern: key: or key: []
    let array_start_re = Regex::new(r"^(\w[\w_]*):\s*(\[\])?\s*$").unwrap();
    // Array item pattern:   - value
    let item_re = Regex::new(r"^\s+-\s+(.+)$").unwrap();

    for line in lines {
        if let Some(caps) = scalar_re.captures(line) {
            // Close any pending array
            if let Some(key) = current_key.take() {
                if !current_array.is_empty() {
                    result.insert(
                        key.clone(),
                        serde_yaml::Value::Sequence(
                            current_array
                                .iter()
                                .map(|v| serde_yaml::Value::String(v.clone()))
                                .collect(),
                        ),
                    );
                    current_array.clear();
                }
            }

            let key = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let value = caps.get(2).map(|m| m.as_str()).unwrap_or("").trim();
            current_key = Some(key.clone());
            result.insert(key, serde_yaml::Value::String(value.to_string()));
        } else if let Some(caps) = array_start_re.captures(line) {
            if let Some(key) = current_key.take() {
                if !current_array.is_empty() {
                    result.insert(
                        key.clone(),
                        serde_yaml::Value::Sequence(
                            current_array
                                .iter()
                                .map(|v| serde_yaml::Value::String(v.clone()))
                                .collect(),
                        ),
                    );
                    current_array.clear();
                }
            }
            current_key = caps.get(1).map(|m| m.as_str()).map(|s| s.to_string());
        } else if let Some(caps) = item_re.captures(line) {
            if current_key.is_some() {
                let value = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
                current_array.push(value.to_string());
            }
        }
    }

    // Don't forget last array
    if let Some(key) = current_key {
        if !current_array.is_empty() {
            result.insert(
                key,
                serde_yaml::Value::Sequence(
                    current_array
                        .iter()
                        .map(|v| serde_yaml::Value::String(v.clone()))
                        .collect(),
                ),
            );
        }
    }

    result
}

/// Escape special regex characters
fn regex_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '[' | ']' | '\\' | '|' => {
                format!("\\{}", c)
            }
            _ => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_section ---

    #[test]
    fn extract_section_h2() {
        let content = "## Goal\n\nCreate a feature\n\n## Tasks\n\n- T01\n";
        let section = extract_section(content, "Goal", 2).unwrap();
        assert_eq!(section, "Create a feature");
    }

    #[test]
    fn extract_section_h1() {
        let content = "# Title\n\nBody text\n\n# Next\n\nOther";
        let section = extract_section(content, "Title", 1).unwrap();
        assert_eq!(section, "Body text");
    }

    #[test]
    fn extract_section_not_found() {
        let content = "## Goal\n\nText\n";
        assert!(extract_section(content, "Missing", 2).is_none());
    }

    #[test]
    fn extract_section_empty() {
        assert!(extract_section("", "Anything", 2).is_none());
    }

    #[test]
    fn extract_section_last_section_to_eof() {
        let content = "## Last\n\nFinal content here\n";
        let section = extract_section(content, "Last", 2).unwrap();
        assert_eq!(section, "Final content here");
    }

    // --- extract_all_sections ---

    #[test]
    fn extract_all_sections_h2() {
        let content = "## Alpha\n\nA\n\n## Beta\n\nB\n";
        let sections = extract_all_sections(content, 2);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections["Alpha"], "A");
        assert_eq!(sections["Beta"], "B");
    }

    #[test]
    fn extract_all_sections_empty() {
        let sections = extract_all_sections("", 2);
        assert!(sections.is_empty());
    }

    // --- extract_subsection ---

    #[test]
    fn test_extract_subsection() {
        let content = "### Details\n\nSome details here\n### Other\n\nOther stuff\n";
        let sub = extract_subsection(content, "Details").unwrap();
        assert_eq!(sub, "Some details here");
    }

    // --- parse_bullets ---

    #[test]
    fn parse_bullets_dash() {
        let text = "- Item 1\n- Item 2\n- Item 3";
        let bullets = parse_bullets(text);
        assert_eq!(bullets, vec!["Item 1", "Item 2", "Item 3"]);
    }

    #[test]
    fn parse_bullets_asterisk() {
        let text = "* First\n* Second";
        let bullets = parse_bullets(text);
        assert_eq!(bullets, vec!["First", "Second"]);
    }

    #[test]
    fn parse_bullets_skips_headings_and_empty() {
        let text = "- Valid\n## Heading\n\n- Also valid";
        let bullets = parse_bullets(text);
        assert_eq!(bullets.len(), 2);
        assert_eq!(bullets[0], "Valid");
    }

    #[test]
    fn parse_bullets_empty() {
        assert!(parse_bullets("").is_empty());
    }

    // --- extract_bold_field ---

    #[test]
    fn extract_bold_field_present() {
        let text = "**Name:** Alice\n**Age:** 30";
        assert_eq!(extract_bold_field(text, "Name"), Some("Alice".into()));
        assert_eq!(extract_bold_field(text, "Age"), Some("30".into()));
    }

    #[test]
    fn extract_bold_field_missing() {
        assert!(extract_bold_field("**Name:** Alice", "Missing").is_none());
    }

    // --- split_frontmatter ---

    #[test]
    fn split_frontmatter_present() {
        let content = "---\nid: S01\n---\n\nBody text\n";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_some());
        assert!(fm.unwrap().contains("id: S01"));
        assert!(body.contains("Body text"));
    }

    #[test]
    fn split_frontmatter_absent() {
        let content = "# No frontmatter\n\nJust body\n";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
        assert!(body.contains("No frontmatter"));
    }

    #[test]
    fn split_frontmatter_empty() {
        let (fm, body) = split_frontmatter("");
        assert!(fm.is_none());
        assert!(body.is_empty());
    }

    // --- parse_frontmatter_map ---

    #[test]
    fn parse_frontmatter_map_scalars() {
        let fm = "id: S01\nmilestone: M01\n";
        let map = parse_frontmatter_map(fm);
        assert_eq!(map.get("id").unwrap().as_str(), Some("S01"));
        assert_eq!(map.get("milestone").unwrap().as_str(), Some("M01"));
    }

    #[test]
    fn parse_frontmatter_map_arrays() {
        let fm = "provides:\n  - Feature A\n  - Feature B\n";
        let map = parse_frontmatter_map(fm);
        let arr = map.get("provides").unwrap().as_sequence().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn parse_frontmatter_map_mixed() {
        let fm = "id: S01\nprovides:\n  - A\n  - B\nmilestone: M01\n";
        let map = parse_frontmatter_map(fm);
        assert_eq!(map.get("id").unwrap().as_str(), Some("S01"));
        assert_eq!(map.get("milestone").unwrap().as_str(), Some("M01"));
        assert_eq!(map.get("provides").unwrap().as_sequence().unwrap().len(), 2);
    }

    #[test]
    fn parse_frontmatter_map_empty() {
        let map = parse_frontmatter_map("");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_frontmatter_map_empty_array_bracket() {
        let fm = "tags: []\nid: X\n";
        let map = parse_frontmatter_map(fm);
        assert_eq!(map.get("id").unwrap().as_str(), Some("X"));
    }
}
