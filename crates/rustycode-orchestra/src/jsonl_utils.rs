//! Shared JSONL parsing utilities
//!
//! Both forensics and session-forensics need to parse JSONL activity logs
//! with an upper byte limit to prevent OOM on bloated files. This module
//! provides the single canonical implementation and constant.

use serde_json::Value;

/// Max bytes to parse from a JSONL source. Prevents OOM on bloated activity logs.
pub const MAX_JSONL_BYTES: usize = 10 * 1024 * 1024; // 10 MB

/// Parse a raw JSONL string into a vector of parsed JSON values
///
/// If the input exceeds MAX_JSONL_BYTES, only the tail is parsed (most recent entries).
/// Each line is parsed as JSON; invalid lines are skipped.
///
/// # Arguments
/// * `raw` - Raw JSONL string (one JSON object per line)
///
/// # Returns
/// Vector of parsed JSON values
///
/// # Example
/// ```
/// use rustycode_orchestra::jsonl_utils::*;
///
/// let jsonl = r#"{"a":1}
/// {"b":2}
/// {"c":3}"#;
///
/// let parsed = parse_jsonl(jsonl);
/// assert_eq!(parsed.len(), 3);
/// ```
pub fn parse_jsonl(raw: &str) -> Vec<Value> {
    let source = if raw.len() > MAX_JSONL_BYTES {
        // Take the tail (most recent entries)
        &raw[raw.len() - MAX_JSONL_BYTES..]
    } else {
        raw
    };

    source
        .trim()
        .lines()
        .filter_map(|line| {
            if line.trim().is_empty() {
                return None;
            }
            serde_json::from_str(line).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_jsonl_basic() {
        let jsonl = r#"{"a":1}
{"b":2}
{"c":3}"#;

        let parsed = parse_jsonl(jsonl);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], json!({"a": 1}));
        assert_eq!(parsed[1], json!({"b": 2}));
        assert_eq!(parsed[2], json!({"c": 3}));
    }

    #[test]
    fn test_parse_jsonl_with_empty_lines() {
        let jsonl = r#"{"a":1}

{"b":2}

{"c":3}"#;

        let parsed = parse_jsonl(jsonl);
        assert_eq!(parsed.len(), 3);
    }

    #[test]
    fn test_parse_jsonl_with_invalid_lines() {
        let jsonl = r#"{"a":1}
invalid json
{"b":2}
also invalid
{"c":3}"#;

        let parsed = parse_jsonl(jsonl);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], json!({"a": 1}));
        assert_eq!(parsed[1], json!({"b": 2}));
        assert_eq!(parsed[2], json!({"c": 3}));
    }

    #[test]
    fn test_parse_jsonl_empty() {
        let jsonl = "";
        let parsed = parse_jsonl(jsonl);
        assert_eq!(parsed.len(), 0);
    }

    #[test]
    fn test_parse_jsonl_whitespace_only() {
        let jsonl = "   \n\n  \n   ";
        let parsed = parse_jsonl(jsonl);
        assert_eq!(parsed.len(), 0);
    }

    #[test]
    fn test_max_jsonl_bytes() {
        assert_eq!(MAX_JSONL_BYTES, 10 * 1024 * 1024);
    }

    #[test]
    fn test_parse_jsonl_truncation() {
        // Create a JSONL string larger than MAX_JSONL_BYTES
        let mut jsonl = String::new();
        for i in 0..1000 {
            jsonl.push_str(&format!("{{\"line\":{}}}\n", i));
        }

        // Add a prefix to push it over the limit
        let prefix = "x".repeat(MAX_JSONL_BYTES + 1000);
        let oversized = format!("{}\n{}", prefix, jsonl);

        // Should parse without panic
        let parsed = parse_jsonl(&oversized);

        // Should have parsed the tail (most recent entries)
        // Due to truncation in the middle of the prefix, we might get fewer entries
        // but it should not panic and should return valid JSON
        for value in &parsed {
            assert!(value.is_object());
        }
    }

    #[test]
    fn test_parse_jsonl_complex_json() {
        let jsonl = r#"{"name":"test","nested":{"value":42}}
{"array":[1,2,3]}
{"string":"value"}"#;

        let parsed = parse_jsonl(jsonl);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0]["nested"]["value"], 42);
        assert_eq!(parsed[1]["array"][0], 1);
        assert_eq!(parsed[1]["array"][1], 2);
        assert_eq!(parsed[1]["array"][2], 3);
        assert_eq!(parsed[2]["string"], "value");
    }
}
