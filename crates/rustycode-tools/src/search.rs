use crate::security::{validate_list_path, validate_regex_pattern, MAX_REGEX_MATCHES};
use crate::truncation::{truncate_items, GREP_MAX_MATCHES, LIST_MAX_ITEMS};
use crate::{Checkpoint, Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

/// Type alias for the regex cache to reduce type complexity
type RegexCache = Arc<Mutex<lru::LruCache<String, Arc<Regex>>>>;

/// Thread-safe LRU cache for compiled regex patterns
/// Reduces regex compilation overhead for repeated patterns
static REGEX_CACHE: Lazy<RegexCache> = Lazy::new(|| {
    Arc::new(Mutex::new(lru::LruCache::new(
        std::num::NonZeroUsize::new(256).unwrap(),
    )))
});

/// Get or compile a regex pattern from cache
/// Made public for benchmarking and external use
pub fn get_regex(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
    // Try to get from cache first
    {
        let mut cache = REGEX_CACHE.lock();
        if let Some(regex) = cache.get(pattern) {
            return Ok(Arc::clone(regex));
        }
    }

    // Not in cache, compile and insert
    let compiled = Regex::new(pattern)?;
    let compiled = Arc::new(compiled);

    // Insert into cache (will evict LRU if full)
    let mut cache = REGEX_CACHE.lock();
    cache.put(pattern.to_string(), Arc::clone(&compiled));

    Ok(compiled)
}

pub struct GrepTool;
pub struct GlobTool;

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for text patterns across all files in the codebase. Use this to find function definitions, variable usages, or any text pattern in code. Supports simple text search (no regex required) and can show context around matches."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string" },
                "before_context": { "type": "integer", "description": "Lines of context before match" },
                "after_context": { "type": "integer", "description": "Lines of context after match" },
                "max_matches_per_file": { "type": "integer", "description": "Limit matches per file" }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let pattern = required_string(&params, "pattern")?;

        // Validate regex pattern for ReDoS
        validate_regex_pattern(pattern)?;

        let path_str = optional_string(&params, "path").unwrap_or(".");
        let root = validate_list_path(path_str, &ctx.cwd)?;

        // Use cached regex compilation for better performance
        let re = get_regex(pattern)
            .map_err(|e| anyhow!("Invalid regex pattern '{}': {}", pattern, e))?;

        // Get context parameters
        let before_context = params
            .get("before_context")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let after_context = params
            .get("after_context")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let max_matches_per_file = params
            .get("max_matches_per_file")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        // Group matches by file for dense display
        let mut file_matches: Vec<(String, Vec<(usize, String)>)> = Vec::new();

        // Check for cancellation before starting file walk
        ctx.checkpoint()?;

        let mut file_count = 0;
        for entry in WalkDir::new(&root)
            .max_depth(4)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
        {
            file_count += 1;

            // Check for cancellation every 50 files to balance responsiveness with performance
            if file_count % 50 == 0 {
                ctx.checkpoint()?;
            }

            if should_skip(entry.path()) {
                continue;
            }
            let Ok(content) = fs::read_to_string(entry.path()) else {
                continue;
            };

            let lines: Vec<&str> = content.lines().collect();
            let mut matches_in_file = Vec::new();
            let mut match_count = 0;
            let mut total_matches = 0; // Track total across all files

            for (index, line) in lines.iter().enumerate() {
                // Enforce global match limit to prevent DoS
                if total_matches >= MAX_REGEX_MATCHES {
                    break;
                }
                // Use the pre-compiled regex instead of compiling on every iteration
                if re.is_match(line) {
                    total_matches += 1;

                    // Check per-file limit
                    if let Some(limit) = max_matches_per_file {
                        if match_count >= limit {
                            break;
                        }
                        match_count += 1;
                    }

                    // Include context lines if requested
                    #[allow(clippy::needless_range_loop)]
                    if before_context > 0 || after_context > 0 {
                        let start = index.saturating_sub(before_context);
                        let end = (index + after_context + 1).min(lines.len());

                        for ctx_idx in start..end {
                            let prefix = if ctx_idx == index {
                                "→"
                            } else if ctx_idx < index {
                                "◀"
                            } else {
                                "▶"
                            };
                            matches_in_file.push((
                                ctx_idx + 1,
                                format!("{} {}", prefix, lines[ctx_idx].trim()),
                            ));
                        }
                    } else {
                        matches_in_file.push((index + 1, line.trim().to_string()));
                    }
                }
            }
            if !matches_in_file.is_empty() {
                file_matches.push((entry.path().display().to_string(), matches_in_file));

                // Check for cancellation after each file with matches
                ctx.checkpoint()?;
            }
        }

        // Flatten all matches
        let all_matches: Vec<String> = file_matches
            .iter()
            .flat_map(|(path, matches)| {
                matches
                    .iter()
                    .map(move |(line, text)| format!("{}:{} → {}", path, line, text))
            })
            .collect();

        let total_count = all_matches.len();
        let files_with_matches = file_matches.len();

        // Calculate file-level statistics
        let mut file_stats: Vec<(String, usize)> = file_matches
            .iter()
            .map(|(path, matches)| (path.clone(), matches.len()))
            .collect();
        file_stats.sort_by_key(|a| std::cmp::Reverse(a.1));

        // Apply truncation
        let truncated = truncate_items(all_matches, GREP_MAX_MATCHES, "grep results");

        // Format output densely
        let mut output = format!(
            "**{} matches in {} file(s)** for \"{}\"\n\n",
            total_count, files_with_matches, pattern
        );
        output.push_str(truncated.as_str());

        // Build metadata with file statistics
        let mut metadata = truncated.into_metadata();
        metadata["pattern"] = json!(pattern);
        metadata["total_matches"] = json!(total_count);
        metadata["files_with_matches"] = json!(files_with_matches);

        // Add top files by match count (up to 10)
        let top_files: Vec<Value> = file_stats
            .iter()
            .take(10)
            .map(|(path, count)| {
                json!({
                    "path": path,
                    "matches": count
                })
            })
            .collect();
        if !top_files.is_empty() {
            metadata["top_files"] = json!(top_files);
        }

        // Add context parameters to metadata
        if before_context > 0 {
            metadata["before_context"] = json!(before_context);
        }
        if after_context > 0 {
            metadata["after_context"] = json!(after_context);
        }
        if let Some(limit) = max_matches_per_file {
            metadata["max_matches_per_file"] = json!(limit);
        }

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files whose path contains a glob-like fragment."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": { "pattern": { "type": "string" } }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let pattern = required_string(&params, "pattern")?
            .replace('*', "")
            .to_lowercase();

        // Always search from workspace root
        let root = &ctx.cwd;

        // Check for cancellation before starting file walk
        ctx.checkpoint()?;

        let mut matches = Vec::new();
        let mut file_count = 0;
        for entry in WalkDir::new(root)
            .max_depth(5)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
        {
            file_count += 1;

            // Check for cancellation every 50 files
            if file_count % 50 == 0 {
                ctx.checkpoint()?;
            }

            let path = entry.path().display().to_string();
            if should_skip(entry.path()) {
                continue;
            }
            if path.to_lowercase().contains(&pattern) {
                matches.push(path);

                // Check for cancellation after each match
                ctx.checkpoint()?;
            }
        }

        let total_count = matches.len();

        // Calculate file extension statistics (clone path strings to avoid borrow issues)
        let mut extension_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for path in &matches {
            if let Some(ext) = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
            {
                *extension_counts.entry(ext.to_string()).or_insert(0) += 1;
            } else {
                *extension_counts
                    .entry("(no extension)".to_string())
                    .or_insert(0) += 1;
            }
        }

        matches.sort();

        // Apply truncation
        let truncated = truncate_items(matches, LIST_MAX_ITEMS, "glob results");

        // Format output densely
        let output = format!(
            "**{} matches** for \"{}\"\n\n{}",
            total_count,
            pattern,
            truncated.as_str()
        );

        // Build metadata with extension statistics
        let mut metadata = truncated.into_metadata();
        metadata["pattern"] = json!(pattern);
        metadata["total_matches"] = json!(total_count);

        // Add extension breakdown
        if !extension_counts.is_empty() {
            let ext_stats: Vec<Value> = extension_counts
                .into_iter()
                .map(|(ext, count)| {
                    json!({
                        "extension": ext,
                        "count": count
                    })
                })
                .collect();
            metadata["extensions"] = json!(ext_stats);
        }

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string parameter `{key}`"))
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value == ".git" || value == "target" || value == "node_modules"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- get_regex ---

    #[test]
    fn get_regex_valid_pattern() {
        let re = get_regex(r"\d+").unwrap();
        assert!(re.is_match("123"));
        assert!(!re.is_match("abc"));
    }

    #[test]
    fn get_regex_invalid_pattern() {
        assert!(get_regex(r"[invalid").is_err());
    }

    #[test]
    fn get_regex_caches_compiled() {
        let re1 = get_regex(r"test").unwrap();
        let re2 = get_regex(r"test").unwrap();
        assert!(re1.is_match("test"));
        assert!(re2.is_match("test"));
    }

    // --- should_skip ---

    #[test]
    fn should_skip_git() {
        assert!(should_skip(Path::new(".git/refs/heads")));
        assert!(should_skip(Path::new("src/.git/config")));
    }

    #[test]
    fn should_skip_target() {
        assert!(should_skip(Path::new("target/debug/app")));
    }

    #[test]
    fn should_skip_node_modules() {
        assert!(should_skip(Path::new("node_modules/react/index.js")));
    }

    #[test]
    fn should_not_skip_normal() {
        assert!(!should_skip(Path::new("src/main.rs")));
        assert!(!should_skip(Path::new("lib/core/mod.rs")));
    }

    // --- Tool metadata ---

    #[test]
    fn grep_tool_name() {
        assert_eq!(GrepTool.name(), "grep");
    }

    #[test]
    fn grep_tool_permission() {
        assert_eq!(GrepTool.permission(), ToolPermission::Read);
    }

    #[test]
    fn glob_tool_name() {
        assert_eq!(GlobTool.name(), "glob");
    }

    #[test]
    fn glob_tool_permission() {
        assert_eq!(GlobTool.permission(), ToolPermission::Read);
    }

    // --- GrepTool schema ---

    #[test]
    fn grep_tool_schema_has_required_fields() {
        let schema = GrepTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "pattern"));
    }

    // --- GlobTool schema ---

    #[test]
    fn glob_tool_schema_has_required_fields() {
        let schema = GlobTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "pattern"));
    }

    // --- required_string / optional_string helpers ---

    #[test]
    fn required_string_present() {
        let v = serde_json::json!({"name": "test"});
        assert_eq!(required_string(&v, "name").unwrap(), "test");
    }

    #[test]
    fn required_string_missing() {
        let v = serde_json::json!({});
        assert!(required_string(&v, "name").is_err());
    }

    #[test]
    fn optional_string_present() {
        let v = serde_json::json!({"name": "test"});
        assert_eq!(optional_string(&v, "name"), Some("test"));
    }

    #[test]
    fn optional_string_missing() {
        let v = serde_json::json!({});
        assert_eq!(optional_string(&v, "name"), None);
    }

    #[test]
    fn optional_string_wrong_type() {
        let v = serde_json::json!({"name": 123});
        assert_eq!(optional_string(&v, "name"), None);
    }
}
