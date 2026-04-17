//! Orchestra Captures — Fire-and-forget thought capture with triage classification
//!
//! Append-only capture file at `.orchestra/CAPTURES.md`. Each capture is an H3 section
//! with bold metadata fields, parseable by regex patterns.
//!
//! Worktree-aware: captures always resolve to the original project root's
//! `.orchestra/CAPTURES.md`, not the worktree's local `.orchestra/`.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

use crate::paths::orchestra_root;

// ─── Types ──────────────────────────────────────────────────────────────────────

/// Capture classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Classification {
    QuickTask,
    Inject,
    Defer,
    Replan,
    Note,
}

impl Classification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Classification::QuickTask => "quick-task",
            Classification::Inject => "inject",
            Classification::Defer => "defer",
            Classification::Replan => "replan",
            Classification::Note => "note",
        }
    }

    pub fn parse_name(s: &str) -> Option<Self> {
        match s {
            "quick-task" => Some(Classification::QuickTask),
            "inject" => Some(Classification::Inject),
            "defer" => Some(Classification::Defer),
            "replan" => Some(Classification::Replan),
            "note" => Some(Classification::Note),
            _ => None,
        }
    }
}

/// Capture entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureEntry {
    pub id: String,
    pub text: String,
    pub timestamp: String,
    pub status: CaptureStatus,
    pub classification: Option<Classification>,
    pub resolution: Option<String>,
    pub rationale: Option<String>,
    pub resolved_at: Option<String>,
    pub executed: bool,
}

/// Capture status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum CaptureStatus {
    Pending,
    Triaged,
    Resolved,
}

/// Triage result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageResult {
    pub capture_id: String,
    pub classification: Classification,
    pub rationale: String,
    pub affected_files: Option<Vec<String>>,
    pub target_slice: Option<String>,
}

// ─── Constants ───────────────────────────────────────────────────────────────────

const CAPTURES_FILENAME: &str = "CAPTURES.md";

/// Regex to match bold field lines like "**Key:** Value"
static BOLD_FIELD_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\*\*([^:]+):\*\*\s*(.+)$").unwrap());

/// Regex to match status lines
static STATUS_PENDING_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\*\*Status:\*\*\s*pending").unwrap());

// ─── Path Resolution ─────────────────────────────────────────────────────────────

/// Resolve the path to CAPTURES.md, aware of worktree context
///
/// In worktree-isolated mode, basePath is `.orchestra/worktrees/<MID>/`.
/// Captures must resolve to the *original* project root's `.orchestra/CAPTURES.md`,
/// not the worktree-local `.orchestra/`.
///
/// Detection: if basePath contains `/.orchestra/worktrees/`, walk up to the
/// directory that contains `.orchestra/worktrees/` — that's the project root.
///
/// # Arguments
/// * `base_path` - Base path (may be in worktree)
///
/// # Returns
/// Full path to CAPTURES.md
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
///
/// let path = resolve_captures_path("/project/.orchestra/worktrees/M01");
/// assert!(path.contains("CAPTURES.md"));
/// ```
pub fn resolve_captures_path(base_path: &Path) -> String {
    let base_path_str = base_path.to_string_lossy();

    // Check if we're in a worktree
    if let Some(worktree_idx) = base_path_str.find("/.orchestra/worktrees/") {
        // Walk up to project root
        let project_root = &base_path_str[..worktree_idx];
        return format!("{}/.orchestra/{}", project_root, CAPTURES_FILENAME);
    }

    // Not in worktree - use normal orchestra root
    format!(
        "{}/{}",
        orchestra_root(base_path).display(),
        CAPTURES_FILENAME
    )
}

// ─── File I/O ───────────────────────────────────────────────────────────────────

