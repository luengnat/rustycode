//! Orchestra Path Display — Cross-platform path display for LLM-visible text.
//!
//! Paths injected into prompts, tool results, or extension messages must use
//! forward slashes. Windows backslash paths cause bash failures when the model
//! copies them into shell commands — bash interprets backslashes as escape chars.
//!
//! Use this ONLY for paths entering text the LLM or shell sees.
//! Filesystem operations (fs::read_file, path joining, spawn cwd) handle native
//! separators correctly and should NOT be normalized.
//!
//! Matches orchestra-2's path-display.ts implementation.

// ─── Path Conversion ───────────────────────────────────────────────────────────

/// Convert a filesystem path to forward-slash form for display in LLM text.
///
/// No-op on Unix. On Windows converts `C:\Users\name` to `C:/Users/name`.
///
/// # Arguments
/// * `fs_path` - The filesystem path to convert
///
/// # Returns
/// Path with forward slashes
///
/// # Examples
/// ```
/// use rustycode_orchestra::path_display::to_posix_path;
///
/// // Unix paths unchanged
/// assert_eq!(to_posix_path("/usr/local/bin"), "/usr/local/bin");
///
/// // Windows paths converted
/// assert_eq!(to_posix_path(r"C:\Users\name"), "C:/Users/name");
/// assert_eq!(to_posix_path(r"relative\path"), "relative/path");
/// ```
pub fn to_posix_path(fs_path: &str) -> String {
    fs_path.replace('\\', "/")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_posix_path_unix() {
        // Unix paths unchanged
        assert_eq!(to_posix_path("/usr/local/bin"), "/usr/local/bin");
        assert_eq!(to_posix_path("relative/path"), "relative/path");
        assert_eq!(to_posix_path("./file.txt"), "./file.txt");
    }

    #[test]
    fn test_to_posix_path_windows() {
        // Windows paths converted
        assert_eq!(to_posix_path(r"C:\Users\name"), "C:/Users/name");
        assert_eq!(to_posix_path(r"D:\Projects\test"), "D:/Projects/test");
        assert_eq!(to_posix_path(r"relative\path"), "relative/path");
        assert_eq!(to_posix_path(r".\file.txt"), "./file.txt");
    }

    #[test]
    fn test_to_posix_path_mixed() {
        // Mixed separators
        assert_eq!(to_posix_path(r"C:\Users/name\path"), "C:/Users/name/path");
        assert_eq!(
            to_posix_path(r"relative/path\to\file"),
            "relative/path/to/file"
        );
    }

    #[test]
    fn test_to_posix_path_empty() {
        assert_eq!(to_posix_path(""), "");
    }

    #[test]
    fn test_to_posix_path_no_backslashes() {
        // Paths without backslashes unchanged
        assert_eq!(to_posix_path("forward/slash/path"), "forward/slash/path");
        assert_eq!(to_posix_path("no_slashes"), "no_slashes");
    }

    #[test]
    fn test_to_posix_path_multiple_backslashes() {
        // Multiple consecutive backslashes
        assert_eq!(to_posix_path(r"path\\to\\file"), "path//to//file");
        assert_eq!(to_posix_path(r"C:\\\\Windows"), "C:////Windows");
    }
}
