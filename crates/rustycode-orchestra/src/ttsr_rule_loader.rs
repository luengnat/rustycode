//! Orchestra TTSR Rule Loader — Scan and parse TTSR rule files.
//!
//! Scans global (~/.orchestra/agent/rules/*.md) and project-local (.orchestra/rules/*.md)
//! rule files. Parses YAML frontmatter for condition, scope, globs.
//! Project rules override global rules with the same name.
//!
//! Matches orchestra-2's rule-loader.ts implementation.

use crate::frontmatter::{parse_frontmatter_map, split_frontmatter, FrontmatterMap};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ─── Types ───────────────────────────────────────────────────────────────────

/// TTSR rule loaded from markdown file
#[derive(Debug, Clone)]
pub struct Rule {
    /// Rule name (filename without .md extension)
    pub name: String,
    /// Full path to the rule file
    pub path: PathBuf,
    /// Rule content (markdown body without frontmatter)
    pub content: String,
    /// Conditions that must match for rule to apply
    pub condition: Vec<String>,
    /// Optional scope restrictions
    pub scope: Option<Vec<String>>,
    /// Optional glob patterns for file matching
    pub globs: Option<Vec<String>>,
}

// ─── Rule Parsing ───────────────────────────────────────────────────────────

/// Parse a TTSR rule file from disk.
///
/// Reads the file, splits frontmatter, parses metadata, and extracts
/// condition, scope, and globs from the frontmatter.
///
/// # Arguments
/// * `file_path` - Path to the rule file
///
/// # Returns
/// Parsed rule or `None` if file is invalid or missing required fields
fn parse_rule_file(file_path: &Path) -> Option<Rule> {
    // Read file content
    let content = fs::read_to_string(file_path).ok()?;

    // Split frontmatter from body
    let fm_lines = split_frontmatter(&content)?;
    // Body is everything after the closing "---" delimiter
    // Frontmatter format: ---\n<yaml>\n---\n<body>
    let body = if let Some(first_delim) = content.find("---") {
        let after_first = &content[first_delim + 3..];
        // Skip optional newline after first ---
        let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);
        if let Some(second_delim) = after_first.find("\n---") {
            let after_second = &after_first[second_delim + 4..]; // skip \n---
            after_second.trim_start()
        } else {
            content.as_str()
        }
    } else {
        content.as_str()
    };

    // Parse frontmatter
    let meta = parse_frontmatter_map(&fm_lines);

    // Extract condition (required)
    let condition = extract_string_array(&meta, "condition")?;
    if condition.is_empty() {
        return None;
    }

    // Extract rule name from filename
    let name = file_path.file_stem()?.to_str()?.to_string();

    // Extract optional fields
    let scope = extract_string_array(&meta, "scope");
    let globs = extract_string_array(&meta, "globs");

    Some(Rule {
        name,
        path: file_path.to_path_buf(),
        content: body.trim().to_string(),
        condition,
        scope,
        globs,
    })
}

/// Extract a string array from frontmatter map.
///
/// # Arguments
/// * `meta` - Frontmatter map
/// * `key` - Key to extract
///
/// # Returns
/// Vector of strings or `None` if key not found or not an array
fn extract_string_array(meta: &FrontmatterMap, key: &str) -> Option<Vec<String>> {
    match meta.get(key) {
        Some(crate::frontmatter::FrontmatterValue::Array(arr)) => {
            let mut result = Vec::new();
            for item in arr {
                if let crate::frontmatter::FrontmatterValue::String(s) = item {
                    result.push(s.clone());
                }
            }
            if result.is_empty() {
                None
            } else {
                Some(result)
            }
        }
        _ => None,
    }
}

// ─── Directory Scanning ─────────────────────────────────────────────────────

