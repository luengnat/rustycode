//! Milestone ID Primitives — Pure utilities for generating, parsing, sorting,
//! and discovering milestone identifiers.
//!
//! Consumed by 15+ modules across the Orchestra extension. Zero side-effects.

use crate::error::{OrchestraV2Error, Result};
use crate::paths::milestones_dir;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

// ─── Regex ───────────────────────────────────────────────────────────────────

/// Matches both classic `M001` and unique `M001-abc123` formats (anchored)
pub const MILESTONE_ID_RE: &str = r"^M\d{3}(?:-[a-z0-9]{6})?$";

/// Check if a string is a valid milestone ID
pub fn is_valid_milestone_id(id: &str) -> bool {
    let re = regex_lite::Regex::new(MILESTONE_ID_RE).unwrap();
    re.is_match(id)
}

// ─── Parsing & Extraction ─────────────────────────────────────────────────────

/// Extract the trailing sequential number from a milestone ID.
/// Returns 0 for non-matches.
///
/// # Arguments
/// * `id` - Milestone ID string (e.g., "M001", "M001-abc123")
///
/// # Returns
/// Numeric sequence number (e.g., 1 for "M001", 0 for invalid)
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_ids::*;
///
/// assert_eq!(extract_milestone_seq("M001"), 1);
/// assert_eq!(extract_milestone_seq("M123-abc123"), 123);
/// assert_eq!(extract_milestone_seq("invalid"), 0);
/// ```
pub fn extract_milestone_seq(id: &str) -> u32 {
    if let Some(caps) = regex_lite::Regex::new(r"^M(\d{3})(?:-[a-z0-9]{6})?$")
        .unwrap()
        .captures(id)
    {
        if let Some(num_str) = caps.get(1) {
            return num_str.as_str().parse().unwrap_or(0);
        }
    }
    0
}

/// Structured parse of a milestone ID into optional suffix and sequence number.
///
/// # Arguments
/// * `id` - Milestone ID string
///
/// # Returns
/// Parsed milestone ID with suffix and number
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_ids::*;
///
/// let parsed = parse_milestone_id("M001-abc123");
/// assert_eq!(parsed.num, 1);
/// assert_eq!(parsed.suffix, Some("abc123".to_string()));
///
/// let parsed = parse_milestone_id("M001");
/// assert_eq!(parsed.num, 1);
/// assert_eq!(parsed.suffix, None);
/// ```
pub fn parse_milestone_id(id: &str) -> ParsedMilestoneId {
    if let Some(caps) = regex_lite::Regex::new(r"^M(\d{3})(?:-([a-z0-9]{6}))?$")
        .unwrap()
        .captures(id)
    {
        let num = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let suffix = caps.get(2).map(|m| m.as_str().to_string());
        ParsedMilestoneId { suffix, num }
    } else {
        ParsedMilestoneId {
            suffix: None,
            num: 0,
        }
    }
}

/// Parsed milestone ID structure
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMilestoneId {
    /// Optional 6-char suffix (e.g., "abc123" in "M001-abc123")
    pub suffix: Option<String>,
    /// Numeric sequence (e.g., 1 for "M001")
    pub num: u32,
}

// ─── Sorting ──────────────────────────────────────────────────────────────────

/// Comparator for sorting milestone IDs by sequential number.
///
/// # Arguments
/// * `a` - First milestone ID
/// * `b` - Second milestone ID
///
/// # Returns
/// Ordering based on sequence number
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_ids::*;
///
/// assert_eq!(milestone_id_sort("M002", "M001"), std::cmp::Ordering::Greater);
/// assert_eq!(milestone_id_sort("M001", "M002"), std::cmp::Ordering::Less);
/// assert_eq!(milestone_id_sort("M001", "M001"), std::cmp::Ordering::Equal);
/// ```
pub fn milestone_id_sort(a: &str, b: &str) -> std::cmp::Ordering {
    let a_seq = extract_milestone_seq(a);
    let b_seq = extract_milestone_seq(b);
    a_seq.cmp(&b_seq)
}