/// Append a new capture entry to CAPTURES.md
///
/// Creates `.orchestra/` and the file if they don't exist.
/// Returns the generated capture ID.
///
/// # Arguments
/// * `base_path` - Base path (may be in worktree)
/// * `text` - Capture text
///
/// # Returns
/// Generated capture ID (format: CAP-XXXXXXXX)
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
///
/// let id = append_capture(Path::new("/project"), "Fix the bug");
/// assert!(id.starts_with("CAP-"));
/// ```
pub fn append_capture(base_path: &Path, text: &str) -> Result<String, String> {
    let file_path = resolve_captures_path(base_path);
    let file_path_obj = Path::new(&file_path);

    // Create .orchestra directory if it doesn't exist
    if let Some(parent) = file_path_obj.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
        }
    }

    // Generate capture ID and timestamp
    let uuid_str = Uuid::new_v4().simple().to_string();
    let id = format!(
        "CAP-{}",
        uuid_str.chars().take(8).collect::<String>().to_uppercase()
    );
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Build entry
    let entry = format!(
        "### {}\n**Text:** {}\n**Captured:** {}\n**Status:** pending\n\n",
        id, text, timestamp
    );

    // Write to file
    if file_path_obj.exists() {
        let existing = fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read captures file: {}", e))?;
        let trimmed = existing.trim_end();
        fs::write(&file_path, format!("{}\n\n{}", trimmed, entry))
            .map_err(|e| format!("Failed to write captures file: {}", e))?;
    } else {
        let header = "# Captures\n\n";
        fs::write(&file_path, format!("{}{}", header, entry))
            .map_err(|e| format!("Failed to write captures file: {}", e))?;
    }

    Ok(id)
}

/// Parse all capture entries from CAPTURES.md
///
/// Returns entries in file order (oldest first).
///
/// # Arguments
/// * `base_path` - Base path (may be in worktree)
///
/// # Returns
/// Vector of capture entries
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
///
/// let captures = load_all_captures(Path::new("/project"));
/// ```
pub fn load_all_captures(base_path: &Path) -> Vec<CaptureEntry> {
    let file_path = resolve_captures_path(base_path);
    let file_path_obj = Path::new(&file_path);

    if !file_path_obj.exists() {
        return Vec::new();
    }

    let content = match fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    parse_captures_content(&content)
}

/// Load only pending (unresolved) captures
///
/// # Arguments
/// * `base_path` - Base path (may be in worktree)
///
/// # Returns
/// Vector of pending capture entries
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
///
/// let pending = load_pending_captures(Path::new("/project"));
/// ```
pub fn load_pending_captures(base_path: &Path) -> Vec<CaptureEntry> {
    load_all_captures(base_path)
        .into_iter()
        .filter(|c| c.status == CaptureStatus::Pending)
        .collect()
}

/// Fast check for pending captures without full parse
///
/// Reads the file and scans for `**Status:** pending` via regex.
/// Returns false if the file doesn't exist.
///
/// # Arguments
/// * `base_path` - Base path (may be in worktree)
///
/// # Returns
/// true if pending captures exist
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
///
/// if has_pending_captures(Path::new("/project")) {
///     println!("There are pending captures");
/// }
/// ```
pub fn has_pending_captures(base_path: &Path) -> bool {
    let file_path = resolve_captures_path(base_path);
    let file_path_obj = Path::new(&file_path);

    if !file_path_obj.exists() {
        return false;
    }

    match fs::read_to_string(&file_path) {
        Ok(content) => STATUS_PENDING_RE.is_match(&content),
        Err(_) => false,
    }
}

/// Count pending captures without full parse — single file read
///
/// Uses regex to count `**Status:** pending` occurrences.
/// Returns 0 if file doesn't exist or on error.
///
/// # Arguments
/// * `base_path` - Base path (may be in worktree)
///
/// # Returns
/// Number of pending captures
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
///
/// let count = count_pending_captures(Path::new("/project"));
/// println!("{} pending captures", count);
/// ```
pub fn count_pending_captures(base_path: &Path) -> usize {
    let file_path = resolve_captures_path(base_path);
    let file_path_obj = Path::new(&file_path);

    if !file_path_obj.exists() {
        return 0;
    }

    match fs::read_to_string(&file_path) {
        Ok(content) => STATUS_PENDING_RE.find_iter(&content).count(),
        Err(_) => 0,
    }
}

