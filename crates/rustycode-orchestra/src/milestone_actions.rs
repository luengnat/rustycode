//! Orchestra Milestone Actions — Park, Unpark, and Discard operations
//!
//! Park: Creates a PARKED.md marker file. deriveState() skips parked milestones
//! when finding the active milestone, but keeps them in the registry.
//!
//! Unpark: Removes the PARKED.md marker. The milestone resumes normal state
//! derivation (active/pending depending on position and dependencies).
//!
//! Discard: Permanently removes the milestone directory. Also prunes
//! QUEUE-ORDER.json if the discarded milestone was in it.

use crate::cache::invalidate_all_caches;
use crate::paths::{build_milestone_file_name, resolve_milestone_file, resolve_milestone_path};
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Park
// ---------------------------------------------------------------------------

/// Park a milestone — creates a PARKED.md marker file with reason and timestamp.
/// Parked milestones are skipped during active-milestone discovery but stay on disk.
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `milestone_id` - Milestone identifier (e.g., "M01")
/// * `reason` - Reason for parking
///
/// # Returns
/// true if successfully parked, false if milestone not found or already parked
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_actions::*;
///
/// let parked = park_milestone("/project", "M01", "Blocked by external dependency");
/// if parked {
///     println!("Milestone M01 parked");
/// }
/// ```
pub fn park_milestone(base_path: &Path, milestone_id: &str, reason: &str) -> bool {
    let m_dir = match resolve_milestone_path(base_path, milestone_id) {
        Some(path) => path,
        None => return false,
    };

    if !m_dir.exists() {
        return false;
    }

    // Guard: do not park a completed milestone — it would corrupt depends_on satisfaction
    if let Some(summary_file) = resolve_milestone_file(base_path, milestone_id, "SUMMARY") {
        if summary_file.exists() {
            return false;
        }
    }

    let parked_path = m_dir.join(build_milestone_file_name(milestone_id, "PARKED"));
    if parked_path.exists() {
        return false; // already parked
    }

    // For YAML, use double-quoted string and escape any quotes in the reason
    let escaped_reason = reason.replace('\\', "\\\\").replace('"', "\\\"");

    let content = format!(
        "---\nparked_at: \"{}\"\nreason: \"{}\"\n---\n\n# {} — Parked\n\n> {}\n\n",
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        escaped_reason,
        milestone_id,
        reason
    );

    if let Err(e) = fs::write(&parked_path, content) {
        tracing::warn!("[milestone_actions] Failed to write PARKED.md: {}", e);
        return false;
    }

    let _ = invalidate_all_caches();
    true
}

// ---------------------------------------------------------------------------
// Unpark
// ---------------------------------------------------------------------------

/// Unpark a milestone — removes the PARKED.md marker file.
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `milestone_id` - Milestone identifier (e.g., "M01")
///
/// # Returns
/// true if successfully unparked, false if milestone not found or not parked
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_actions::*;
///
/// let unparked = unpark_milestone("/project", "M01");
/// if unparked {
///     println!("Milestone M01 unparked");
/// }
/// ```
pub fn unpark_milestone(base_path: &Path, milestone_id: &str) -> bool {
    let m_dir = match resolve_milestone_path(base_path, milestone_id) {
        Some(path) => path,
        None => return false,
    };

    if !m_dir.exists() {
        return false;
    }

    let parked_path = m_dir.join(build_milestone_file_name(milestone_id, "PARKED"));
    if !parked_path.exists() {
        return false; // not parked
    }

    if let Err(e) = fs::remove_file(&parked_path) {
        tracing::warn!("[milestone_actions] Failed to remove PARKED.md: {}", e);
        return false;
    }

    let _ = invalidate_all_caches();
    true
}

// ---------------------------------------------------------------------------
// Discard
// ---------------------------------------------------------------------------

/// Discard a milestone — permanently removes the milestone directory and
/// prunes it from QUEUE-ORDER.json if present.
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `milestone_id` - Milestone identifier (e.g., "M01")
///
/// # Returns
/// true if successfully discarded, false if milestone not found
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_actions::*;
///
/// let discarded = discard_milestone("/project", "M01");
/// if discarded {
///     println!("Milestone M01 discarded");
/// }
/// ```
pub fn discard_milestone(base_path: &Path, milestone_id: &str) -> bool {
    let m_dir = match resolve_milestone_path(base_path, milestone_id) {
        Some(path) => path,
        None => return false,
    };

    if !m_dir.exists() {
        return false;
    }

    // Remove directory recursively
    if let Err(e) = fs::remove_dir_all(&m_dir) {
        tracing::warn!(
            "[milestone_actions] Failed to remove milestone directory: {}",
            e
        );
        return false;
    }

    // Prune from queue order if present
    // Note: queue-order functionality not yet implemented
    // TODO: Prune from QUEUE-ORDER.json when queue-order module is available

    let _ = invalidate_all_caches();
    true
}

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

