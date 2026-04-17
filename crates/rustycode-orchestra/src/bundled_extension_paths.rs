//! Bundled Extension Paths — Serialize and parse bundled extension path lists.
//!
//! Provides utilities for converting between path lists and delimited strings.
//! Handles cross-platform path delimiters (: on Unix, ; on Windows).
//!
//! Matches orchestra-2's bundled-extension-paths.ts implementation.

/// Get the default path delimiter for the current platform
///
/// Returns `:` on Unix/Linux/macOS, `;` on Windows
fn default_delimiter() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        ";"
    }
    #[cfg(not(target_os = "windows"))]
    {
        ":"
    }
}

/// Serialize a list of extension paths into a delimited string
///
/// # Arguments
/// * `paths` - Slice of path strings to serialize
/// * `path_delimiter` - Optional delimiter (defaults to platform-specific)
///
/// # Returns
/// Delimited string of non-empty paths
///
/// # Examples
/// ```
/// use rustycode_orchestra::bundled_extension_paths::serialize_bundled_extension_paths;
///
/// let paths = vec![
///     "/path/to/ext1".to_string(),
///     "/path/to/ext2".to_string(),
///     "".to_string(), // Empty paths are filtered out
/// ];
/// let serialized = serialize_bundled_extension_paths(&paths, None);
/// assert_eq!(serialized, "/path/to/ext1:/path/to/ext2");
/// ```
pub fn serialize_bundled_extension_paths(paths: &[String], path_delimiter: Option<&str>) -> String {
    let delimiter = match path_delimiter {
        Some(d) => d,
        None => default_delimiter(),
    };
    paths
        .iter()
        .filter(|p| !p.is_empty())
        .map(|p| p.as_str())
        .collect::<Vec<_>>()
        .join(delimiter)
}

/// Parse a delimited string into a list of extension paths
///
/// # Arguments
/// * `value` - Optional delimited string to parse
/// * `path_delimiter` - Optional delimiter (defaults to platform-specific)
///
/// # Returns
/// Vector of trimmed, non-empty path strings
///
/// # Examples
/// ```
/// use rustycode_orchestra::bundled_extension_paths::parse_bundled_extension_paths;
///
/// let input = Some("/path/to/ext1:/path/to/ext2:/empty:".to_string());
/// let parsed = parse_bundled_extension_paths(input.as_deref(), None);
/// assert_eq!(parsed, vec!["/path/to/ext1", "/path/to/ext2", "/empty"]);
/// ```
pub fn parse_bundled_extension_paths(
    value: Option<&str>,
    path_delimiter: Option<&str>,
) -> Vec<String> {
    let delimiter = match path_delimiter {
        Some(d) => d,
        None => default_delimiter(),
    };
    value
        .unwrap_or("")
        .split(delimiter)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_basic() {
        let paths = vec!["/path/to/ext1".to_string(), "/path/to/ext2".to_string()];
        let serialized = serialize_bundled_extension_paths(&paths, None);
        #[cfg(target_os = "windows")]
        assert_eq!(serialized, "/path/to/ext1;/path/to/ext2");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(serialized, "/path/to/ext1:/path/to/ext2");
    }

    #[test]
    fn test_serialize_filters_empty() {
        let paths = vec![
            "/path/to/ext1".to_string(),
            "".to_string(),
            "/path/to/ext2".to_string(),
            "".to_string(),
        ];
        let serialized = serialize_bundled_extension_paths(&paths, None);
        #[cfg(target_os = "windows")]
        assert_eq!(serialized, "/path/to/ext1;/path/to/ext2");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(serialized, "/path/to/ext1:/path/to/ext2");
    }

    #[test]
    fn test_serialize_custom_delimiter() {
        let paths = vec!["/path/to/ext1".to_string(), "/path/to/ext2".to_string()];
        let serialized = serialize_bundled_extension_paths(&paths, Some("|"));
        assert_eq!(serialized, "/path/to/ext1|/path/to/ext2");
    }

    #[test]
    fn test_serialize_empty_list() {
        let paths: Vec<String> = vec![];
        let serialized = serialize_bundled_extension_paths(&paths, None);
        assert_eq!(serialized, "");
    }

    #[test]
    fn test_parse_basic() {
        #[cfg(target_os = "windows")]
        let input = Some("/path/to/ext1;/path/to/ext2");
        #[cfg(not(target_os = "windows"))]
        let input = Some("/path/to/ext1:/path/to/ext2");

        let parsed = parse_bundled_extension_paths(input, None);
        assert_eq!(parsed, vec!["/path/to/ext1", "/path/to/ext2"]);
    }

    #[test]
    fn test_parse_none() {
        let parsed = parse_bundled_extension_paths(None, None);
        assert_eq!(parsed, Vec::<String>::new());
    }

    #[test]
    fn test_parse_empty_string() {
        let parsed = parse_bundled_extension_paths(Some(""), None);
        assert_eq!(parsed, Vec::<String>::new());
    }

    #[test]
    fn test_parse_trims_whitespace() {
        #[cfg(target_os = "windows")]
        let input = Some(" /path/to/ext1 ; /path/to/ext2 ; ");
        #[cfg(not(target_os = "windows"))]
        let input = Some(" /path/to/ext1 : /path/to/ext2 : ");

        let parsed = parse_bundled_extension_paths(input, None);
        assert_eq!(parsed, vec!["/path/to/ext1", "/path/to/ext2"]);
    }

    #[test]
    fn test_parse_filters_empty_segments() {
        #[cfg(target_os = "windows")]
        let input = Some("/path/to/ext1;;/path/to/ext2;;");
        #[cfg(not(target_os = "windows"))]
        let input = Some("/path/to/ext1::/path/to/ext2::");

        let parsed = parse_bundled_extension_paths(input, None);
        assert_eq!(parsed, vec!["/path/to/ext1", "/path/to/ext2"]);
    }

    #[test]
    fn test_parse_custom_delimiter() {
        let input = Some("/path/to/ext1|/path/to/ext2|/path/to/ext3");
        let parsed = parse_bundled_extension_paths(input, Some("|"));
        assert_eq!(
            parsed,
            vec!["/path/to/ext1", "/path/to/ext2", "/path/to/ext3"]
        );
    }

    #[test]
    fn test_roundtrip() {
        let original = vec![
            "/path/to/ext1".to_string(),
            "/path/to/ext2".to_string(),
            "/path/to/ext3".to_string(),
        ];

        let serialized = serialize_bundled_extension_paths(&original, None);
        let parsed = parse_bundled_extension_paths(Some(&serialized), None);

        assert_eq!(parsed, original);
    }
}
