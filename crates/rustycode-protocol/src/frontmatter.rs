use std::collections::HashMap;

// A minimal, pure-Rust frontmatter representation and parser.
// This module intentionally avoids any external YAML dependency and
// supports the subset of frontmatter needed by RustyCode: strings, booleans,
// numbers, arrays (inline and multi-line), and simple nesting via objects.

#[derive(Debug, Clone)]
pub enum FrontmatterValue {
    String(String),
    Bool(bool),
    Number(i64),
    Array(Vec<FrontmatterValue>),
    Object(FrontmatterMap),
}

pub type FrontmatterMap = HashMap<String, FrontmatterValue>;

/// Splits a content blob into an optional YAML frontmatter string.
/// The common pattern is:
/// ---
/// key: value
/// ---
pub fn split_frontmatter(content: &str) -> Option<String> {
    let mut lines = content.lines();
    // First line must be a delimiter
    let first = lines.next()?.trim();
    if first != "---" {
        return None;
    }
    let mut yaml_lines: Vec<String> = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            return Some(yaml_lines.join("\n"));
        } else {
            yaml_lines.push(line.to_string());
        }
    }
    None
}

/// Minimal frontmatter parser.
/// Supports:
/// - top-level key: value (string/bool/number)
/// - multi-line arrays with "- item" syntax
/// - inline arrays like ["a", "b"]
pub fn parse_frontmatter_map(yaml: &str) -> FrontmatterMap {
    let mut map: FrontmatterMap = FrontmatterMap::new();
    let mut current_key: Option<String> = None;
    for raw in yaml.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Array item continuation
        if let Some(stripped) = trimmed.strip_prefix("- ") {
            if let Some(ref key) = current_key {
                // Ensure an Array exists for this key
                map.entry(key.clone())
                    .or_insert(FrontmatterValue::Array(Vec::new()));
                if let Some(FrontmatterValue::Array(arr)) = map.get_mut(key) {
                    arr.push(parse_scalar(stripped.trim()));
                }
            }
            continue;
        }
        // Key: value
        if let Some(pos) = trimmed.find(':') {
            let key = trimmed[..pos].trim().to_string();
            let val = trimmed[pos + 1..].trim();
            // Empty value means array/map follows on next lines
            if val.is_empty() {
                current_key = Some(key);
                continue;
            }
            // Inline array
            if val.starts_with('[') && val.ends_with(']') {
                let inner = &val[1..val.len() - 1];
                let mut items: Vec<FrontmatterValue> = Vec::new();
                for part in inner.split(',') {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    items.push(parse_scalar(part));
                }
                map.insert(key.clone(), FrontmatterValue::Array(items));
                current_key = Some(key);
                continue;
            } else {
                let value = parse_scalar(val);
                map.insert(key.clone(), value);
                current_key = Some(key);
                continue;
            }
        }
        // Lines without a colon are ignored in this lightweight parser.
    }
    map
}

fn parse_scalar(token: &str) -> FrontmatterValue {
    let t = token.trim();
    if t.eq_ignore_ascii_case("true") {
        FrontmatterValue::Bool(true)
    } else if t.eq_ignore_ascii_case("false") {
        FrontmatterValue::Bool(false)
    } else if let Ok(n) = t.parse::<i64>() {
        FrontmatterValue::Number(n)
    } else {
        // Strip surrounding quotes if present
        let mut s = t.to_string();
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            s = s[1..s.len() - 1].to_string();
        }
        FrontmatterValue::String(s)
    }
}

/// Convenience helpers for consumers of FrontmatterValue.
pub fn as_string(v: &FrontmatterValue) -> Option<String> {
    if let FrontmatterValue::String(s) = v {
        Some(s.clone())
    } else {
        None
    }
}

pub fn as_bool(v: &FrontmatterValue) -> Option<bool> {
    if let FrontmatterValue::Bool(b) = v {
        Some(*b)
    } else {
        None
    }
}

pub fn as_array(v: &FrontmatterValue) -> Option<Vec<FrontmatterValue>> {
    if let FrontmatterValue::Array(arr) = v {
        Some(arr.clone())
    } else {
        None
    }
}