/// Scan a directory for TTSR rule files (*.md).
///
/// # Arguments
/// * `dir` - Directory to scan
///
/// # Returns
/// Vector of parsed rules (empty if directory doesn't exist or is unreadable)
fn scan_dir(dir: &Path) -> Vec<Rule> {
    let mut rules = Vec::new();

    // Check if directory exists
    if !dir.exists() {
        return rules;
    }

    // Read directory entries
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return rules, // Directory unreadable
    };

    // Process each .md file
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        if let Some(rule) = parse_rule_file(&path) {
            rules.push(rule);
        }
    }

    rules
}

// ─── Rule Loading ───────────────────────────────────────────────────────────

/// Load all TTSR rules from global and project-local directories.
///
/// Scans both `~/.orchestra/agent/rules/*.md` (global) and `.orchestra/rules/*.md` (project).
/// Project rules override global rules with the same name.
///
/// # Arguments
/// * `cwd` - Current working directory (project root)
///
/// # Returns
/// Vector of rules with project rules overriding global rules
///
/// # Examples
/// ```
/// use rustycode_orchestra::ttsr_rule_loader::load_rules;
///
/// let rules = load_rules("/path/to/project");
/// for rule in rules {
///     println!("Rule: {}", rule.name);
/// }
/// ```
pub fn load_rules(cwd: &Path) -> Vec<Rule> {
    // Get global rules directory
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let global_dir = home.join(".orchestra").join("agent").join("rules");

    // Get project rules directory
    let project_dir = cwd.join(".orchestra").join("rules");

    // Scan both directories
    let global_rules = scan_dir(&global_dir);
    let project_rules = scan_dir(&project_dir);

    // Merge: project rules override global by name
    let mut by_name: HashMap<String, Rule> = HashMap::new();

    for rule in global_rules {
        by_name.insert(rule.name.clone(), rule);
    }

    for rule in project_rules {
        by_name.insert(rule.name.clone(), rule);
    }

    // Convert to vector
    by_name.into_values().collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_rule_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(format!("{}.md", name));
        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_parse_rule_file_valid() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
condition:
  - test-condition
scope:
  - project
---
Rule content here
"#;
        let path = create_rule_file(temp_dir.path(), "test-rule", content);

        let rule = parse_rule_file(&path).unwrap();
        assert_eq!(rule.name, "test-rule");
        assert_eq!(rule.content, "Rule content here");
        assert_eq!(rule.condition, vec!["test-condition"]);
        assert_eq!(rule.scope, Some(vec!["project".to_string()]));
        assert_eq!(rule.globs, None);
    }

    #[test]
    fn test_parse_rule_file_with_globs() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
condition:
  - rust
globs:
  - "*.rs"
  - "Cargo.toml"
---
Check Rust code style
"#;
        let path = create_rule_file(temp_dir.path(), "rust-style", content);

        let rule = parse_rule_file(&path).unwrap();
        assert_eq!(rule.name, "rust-style");
        assert_eq!(rule.condition, vec!["rust"]);
        assert_eq!(
            rule.globs,
            Some(vec!["*.rs".to_string(), "Cargo.toml".to_string()])
        );
    }

    #[test]
    fn test_parse_rule_file_no_condition() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
scope:
  - project
---
No condition
"#;
        let path = create_rule_file(temp_dir.path(), "invalid", content);

        let rule = parse_rule_file(&path);
        assert!(rule.is_none(), "Should reject rules without condition");
    }

    #[test]
    fn test_parse_rule_file_empty_condition() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"---