/// Mark a capture as resolved with classification and rationale
///
/// Rewrites the entry in place, preserving other entries.
///
/// # Arguments
/// * `base_path` - Base path (may be in worktree)
/// * `capture_id` - Capture ID to mark as resolved
/// * `classification` - Triage classification
/// * `resolution` - Resolution description
/// * `rationale` - Triage rationale
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
/// use rustycode_orchestra::captures::Classification;
///
/// mark_capture_resolved(
///     Path::new("/project"),
///     "CAP-12345678",
///     Classification::QuickTask,
///     "Will implement feature",
///     "Low complexity, quick win"
/// );
/// ```
pub fn mark_capture_resolved(
    base_path: &Path,
    capture_id: &str,
    classification: Classification,
    resolution: &str,
    rationale: &str,
) -> Result<(), String> {
    let file_path = resolve_captures_path(base_path);
    let file_path_obj = Path::new(&file_path);

    if !file_path_obj.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read captures file: {}", e))?;

    let resolved_at = chrono::Utc::now().to_rfc3339();

    // Split content by sections and rebuild with updated capture
    let sections: Vec<&str> = content.split("\n### ").collect();

    let mut found = false;
    let mut updated_sections = Vec::new();

    for (i, section) in sections.iter().enumerate() {
        if i == 0 {
            // First section is the header ("# Captures\n\n")
            updated_sections.push(section.to_string());
            continue;
        }

        // Check if this section starts with our target ID
        if section.starts_with(&format!("{}\n", capture_id)) {
            found = true;

            // Parse the section content (first line is the ID without "### " prefix)
            let lines: Vec<&str> = section.lines().collect();
            let mut cleaned_lines = Vec::new();

            // Add back the "### " prefix to the ID line
            cleaned_lines.push(format!("### {}", lines[0]));

            // Process field lines
            for line in &lines[1..] {
                // Skip existing classification/resolution/rationale/resolved fields
                if line.starts_with("**Classification:**")
                    || line.starts_with("**Resolution:**")
                    || line.starts_with("**Rationale:**")
                    || line.starts_with("**Resolved:**")
                {
                    continue;
                }

                // Update status line
                if line.starts_with("**Status:**") {
                    cleaned_lines.push("**Status:** resolved".to_string());
                } else {
                    cleaned_lines.push(line.to_string());
                }
            }

            // Add new fields
            cleaned_lines.push(format!("**Classification:** {}", classification.as_str()));
            cleaned_lines.push(format!("**Resolution:** {}", resolution));
            cleaned_lines.push(format!("**Rationale:** {}", rationale));
            cleaned_lines.push(format!("**Resolved:** {}", resolved_at));

            updated_sections.push(cleaned_lines.join("\n"));
        } else {
            // Keep other sections as-is, adding back the "### " prefix
            updated_sections.push(format!("### {}", section));
        }
    }

    if !found {
        return Ok(()); // Capture not found, nothing to do
    }

    let updated_content = updated_sections.join("\n");

    fs::write(&file_path, updated_content)
        .map_err(|e| format!("Failed to write captures file: {}", e))?;

    Ok(())
}