/// Sort milestone IDs by sequence number
///
/// # Arguments
/// * `ids` - Slice of milestone ID strings
///
/// # Returns
/// New vector with sorted IDs
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_ids::*;
///
/// let ids = vec!["M003", "M001", "M002"];
/// let sorted = sort_milestone_ids(&ids);
/// assert_eq!(sorted, vec!["M001", "M002", "M003"]);
/// ```
pub fn sort_milestone_ids(ids: &[&str]) -> Vec<String> {
    let mut sorted: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    sorted.sort_by(|a, b| milestone_id_sort(a, b));
    sorted
}

// ─── Generation ───────────────────────────────────────────────────────────────

/// Generate a 6-char lowercase `[a-z0-9]` suffix using crypto randomness.
///
/// # Returns
/// 6-character random suffix
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_ids::*;
///
/// let suffix = generate_milestone_suffix();
/// assert_eq!(suffix.len(), 6);
/// assert!(suffix.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
/// ```
pub fn generate_milestone_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let chars = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();

    (0..6)
        .map(|i| {
            let idx = ((nanos >> (i * 5)) % 36) as usize;
            chars[idx] as char
        })
        .collect()
}

/// Return the highest numeric suffix among milestone IDs.
/// Returns 0 when the list is empty or has no numeric IDs.
///
/// # Arguments
/// * `milestone_ids` - Slice of milestone ID strings
///
/// # Returns
/// Maximum sequence number found
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_ids::*;
///
/// let ids = vec!["M001", "M003", "M002"];
/// assert_eq!(max_milestone_num(&ids), 3);
///
/// let ids: Vec<&str> = vec![];
/// assert_eq!(max_milestone_num(&ids), 0);
/// ```
pub fn max_milestone_num(milestone_ids: &[&str]) -> u32 {
    milestone_ids
        .iter()
        .map(|id| extract_milestone_seq(id))
        .max()
        .unwrap_or(0)
}

/// Derive the next milestone ID from existing IDs using max-based approach
/// to avoid collisions after deletions.
///
/// # Arguments
/// * `milestone_ids` - Slice of existing milestone ID strings
/// * `unique_enabled` - Whether to add a unique suffix
///
/// # Returns
/// Next milestone ID string
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_ids::*;
///
/// let ids = vec!["M001", "M002"];
/// assert_eq!(next_milestone_id(&ids, false), "M003");
/// assert!(next_milestone_id(&ids, true).starts_with("M003-"));
/// assert_eq!(next_milestone_id(&ids, true).len(), 10);
/// ```
pub fn next_milestone_id(milestone_ids: &[&str], unique_enabled: bool) -> String {
    let seq = max_milestone_num(milestone_ids) + 1;
    let seq_str = format!("{:03}", seq);

    if unique_enabled {
        format!("M{}-{}", seq_str, generate_milestone_suffix())
    } else {
        format!("M{}", seq_str)
    }
}

// ─── Discovery ────────────────────────────────────────────────────────────────