condition: []
---
Empty condition
"#;
        let path = create_rule_file(temp_dir.path(), "invalid", content);

        let rule = parse_rule_file(&path);
        assert!(rule.is_none(), "Should reject rules with empty condition");
    }

    #[test]
    fn test_parse_rule_file_no_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let content = "No frontmatter here";
        let path = create_rule_file(temp_dir.path(), "no-fm", content);

        let rule = parse_rule_file(&path);
        assert!(rule.is_none(), "Should reject rules without frontmatter");
    }

    #[test]
    fn test_scan_dir_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent");

        let rules = scan_dir(&nonexistent);
        assert_eq!(rules.len(), 0);
    }

    #[test]
    fn test_scan_dir_with_rules() {
        let temp_dir = TempDir::new().unwrap();

        // Create valid rule
        let content1 = r#"---
condition:
  - test1
---
Content 1
"#;
        create_rule_file(temp_dir.path(), "rule1", content1);

        // Create another valid rule
        let content2 = r#"---
condition:
  - test2
---
Content 2
"#;
        create_rule_file(temp_dir.path(), "rule2", content2);

        // Create invalid file (no .md extension)
        let _ = create_rule_file(temp_dir.path(), "readme.txt", "Not a markdown file");

        let rules = scan_dir(temp_dir.path());
        assert_eq!(rules.len(), 2);
        let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"rule1"), "Expected rule1 in {:?}", names);
        assert!(names.contains(&"rule2"), "Expected rule2 in {:?}", names);
    }

    #[test]
    fn test_load_rules_project_overrides_global() {
        let temp_dir = TempDir::new().unwrap();
        let home = temp_dir.path().join("home");
        let cwd = temp_dir.path().join("project");

        // Create global rules directory
        let global_dir = home.join(".orchestra").join("agent").join("rules");
        fs::create_dir_all(&global_dir).unwrap();

        // Create project rules directory
        let project_dir = cwd.join(".orchestra").join("rules");
        fs::create_dir_all(&project_dir).unwrap();

        // Set HOME for this test
        std::env::set_var("HOME", &home);

        // Create global rule
        let global_content = r#"---
condition:
  - global
---
Global rule content
"#;
        create_rule_file(&global_dir, "shared-rule", global_content);

        // Create project rule with same name (should override)
        let project_content = r#"---
condition:
  - project
---
Project rule content
"#;
        create_rule_file(&project_dir, "shared-rule", project_content);

        // Create project-specific rule
        let project_only_content = r#"---
condition:
  - project-only
---
Project-only rule
"#;
        create_rule_file(&project_dir, "project-only", project_only_content);

        // Reset dirs cache by using custom function
        let rules = load_rules_with_home(&cwd, &home);

        assert_eq!(rules.len(), 2);

        // Find shared-rule (should be project version)
        let shared_rule = rules.iter().find(|r| r.name == "shared-rule").unwrap();
        assert_eq!(shared_rule.content, "Project rule content");

        // Find project-only rule
        let project_only = rules.iter().find(|r| r.name == "project-only").unwrap();
        assert_eq!(project_only.content, "Project-only rule");
    }

    #[test]
    fn test_load_rules_empty() {
        let temp_dir = TempDir::new().unwrap();
        let home = temp_dir.path().join("home");
        let cwd = temp_dir.path().join("project");

        // Create empty directories
        let global_dir = home.join(".orchestra").join("agent").join("rules");
        fs::create_dir_all(&global_dir).unwrap();

        let project_dir = cwd.join(".orchestra").join("rules");
        fs::create_dir_all(&project_dir).unwrap();

        std::env::set_var("HOME", &home);

        // Reset dirs cache by using custom function
        let rules = load_rules_with_home(&cwd, &home);
        assert_eq!(rules.len(), 0);
    }

    // Helper function for testing that allows specifying home directory
    fn load_rules_with_home(cwd: &Path, home: &Path) -> Vec<Rule> {
        let global_dir = home.join(".orchestra").join("agent").join("rules");
        let project_dir = cwd.join(".orchestra").join("rules");

        let global_rules = scan_dir(&global_dir);
        let project_rules = scan_dir(&project_dir);

        let mut by_name: HashMap<String, Rule> = HashMap::new();

        for rule in global_rules {
            by_name.insert(rule.name.clone(), rule);
        }

        for rule in project_rules {
            by_name.insert(rule.name.clone(), rule);
        }

        by_name.into_values().collect()
    }
}