/// Mark a resolved capture as executed
///
/// Appends `**Executed:** <timestamp>` to the capture's section in CAPTURES.md.
///
/// # Arguments
/// * `base_path` - Base path (may be in worktree)
/// * `capture_id` - Capture ID to mark as executed
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
///
/// mark_capture_executed(Path::new("/project"), "CAP-12345678");
/// ```
pub fn mark_capture_executed(base_path: &Path, capture_id: &str) -> Result<(), String> {
    let file_path = resolve_captures_path(base_path);
    let file_path_obj = Path::new(&file_path);

    if !file_path_obj.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read captures file: {}", e))?;

    let executed_at = chrono::Utc::now().to_rfc3339();

    // Split content by sections and rebuild with updated capture
    let sections: Vec<&str> = content.split("\n### ").collect();

    let mut found = false;
    let mut updated_sections = Vec::new();

    for (i, section) in sections.iter().enumerate() {
        if i == 0 {
            // First section is the header ("# Captures\n\n")
            updated_sections.push(section.to_string());
            continue;
        }

        // Check if this section starts with our target ID
        if section.starts_with(&format!("{}\n", capture_id)) {
            found = true;

            // Parse the section content (first line is the ID without "### " prefix)
            let lines: Vec<&str> = section.lines().collect();
            let mut cleaned_lines = Vec::new();

            // Add back the "### " prefix to the ID line
            cleaned_lines.push(format!("### {}", lines[0]));

            // Process field lines
            for line in &lines[1..] {
                // Skip existing executed field
                if line.starts_with("**Executed:**") {
                    continue;
                } else {
                    cleaned_lines.push(line.to_string());
                }
            }

            // Add executed field
            cleaned_lines.push(format!("**Executed:** {}", executed_at));

            updated_sections.push(cleaned_lines.join("\n"));
        } else {
            // Keep other sections as-is, adding back the "### " prefix
            updated_sections.push(format!("### {}", section));
        }
    }

    if !found {
        return Ok(()); // Capture not found, nothing to do
    }

    let updated_content = updated_sections.join("\n");

    fs::write(&file_path, updated_content)
        .map_err(|e| format!("Failed to write captures file: {}", e))?;

    Ok(())
}

/// Load resolved captures that have actionable classifications
///
/// Returns captures with classifications: inject, replan, quick-task
/// that have NOT yet been executed.
///
/// # Arguments
/// * `base_path` - Base path (may be in worktree)
///
/// # Returns
/// Vector of actionable capture entries
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
///
/// let actionable = load_actionable_captures(Path::new("/project"));
/// for capture in actionable {
///     println!("Actionable: {}", capture.id);
/// }
/// ```
pub fn load_actionable_captures(base_path: &Path) -> Vec<CaptureEntry> {
    load_all_captures(base_path)
        .into_iter()
        .filter(|c| {
            c.status == CaptureStatus::Resolved
                && !c.executed
                && c.classification.as_ref().is_some_and(|cls| {
                    matches!(
                        cls,
                        Classification::Inject | Classification::Replan | Classification::QuickTask
                    )
                })
        })
        .collect()
}

// ─── Parser ─────────────────────────────────────────────────────────────────────

/// Parse CAPTURES.md content into CaptureEntry array
fn parse_captures_content(content: &str) -> Vec<CaptureEntry> {
    let mut entries = Vec::new();

    // Split on H3 headings (###)
    let sections: Vec<&str> = content
        .split("### ")
        .skip(1) // Skip content before first H3
        .collect();

    for section in sections {
        let lines: Vec<&str> = section.lines().collect();
        if lines.is_empty() {
            continue;
        }

        let id = lines[0].trim();
        if id.is_empty() {
            continue;
        }

        let body = lines[1..].join("\n");
        let text = extract_bold_field(&body, "Text");
        let timestamp = extract_bold_field(&body, "Captured");
        let status_raw = extract_bold_field(&body, "Status");
        let classification = extract_bold_field(&body, "Classification")
            .and_then(|c| Classification::parse_name(&c));
        let resolution = extract_bold_field(&body, "Resolution");
        let rationale = extract_bold_field(&body, "Rationale");
        let resolved_at = extract_bold_field(&body, "Resolved");
        let executed_at = extract_bold_field(&body, "Executed");

        if text.is_none() || timestamp.is_none() {
            continue;
        }

        let status = match status_raw.as_deref() {
            Some("resolved") | Some("triaged") => {
                if status_raw == Some("resolved".to_string()) {
                    CaptureStatus::Resolved
                } else {
                    CaptureStatus::Triaged
                }
            }
            _ => CaptureStatus::Pending,
        };

        entries.push(CaptureEntry {
            id: id.to_string(),
            text: text.unwrap(),
            timestamp: timestamp.unwrap(),
            status,
            classification,
            resolution,
            rationale,
            resolved_at,
            executed: executed_at.is_some(),
        });
    }

    entries
}