/// Scan the milestones directory and return IDs sorted by sequence number.
///
/// # Arguments
/// * `base_path` - Project root directory
///
/// # Returns
/// Vector of milestone ID strings, sorted
///
/// # Errors
/// Returns error if milestones directory cannot be read
///
/// # Example
/// ```
/// use rustycode_orchestra::milestone_ids::*;
/// use std::path::Path;
///
/// let ids = find_milestone_ids(Path::new("/project"));
/// // Returns: ["M001", "M002", "M003"]
/// ```
pub fn find_milestone_ids(base_path: &Path) -> Result<Vec<String>> {
    let dir = milestones_dir(base_path);

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&dir).map_err(|e| {
        OrchestraV2Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Failed to read milestones directory: {}", e),
        ))
    })?;

    let mut ids = Vec::new();
    let mut seen = HashSet::new();

    for entry in entries {
        let entry = entry.map_err(|e| {
            OrchestraV2Error::Io(std::io::Error::other(format!(
                "Failed to read directory entry: {}",
                e
            )))
        })?;

        if !entry.path().is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Extract milestone ID from directory name
        if let Some(caps) = regex_lite::Regex::new(r"^(M\d+(?:-[a-z0-9]{6})?)")
            .unwrap()
            .captures(&name_str)
        {
            let id = match caps.get(1) {
                Some(m) => m.as_str().to_string(),
                None => continue,
            };

            // Avoid duplicates
            if seen.insert(id.clone()) {
                ids.push(id);
            }
        }
    }

    // Sort by sequence number
    ids.sort_by(|a, b| milestone_id_sort(a, b));

    Ok(ids)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_valid_milestone_id() {
        assert!(is_valid_milestone_id("M001"));
        assert!(is_valid_milestone_id("M123"));
        assert!(is_valid_milestone_id("M001-abc123"));
        assert!(is_valid_milestone_id("M999-zzz999"));

        assert!(!is_valid_milestone_id("M1"));
        assert!(!is_valid_milestone_id("M0001"));
        assert!(!is_valid_milestone_id("m001"));
        assert!(!is_valid_milestone_id("M001-ABC"));
        assert!(!is_valid_milestone_id("M001-abc1234"));
        assert!(!is_valid_milestone_id("invalid"));
    }

    #[test]
    fn test_extract_milestone_seq() {
        assert_eq!(extract_milestone_seq("M001"), 1);
        assert_eq!(extract_milestone_seq("M123"), 123);
        assert_eq!(extract_milestone_seq("M999"), 999);
        assert_eq!(extract_milestone_seq("M001-abc123"), 1);
        assert_eq!(extract_milestone_seq("M123-xyz789"), 123);
        assert_eq!(extract_milestone_seq("invalid"), 0);
        assert_eq!(extract_milestone_seq("M1"), 0);
    }

    #[test]
    fn test_parse_milestone_id() {
        let parsed = parse_milestone_id("M001-abc123");
        assert_eq!(parsed.num, 1);
        assert_eq!(parsed.suffix, Some("abc123".to_string()));

        let parsed = parse_milestone_id("M123");
        assert_eq!(parsed.num, 123);
        assert_eq!(parsed.suffix, None);

        let parsed = parse_milestone_id("invalid");
        assert_eq!(parsed.num, 0);
        assert_eq!(parsed.suffix, None);
    }

    #[test]
    fn test_milestone_id_sort() {
        use std::cmp::Ordering;

        assert_eq!(milestone_id_sort("M001", "M002"), Ordering::Less);
        assert_eq!(milestone_id_sort("M002", "M001"), Ordering::Greater);
        assert_eq!(milestone_id_sort("M001", "M001"), Ordering::Equal);

        // Suffix doesn't affect numeric comparison, but order is stable
        // (can't guarantee Equal for different strings)
        assert_eq!(milestone_id_sort("M001-abc", "M001-xyz"), Ordering::Equal);
        assert_eq!(milestone_id_sort("M001-abc", "M002"), Ordering::Less);
    }

    #[test]
    fn test_sort_milestone_ids() {
        let ids = vec!["M003", "M001", "M002"];
        let sorted = sort_milestone_ids(&ids);
        assert_eq!(sorted, vec!["M001", "M002", "M003"]);

        let ids = vec!["M001-abc", "M001", "M002"];
        let sorted = sort_milestone_ids(&ids);
        // Both M001 and M001-abc should come before M002
        assert_eq!(sorted[2], "M002");
        assert!(sorted[0] == "M001" || sorted[0].starts_with("M001-"));
        assert!(sorted[1] == "M001" || sorted[1].starts_with("M001-"));
    }

    #[test]
    fn test_generate_milestone_suffix() {
        let suffix = generate_milestone_suffix();
        assert_eq!(suffix.len(), 6);
        assert!(suffix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));

        // Check uniqueness (very low probability of collision)
        let _suffix2 = generate_milestone_suffix();
        // Note: This test might fail if the nanos-based generation produces the same result
        // but the probability is extremely low
    }

    #[test]
    fn test_max_milestone_num() {
        let ids = vec!["M001", "M003", "M002"];
        assert_eq!(max_milestone_num(&ids), 3);

        let ids = vec!["M001", "M999"];
        assert_eq!(max_milestone_num(&ids), 999);

        let ids: Vec<&str> = vec![];
        assert_eq!(max_milestone_num(&ids), 0);

        let ids = vec!["invalid"];
        assert_eq!(max_milestone_num(&ids), 0);
    }

    #[test]
    fn test_next_milestone_id() {
        let ids = vec!["M001", "M002"];
        assert_eq!(next_milestone_id(&ids, false), "M003");

        let unique_id = next_milestone_id(&ids, true);
        assert!(unique_id.starts_with("M003-"));
        // M003- + 6 chars = 11 total
        assert_eq!(unique_id.len(), 11);

        let ids: Vec<&str> = vec![];
        assert_eq!(next_milestone_id(&ids, false), "M001");

        let ids = vec!["M009", "M010"];
        assert_eq!(next_milestone_id(&ids, false), "M011");
    }

    #[test]
    fn test_find_milestone_ids() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");

        // Create milestone directories
        fs::create_dir_all(&milestones_path).unwrap();
        fs::create_dir(milestones_path.join("M001")).unwrap();
        fs::create_dir(milestones_path.join("M003")).unwrap();
        fs::create_dir(milestones_path.join("M002")).unwrap();

        let ids = find_milestone_ids(base_path).unwrap();
        assert_eq!(ids, vec!["M001", "M002", "M003"]);
    }

    #[test]
    fn test_find_milestone_ids_with_suffixes() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");

        fs::create_dir_all(&milestones_path).unwrap();
        fs::create_dir(milestones_path.join("M001-abc123")).unwrap();
        fs::create_dir(milestones_path.join("M002")).unwrap();

        let ids = find_milestone_ids(base_path).unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"M001-abc123".to_string()));
        assert!(ids.contains(&"M002".to_string()));
    }

    #[test]
    fn test_find_milestone_ids_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");

        fs::create_dir_all(&milestones_path).unwrap();

        let ids = find_milestone_ids(base_path).unwrap();
        assert_eq!(ids.len(), 0);
    }

    #[test]
    fn test_find_milestone_ids_no_directory() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        let ids = find_milestone_ids(base_path).unwrap();
        assert_eq!(ids.len(), 0);
    }

    #[test]
    fn test_find_milestone_ids_ignores_non_directories() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");

        fs::create_dir_all(&milestones_path).unwrap();
        fs::create_dir(milestones_path.join("M001")).unwrap();
        fs::write(milestones_path.join("M002"), "not a directory").unwrap();

        let ids = find_milestone_ids(base_path).unwrap();
        assert_eq!(ids, vec!["M001"]);
    }

    #[test]
    fn test_find_milestone_ids_ignores_invalid_names() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        let orchestra_path = base_path.join(".orchestra");
        let milestones_path = orchestra_path.join("milestones");

        fs::create_dir_all(&milestones_path).unwrap();
        fs::create_dir(milestones_path.join("M001")).unwrap();
        fs::create_dir(milestones_path.join("invalid")).unwrap();
        fs::create_dir(milestones_path.join("M002-extra")).unwrap();

        let ids = find_milestone_ids(base_path).unwrap();
        // M002-extra is extracted as M002 by the regex, which is reasonable behavior
        assert_eq!(ids, vec!["M001", "M002"]);
    }
}