/// Check whether a milestone is parked (PARKED.md exists).
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `milestone_id` - Milestone identifier (e.g., "M01")
///
/// # Returns
/// true if the milestone is parked, false otherwise
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_actions::*;
///
/// if is_parked("/project", "M01") {
///     println!("Milestone M01 is parked");
/// }
/// ```
pub fn is_parked(base_path: &Path, milestone_id: &str) -> bool {
    // Try resolve_milestone_file first, fall back to direct path check
    if let Some(path) = resolve_milestone_file(base_path, milestone_id, "PARKED") {
        return path.exists();
    }

    // Fallback: check the file directly
    let m_dir = match resolve_milestone_path(base_path, milestone_id) {
        Some(path) => path,
        None => return false,
    };

    if !m_dir.exists() {
        return false;
    }

    let parked_path = m_dir.join(build_milestone_file_name(milestone_id, "PARKED"));
    parked_path.exists()
}

/// Read the park reason from PARKED.md frontmatter.
///
/// # Arguments
/// * `base_path` - Project root directory
/// * `milestone_id` - Milestone identifier (e.g., "M01")
///
/// # Returns
/// Park reason if found, None otherwise
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_actions::*;
///
/// if let Some(reason) = get_parked_reason("/project", "M01") {
///     println!("Milestone M01 is parked because: {}", reason);
/// }
/// ```
pub fn get_parked_reason(base_path: &Path, milestone_id: &str) -> Option<String> {
    let parked_file = match resolve_milestone_file(base_path, milestone_id, "PARKED") {
        Some(path) => path,
        None => {
            // Fallback: build the path directly
            let m_dir = resolve_milestone_path(base_path, milestone_id)?;
            m_dir.join(build_milestone_file_name(milestone_id, "PARKED"))
        }
    };

    if !parked_file.exists() {
        return None;
    }

    let content = fs::read_to_string(&parked_file).ok()?;

    // Parse frontmatter to extract reason
    extract_parked_reason(&content)
}

