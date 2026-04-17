//! Orchestra Files — File Parsing and I/O
//!
//! Re-exports parser types and functions from the parsers module.
//! Provides:
//! * Parse cache with 50-entry limit
//! * File I/O utilities
//! * Manifest status type

// Parser modules
pub mod parsers;

// Re-export commonly used parser types and functions
pub use parsers::{
    // Common utilities
    common::{
        extract_all_sections, extract_bold_field, extract_section, parse_bullets,
        parse_frontmatter_map, split_frontmatter,
    },
    // Overrides types
    overrides::{
        extract_uat_type, format_overrides_section, parse_context_depends_on, parse_overrides,
        Override, OverrideScope, UatType,
    },
    // Plan types
    plan::{parse_plan, SlicePlan, TaskPlanEntry},
    // Continue types
    r#continue::{format_continue, parse_continue, Continue, ContinueFrontmatter},
    // Requirements types
    requirements::{
        count_must_haves_mentioned_in_summary, parse_requirement_counts,
        parse_task_plan_must_haves, MustHaveItem, RequirementCounts,
    },
    // Roadmap types
    roadmap::{parse_roadmap, BoundaryMapEntry, Roadmap, RoadmapSlice},
    // Secrets types
    secrets::{
        format_secrets_manifest, parse_secrets_manifest, SecretsManifest, SecretsManifestEntry,
        VALID_STATUSES,
    },
    // Summary types
    summary::{parse_summary, FileModified, RequiresEntry, Summary, SummaryFrontmatter},
};

use crate::atomic_write;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

// ─── Constants ───────────────────────────────────────────────────────────────────

/// Maximum parse cache entries
#[allow(dead_code)] // Kept for future use
const CACHE_MAX: usize = 50;

// ─── Types ──────────────────────────────────────────────────────────────────────
// Note: All parser types are now re-exported from the parsers module.

/// Manifest status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifestStatus {
    pub pending: Vec<String>,
    pub collected: Vec<String>,
    pub skipped: Vec<String>,
    pub existing: Vec<String>,
}

// ─── Parse Cache ────────────────────────────────────────────────────────────────

/// Global parse cache
static PARSE_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// Get parse cache
fn get_parse_cache() -> &'static Mutex<HashMap<String, String>> {
    PARSE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Generate cache key from content
#[allow(dead_code)] // Kept for future use
fn cache_key(content: &str) -> String {
    let len = content.len();
    let head = if content.len() > 100 {
        content.chars().take(97).collect::<String>()
    } else {
        content.to_string()
    };
    let mid_start = if len > 200 {
        (len / 2).saturating_sub(50)
    } else {
        0
    };
    let mid = if len > 200 {
        &content[mid_start..(mid_start + 100).min(len)]
    } else {
        ""
    };
    let tail = if len > 100 {
        &content[len.saturating_sub(100)..]
    } else {
        ""
    };
    format!("{}:{}:{}:{}", len, head, mid, tail)
}

/// Cached parse wrapper
#[allow(dead_code)] // Kept for future use
fn cached_parse<T, F>(content: &str, tag: &str, parse_fn: F) -> T
where
    T: for<'de> serde::Deserialize<'de> + serde::Serialize,
    F: FnOnce(&str) -> T,
{
    let key = format!("{}|{}", tag, cache_key(content));
    let mut cache = get_parse_cache().lock().unwrap_or_else(|e| e.into_inner());

    if let Some(serialized) = cache.get(&key) {
        if let Ok(result) = serde_json::from_str::<T>(serialized) {
            return result;
        }
    }

    if cache.len() >= CACHE_MAX {
        cache.clear();
    }

    let result = parse_fn(content);
    if let Ok(serialized) = serde_json::to_string(&result) {
        cache.insert(key, serialized);
    }

    result
}

/// Clear the parse cache
pub fn clear_parse_cache() {
    let mut cache = get_parse_cache().lock().unwrap_or_else(|e| e.into_inner());
    cache.clear();
}

// ─── File I/O ──────────────────────────────────────────────────────────────────

/// Load file from disk
pub fn load_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Save content to file atomically
pub fn save_file(path: &Path, content: &str) -> anyhow::Result<()> {
    atomic_write::atomic_write(path, content)
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_section() {
        let content = "# Title\n\n## Section One\nContent here\n\n## Section Two\nMore content";
        let section = extract_section(content, "Section One", 2).unwrap();
        assert_eq!(section, "Content here");
    }

    #[test]
    fn test_extract_section_not_found() {
        let content = "# Title\n\n## Section One\nContent here";
        let section = extract_section(content, "Missing", 2);
        assert!(section.is_none());
    }

    #[test]
    fn test_extract_bold_field() {
        let content = "**Key:** Value\n\nOther content";
        let value = extract_bold_field(content, "Key").unwrap();
        assert_eq!(value, "Value");
    }

    #[test]
    fn test_parse_bullets() {
        let content = "- First item\n- Second item\n- Third item";
        let bullets = parse_bullets(content);
        assert_eq!(bullets.len(), 3);
        assert_eq!(bullets[0], "First item");
    }

    #[test]
    fn test_split_frontmatter() {
        let content = "---\nkey: value\n---\n\nBody content";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_some());
        assert_eq!(body, "Body content");
    }

    #[test]
    fn test_split_frontmatter_none() {
        let content = "No frontmatter\n\nJust body";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_clear_parse_cache() {
        clear_parse_cache();
        // Can't easily test the internal state without exposing it
    }
}