/// Extract value from a bold-prefixed line like "**Key:** Value"
fn extract_bold_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(caps) = BOLD_FIELD_RE.captures(line) {
            if caps.len() >= 3 && caps[1].trim() == key {
                return Some(caps[2].trim().to_string());
            }
        }
    }
    None
}

// ─── Triage Output Parser ─────────────────────────────────────────────────────────

/// Parse LLM triage output into TriageResult array
///
/// Handles:
/// - Clean JSON array
/// - JSON wrapped in fenced code block (```json ... ```)
/// - JSON with leading/trailing prose
/// - Single object (not array) — wraps in array
/// - Malformed JSON — returns empty array (caller should fall back to note)
/// - Partial results — valid entries are kept, invalid skipped
///
/// # Arguments
/// * `llm_response` - LLM response text
///
/// # Returns
/// Vector of triage results
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::captures::*;
///
/// let response = r#"[{"captureId":"CAP-123","classification":"quick-task","rationale":"Quick fix"}]"#;
/// let results = parse_triage_output(response);
/// assert_eq!(results.len(), 1);
/// ```
pub fn parse_triage_output(llm_response: &str) -> Vec<TriageResult> {
    if llm_response.trim().is_empty() {
        return Vec::new();
    }

    // Try to extract JSON from fenced code blocks first
    let json_str = if let Some(caps) = Regex::new(r"```(?:json)?\s*\n?([\s\S]*?)\n?\s*```")
        .ok()
        .and_then(|re| re.captures(llm_response))
    {
        Some(caps[1].trim().to_string())
    } else {
        extract_json_substring(llm_response)
    };

    let json_str = match json_str {
        Some(s) => s,
        None => return Vec::new(),
    };

    // Parse JSON
    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    // Convert to array if single object
    let arr = if parsed.is_array() {
        parsed.as_array().unwrap().clone()
    } else {
        vec![parsed]
    };

    // Filter and normalize
    arr.into_iter()
        .filter_map(|v| {
            if is_valid_triage_result(&v) {
                Some(normalize_triage_result(v))
            } else {
                None
            }
        })
        .collect()
}

/// Try to find a JSON array or object substring in prose text
fn extract_json_substring(text: &str) -> Option<String> {
    let arr_start = text.find('[');
    let obj_start = text.find('{');

    let (start, open_char, close_char) = match (arr_start, obj_start) {
        (None, None) => return None,
        (Some(a), None) => (a, '[', ']'),
        (None, Some(o)) => (o, '{', '}'),
        (Some(a), Some(o)) => {
            if a < o {
                (a, '[', ']')
            } else {
                (o, '{', '}')
            }
        }
    };

    // Find matching bracket
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for (i, ch) in text[start..].char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == open_char {
            depth += 1;
        }
        if ch == close_char {
            depth -= 1;
            if depth == 0 {
                return Some(text[start..=(start + i)].to_string());
            }
        }
    }

    None
}

fn is_valid_triage_result(obj: &serde_json::Value) -> bool {
    if let Some(o) = obj.as_object() {
        let has_capture_id = o.get("captureId").and_then(|v| v.as_str()).is_some();

        let has_classification = o
            .get("classification")
            .and_then(|v| v.as_str())
            .and_then(Classification::parse_name)
            .is_some();

        let has_rationale = o.get("rationale").and_then(|v| v.as_str()).is_some();

        has_capture_id && has_classification && has_rationale
    } else {
        false
    }
}