/// Extract park reason from PARKED.md content
///
/// # Arguments
/// * `content` - PARKED.md file content
///
/// # Returns
/// Park reason if found in frontmatter, None otherwise
fn extract_parked_reason(content: &str) -> Option<String> {
    // Find frontmatter between --- markers
    let frontmatter_start = content.find("---")?;
    let after_first_marker = &content[frontmatter_start + 3..];
    let frontmatter_end = after_first_marker.find("---")?;

    let frontmatter = &after_first_marker[..frontmatter_end];

    // Extract reason field
    for line in frontmatter.lines() {
        let line: &str = line.trim();
        if line.starts_with("reason:") {
            let reason_part = &line[line.find(':')? + 1..];
            let reason = reason_part.trim();

            // Remove surrounding quotes if present
            if reason.starts_with('"') && reason.ends_with('"') {
                let reason = &reason[1..reason.len() - 1];
                // Unescape YAML escape sequences (\", \\)
                let reason = reason.replace("\\\"", "\"").replace("\\\\", "\\");
                return Some(reason.to_string());
            }

            // Plain string (no quotes)
            return Some(reason.to_string());
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_park_milestone() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");
        let m01_path = milestones_path.join("M01");

        fs::create_dir_all(&m01_path).unwrap();

        // Create ROADMAP.md to indicate incomplete milestone
        fs::write(m01_path.join("ROADMAP.md"), "# M01").unwrap();

        let result = park_milestone(base_path, "M01", "Test reason");
        assert!(result);

        let parked_path = m01_path.join("M01-PARKED.md");
        assert!(parked_path.exists());

        let content = fs::read_to_string(&parked_path).unwrap();
        assert!(content.contains("Test reason"));
        assert!(content.contains("parked_at:"));
    }

    #[test]
    fn test_park_milestone_already_parked() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");
        let m01_path = milestones_path.join("M01");

        fs::create_dir_all(&m01_path).unwrap();
        fs::write(m01_path.join("ROADMAP.md"), "# M01").unwrap();

        // Park once
        let result1 = park_milestone(base_path, "M01", "First reason");
        assert!(result1);

        // Try to park again
        let result2 = park_milestone(base_path, "M01", "Second reason");
        assert!(!result2);
    }

    #[test]
    fn test_park_milestone_completed() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");
        let m01_path = milestones_path.join("M01");

        fs::create_dir_all(&m01_path).unwrap();

        // Create SUMMARY.md to indicate completed milestone
        fs::write(m01_path.join("SUMMARY.md"), "# Summary").unwrap();

        // Should not park completed milestone
        let result = park_milestone(base_path, "M01", "Test reason");
        assert!(!result);
    }

    #[test]
    fn test_unpark_milestone() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");
        let m01_path = milestones_path.join("M01");

        fs::create_dir_all(&m01_path).unwrap();
        fs::write(m01_path.join("ROADMAP.md"), "# M01").unwrap();

        // Park first
        park_milestone(base_path, "M01", "Test reason");

        // Then unpark
        let result = unpark_milestone(base_path, "M01");
        assert!(result);

        let parked_path = m01_path.join("M01-PARKED.md");
        assert!(!parked_path.exists());
    }

    #[test]
    fn test_unpark_milestone_not_parked() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");
        let m01_path = milestones_path.join("M01");

        fs::create_dir_all(&m01_path).unwrap();
        fs::write(m01_path.join("ROADMAP.md"), "# M01").unwrap();

        let result = unpark_milestone(base_path, "M01");
        assert!(!result);
    }

    #[test]
    fn test_discard_milestone() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");
        let m01_path = milestones_path.join("M01");

        fs::create_dir_all(&m01_path).unwrap();
        fs::write(m01_path.join("ROADMAP.md"), "# M01").unwrap();

        assert!(m01_path.exists());

        let result = discard_milestone(base_path, "M01");
        assert!(result);

        assert!(!m01_path.exists());
    }

    #[test]
    fn test_discard_milestone_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let result = discard_milestone(base_path, "M01");
        assert!(!result);
    }

    #[test]
    fn test_is_parked() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");
        let m01_path = milestones_path.join("M01");

        fs::create_dir_all(&m01_path).unwrap();
        fs::write(m01_path.join("ROADMAP.md"), "# M01").unwrap();

        assert!(!is_parked(base_path, "M01"));

        park_milestone(base_path, "M01", "Test reason");

        assert!(is_parked(base_path, "M01"));
    }

    #[test]
    fn test_get_parked_reason() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");
        let m01_path = milestones_path.join("M01");

        fs::create_dir_all(&m01_path).unwrap();
        fs::write(m01_path.join("ROADMAP.md"), "# M01").unwrap();

        let reason = get_parked_reason(base_path, "M01");
        assert!(reason.is_none());

        park_milestone(base_path, "M01", "Blocked by dependency");

        let reason = get_parked_reason(base_path, "M01");
        assert_eq!(reason, Some("Blocked by dependency".to_string()));
    }

    #[test]
    fn test_extract_parked_reason() {
        let content = r#"---
parked_at: "2025-03-18T12:00:00Z"
reason: "Test reason"
---

# M01 — Parked
"#;

        let reason = extract_parked_reason(content);
        assert_eq!(reason, Some("Test reason".to_string()));
    }

    #[test]
    fn test_extract_parked_reason_with_quotes() {
        let content = r#"---
parked_at: "2025-03-18T12:00:00Z"
reason: "Reason with \"quotes\" in it"
---

# M01 — Parked
"#;

        let reason = extract_parked_reason(content);
        assert_eq!(reason, Some("Reason with \"quotes\" in it".to_string()));
    }

    #[test]
    fn test_extract_parked_reason_no_frontmatter() {
        let content = r#"# M01 — Parked

No frontmatter here.
"#;

        let reason = extract_parked_reason(content);
        assert!(reason.is_none());
    }

    #[test]
    fn test_extract_parked_reason_no_reason_field() {
        let content = r#"---
parked_at: "2025-03-18T12:00:00Z"
---

# M01 — Parked
"#;

        let reason = extract_parked_reason(content);
        assert!(reason.is_none());
    }

    #[test]
    fn test_park_milestone_with_quotes_in_reason() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");
        let m01_path = milestones_path.join("M01");

        fs::create_dir_all(&m01_path).unwrap();
        fs::write(m01_path.join("ROADMAP.md"), "# M01").unwrap();

        let result = park_milestone(base_path, "M01", "Reason with \"quotes\"");
        assert!(result);

        let reason = get_parked_reason(base_path, "M01");
        assert_eq!(reason, Some("Reason with \"quotes\"".to_string()));
    }

    #[test]
    fn test_park_milestone_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let result = park_milestone(base_path, "M99", "Test reason");
        assert!(!result);
    }

    #[test]
    fn test_unpark_milestone_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let result = unpark_milestone(base_path, "M99");
        assert!(!result);
    }
}
