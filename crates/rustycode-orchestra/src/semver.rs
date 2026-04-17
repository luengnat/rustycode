// rustycode-orchestra/src/semver.rs
//! Semantic versioning utilities for version comparison.
//!
//! Provides semver comparison functionality without external dependencies.
//! Useful for checking if a newer version is available.

/// Compares two semver strings.
///
/// Returns:
/// * `Some(1)` if a > b (a is newer)
/// * `Some(-1)` if a < b (b is newer)
/// * `Some(0)` if a == b (same version)
/// * `None` if either version is invalid
///
/// # Arguments
/// * `a` - First semver string (e.g. "1.2.3")
/// * `b` - Second semver string (e.g. "1.2.4")
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::semver::compare_semver;
///
/// assert_eq!(compare_semver("1.2.3", "1.2.4"), Some(-1));
/// assert_eq!(compare_semver("2.0.0", "1.9.9"), Some(1));
/// assert_eq!(compare_semver("1.0.0", "1.0.0"), Some(0));
/// assert_eq!(compare_semver("invalid", "1.0.0"), None);
/// ```
pub fn compare_semver(a: &str, b: &str) -> Option<i8> {
    let pa = parse_semver(a)?;
    let pb = parse_semver(b)?;

    if pa > pb {
        Some(1)
    } else if pa < pb {
        Some(-1)
    } else {
        Some(0)
    }
}

/// Parse a semver string into its numeric components.
///
/// Returns None if the version string is invalid.
fn parse_semver(version: &str) -> Option<[u64; 3]> {
    let parts: Vec<&str> = version.split('.').collect();

    if parts.is_empty() || parts.len() > 4 {
        return None;
    }

    let mut result = [0u64; 3];

    for (i, part) in parts.iter().enumerate() {
        if i >= 3 {
            break; // Ignore build metadata or additional parts
        }

        // Parse numeric part, handling potential non-numeric suffixes
        let numeric_part: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();

        if numeric_part.is_empty() {
            return None;
        }

        result[i] = numeric_part.parse().ok()?;
    }

    Some(result)
}

/// Check if version a is newer than version b.
///
/// Returns false if either version is invalid.
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::semver::is_newer;
///
/// assert!(is_newer("2.0.0", "1.9.9"));
/// assert!(!is_newer("1.0.0", "1.0.0"));
/// assert!(!is_newer("1.0.0", "2.0.0"));
/// ```
pub fn is_newer(a: &str, b: &str) -> bool {
    compare_semver(a, b).is_some_and(|cmp| cmp > 0)
}