fn normalize_triage_result(obj: serde_json::Value) -> TriageResult {
    let o = match obj.as_object() {
        Some(o) => o,
        None => {
            return TriageResult {
                capture_id: String::new(),
                classification: Classification::Note,
                rationale: "Invalid JSON: expected object".to_string(),
                affected_files: None,
                target_slice: None,
            };
        }
    };

    let capture_id = o
        .get("captureId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let classification = o
        .get("classification")
        .and_then(|v| v.as_str())
        .and_then(Classification::parse_name)
        .unwrap_or(Classification::Note);

    let rationale = o
        .get("rationale")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let affected_files = o
        .get("affectedFiles")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });

    let target_slice = o
        .get("targetSlice")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    TriageResult {
        capture_id,
        classification,
        rationale,
        affected_files,
        target_slice,
    }
}

// ─── Helper Functions ───────────────────────────────────────────────────────────

/// Remove a bold field from markdown text
#[allow(dead_code)] // Kept for future use
fn remove_bold_field(text: &str, key: &str) -> String {
    let re = Regex::new(&format!(r"(?m)^\*\*{}:\*\*\s*.+\n?", regex::escape(key))).unwrap();
    re.replace_all(text, "").to_string()
}

/// Update a bold field in markdown text
#[allow(dead_code)] // Kept for future use
fn update_bold_field(text: &str, key: &str, value: &str) -> String {
    let re = Regex::new(&format!(r"(?m)^\*\*{}:\*\*\s*.+$", regex::escape(key))).unwrap();
    if re.is_match(text) {
        re.replace_all(text, &format!("**{}:** {}", key, value))
            .to_string()
    } else {
        text.to_string()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_classification_as_str() {
        assert_eq!(Classification::QuickTask.as_str(), "quick-task");
        assert_eq!(Classification::Inject.as_str(), "inject");
        assert_eq!(Classification::Defer.as_str(), "defer");
        assert_eq!(Classification::Replan.as_str(), "replan");
        assert_eq!(Classification::Note.as_str(), "note");
    }

    #[test]
    fn test_classification_from_str() {
        assert_eq!(
            Classification::parse_name("quick-task"),
            Some(Classification::QuickTask)
        );
        assert_eq!(
            Classification::parse_name("inject"),
            Some(Classification::Inject)
        );
        assert_eq!(
            Classification::parse_name("defer"),
            Some(Classification::Defer)
        );
        assert_eq!(
            Classification::parse_name("replan"),
            Some(Classification::Replan)
        );
        assert_eq!(
            Classification::parse_name("note"),
            Some(Classification::Note)
        );
        assert_eq!(Classification::parse_name("invalid"), None);
    }

    #[test]
    fn test_extract_bold_field() {
        let text = "**Text:** Test capture\n**Status:** pending\n";
        assert_eq!(
            extract_bold_field(text, "Text"),
            Some("Test capture".to_string())
        );
        assert_eq!(
            extract_bold_field(text, "Status"),
            Some("pending".to_string())
        );
        assert_eq!(extract_bold_field(text, "Missing"), None);
    }

    #[test]
    fn test_parse_captures_content() {
        let content = r#"# Captures

### CAP-12345678
**Text:** Test capture
**Captured:** 2024-01-01T00:00:00Z
**Status:** pending

### CAP-87654321
**Text:** Another capture
**Captured:** 2024-01-02T00:00:00Z
**Status:** resolved
**Classification:** quick-task
"#;

        let entries = parse_captures_content(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "CAP-12345678");
        assert_eq!(entries[0].text, "Test capture");
        assert_eq!(entries[0].status, CaptureStatus::Pending);
        assert_eq!(entries[1].id, "CAP-87654321");
        assert_eq!(entries[1].status, CaptureStatus::Resolved);
        assert_eq!(entries[1].classification, Some(Classification::QuickTask));
    }

    #[test]
    fn test_parse_triage_output() {
        let response = r#"[
  {"captureId":"CAP-123","classification":"quick-task","rationale":"Quick fix"},
  {"captureId":"CAP-456","classification":"defer","rationale":"Complex issue"}
]"#;

        let results = parse_triage_output(response);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].capture_id, "CAP-123");
        assert_eq!(results[0].classification, Classification::QuickTask);
        assert_eq!(results[1].capture_id, "CAP-456");
        assert_eq!(results[1].classification, Classification::Defer);
    }

    #[test]
    fn test_parse_triage_output_with_fenced_code() {
        let response = r#"Here's the triage:

```json
[
  {"captureId":"CAP-123","classification":"quick-task","rationale":"Quick fix"}
]
```

That's it."#;

        let results = parse_triage_output(response);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].capture_id, "CAP-123");
    }

    #[test]
    fn test_append_and_load_captures() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Append a capture
        let id = append_capture(base_path, "Test capture").unwrap();
        assert!(id.starts_with("CAP-"));

        // Load all captures
        let captures = load_all_captures(base_path);
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].text, "Test capture");
        assert_eq!(captures[0].status, CaptureStatus::Pending);

        // Load pending captures
        let pending = load_pending_captures(base_path);
        assert_eq!(pending.len(), 1);

        // Check has pending
        assert!(has_pending_captures(base_path));

        // Count pending
        let count = count_pending_captures(base_path);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_mark_capture_resolved() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Append a capture
        let id = append_capture(base_path, "Test capture").unwrap();

        // Mark as resolved
        mark_capture_resolved(
            base_path,
            &id,
            Classification::QuickTask,
            "Will implement",
            "Quick fix",
        )
        .unwrap();

        // Load and check
        let captures = load_all_captures(base_path);
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].status, CaptureStatus::Resolved);
        assert_eq!(captures[0].classification, Some(Classification::QuickTask));
        assert_eq!(captures[0].resolution, Some("Will implement".to_string()));
        assert_eq!(captures[0].rationale, Some("Quick fix".to_string()));
    }

    #[test]
    fn test_mark_capture_executed() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Append and resolve a capture
        let id = append_capture(base_path, "Test capture").unwrap();
        mark_capture_resolved(
            base_path,
            &id,
            Classification::QuickTask,
            "Will implement",
            "Quick fix",
        )
        .unwrap();

        // Mark as executed
        mark_capture_executed(base_path, &id).unwrap();

        // Load and check
        let captures = load_all_captures(base_path);
        assert_eq!(captures.len(), 1);
        assert!(captures[0].executed);
    }

    #[test]
    fn test_load_actionable_captures() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Append multiple captures
        let id1 = append_capture(base_path, "Quick task").unwrap();
        let _id2 = append_capture(base_path, "Note").unwrap();

        // Mark one as resolved with actionable classification
        mark_capture_resolved(
            base_path,
            &id1,
            Classification::QuickTask,
            "Will do",
            "Quick",
        )
        .unwrap();

        // Load actionable captures
        let actionable = load_actionable_captures(base_path);
        assert_eq!(actionable.len(), 1);
        assert_eq!(actionable[0].id, id1);
    }

    #[test]
    fn test_resolve_captures_path_worktree() {
        let path = Path::new("/project/.orchestra/worktrees/M01");
        let resolved = resolve_captures_path(path);
        assert!(resolved.contains("/project/.orchestra/CAPTURES.md"));
    }

    #[test]
    fn test_resolve_captures_path_normal() {
        let path = Path::new("/project");
        let resolved = resolve_captures_path(path);
        assert!(resolved.contains(".orchestra/CAPTURES.md"));
    }
}