/// Check if version a is newer than or equal to version b.
///
/// Returns false if either version is invalid.
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::semver::is_newer_or_equal;
///
/// assert!(is_newer_or_equal("2.0.0", "1.9.9"));
/// assert!(is_newer_or_equal("1.0.0", "1.0.0"));
/// assert!(!is_newer_or_equal("1.0.0", "2.0.0"));
/// ```
pub fn is_newer_or_equal(a: &str, b: &str) -> bool {
    compare_semver(a, b).is_some_and(|cmp| cmp >= 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_semver_equal() {
        assert_eq!(compare_semver("1.0.0", "1.0.0"), Some(0));
        assert_eq!(compare_semver("2.5.3", "2.5.3"), Some(0));
    }

    #[test]
    fn test_compare_semver_newer_major() {
        assert_eq!(compare_semver("2.0.0", "1.9.9"), Some(1));
        assert_eq!(compare_semver("10.0.0", "9.9.9"), Some(1));
    }

    #[test]
    fn test_compare_semver_newer_minor() {
        assert_eq!(compare_semver("1.2.0", "1.1.9"), Some(1));
        assert_eq!(compare_semver("1.10.0", "1.9.9"), Some(1));
    }

    #[test]
    fn test_compare_semver_newer_patch() {
        assert_eq!(compare_semver("1.0.1", "1.0.0"), Some(1));
        assert_eq!(compare_semver("1.0.10", "1.0.9"), Some(1));
    }

    #[test]
    fn test_compare_semver_older() {
        assert_eq!(compare_semver("1.0.0", "1.0.1"), Some(-1));
        assert_eq!(compare_semver("1.1.0", "1.2.0"), Some(-1));
        assert_eq!(compare_semver("2.0.0", "3.0.0"), Some(-1));
    }

    #[test]
    fn test_compare_semver_invalid() {
        assert_eq!(compare_semver("invalid", "1.0.0"), None);
        assert_eq!(compare_semver("1.0.0", "invalid"), None);
        assert_eq!(compare_semver("", "1.0.0"), None);
        assert_eq!(compare_semver("1.0.0", ""), None);
    }

    #[test]
    fn test_compare_semver_with_prerelease() {
        // Handles versions with pre-release tags by ignoring them
        assert_eq!(compare_semver("1.0.0-alpha", "1.0.0"), Some(0));
        assert_eq!(compare_semver("1.0.0-beta", "1.0.0"), Some(0));
    }

    #[test]
    fn test_compare_semver_different_lengths() {
        // Should handle versions with different numbers of components
        assert_eq!(compare_semver("1.0", "1.0.0"), Some(0));
        assert_eq!(compare_semver("1.0.0", "1.0"), Some(0));
        assert_eq!(compare_semver("1.0.0", "1"), Some(0));
    }

    #[test]
    fn test_parse_semver_valid() {
        assert_eq!(parse_semver("1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_semver("10.20.30"), Some([10, 20, 30]));
        assert_eq!(parse_semver("0.0.0"), Some([0, 0, 0]));
    }

    #[test]
    fn test_parse_semver_two_components() {
        assert_eq!(parse_semver("1.2"), Some([1, 2, 0]));
        assert_eq!(parse_semver("10.20"), Some([10, 20, 0]));
    }

    #[test]
    fn test_parse_semver_one_component() {
        assert_eq!(parse_semver("1"), Some([1, 0, 0]));
        assert_eq!(parse_semver("10"), Some([10, 0, 0]));
    }

    #[test]
    fn test_parse_semver_with_suffix() {
        // Should strip non-numeric suffixes
        assert_eq!(parse_semver("1.2.3-alpha"), Some([1, 2, 3]));
        assert_eq!(parse_semver("1.2.3-beta.1"), Some([1, 2, 3]));
    }

    #[test]
    fn test_parse_semver_invalid() {
        assert_eq!(parse_semver(""), None);
        assert_eq!(parse_semver("invalid"), None);
        assert_eq!(parse_semver("a.b.c"), None);
    }

    #[test]
    fn test_is_newer() {
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(is_newer("1.1.0", "1.0.9"));
        assert!(is_newer("1.0.1", "1.0.0"));

        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.1"));
        assert!(!is_newer("1.0.0", "2.0.0"));
    }

    #[test]
    fn test_is_newer_invalid() {
        assert!(!is_newer("invalid", "1.0.0"));
        assert!(!is_newer("1.0.0", "invalid"));
    }

    #[test]
    fn test_is_newer_or_equal() {
        assert!(is_newer_or_equal("2.0.0", "1.9.9"));
        assert!(is_newer_or_equal("1.0.0", "1.0.0"));
        assert!(is_newer_or_equal("1.0.1", "1.0.0"));

        assert!(!is_newer_or_equal("1.0.0", "1.0.1"));
        assert!(!is_newer_or_equal("1.0.0", "2.0.0"));
    }

    #[test]
    fn test_is_newer_or_equal_invalid() {
        assert!(!is_newer_or_equal("invalid", "1.0.0"));
        assert!(!is_newer_or_equal("1.0.0", "invalid"));
    }

    #[test]
    fn test_real_world_versions() {
        // Test with actual version strings
        assert!(is_newer("1.2.0", "1.1.9"));
        assert!(!is_newer("1.0.0", "1.2.0"));
        assert!(is_newer_or_equal("2.0.0", "2.0.0"));
    }

    #[test]
    fn test_large_versions() {
        assert_eq!(compare_semver("100.200.300", "99.299.299"), Some(1));
        assert_eq!(compare_semver("1.2.3", "1.2.3"), Some(0));
    }

    #[test]
    fn test_zero_versions() {
        assert_eq!(compare_semver("0.0.1", "0.0.0"), Some(1));
        assert_eq!(compare_semver("0.1.0", "0.0.9"), Some(1));
        assert_eq!(compare_semver("1.0.0", "0.9.9"), Some(1));
    }
}
