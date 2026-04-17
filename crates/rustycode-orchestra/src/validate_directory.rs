//! Orchestra Directory Validation — Safeguards against running in dangerous directories.
//!
//! Prevents Orchestra from creating .orchestra/ structures in system paths, home directories,
//! or other locations where writing project scaffolding would be harmful.
//!
//! Matches orchestra-2's validate-directory.ts implementation.

use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

// ─── Types ───────────────────────────────────────────────────────────────────

/// Result of directory validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryValidationResult {
    /// Whether the directory is safe for Orchestra operations
    pub safe: bool,
    /// Severity level
    pub severity: DirectoryValidationSeverity,
    /// Human-readable reason if not safe
    pub reason: Option<String>,
}

/// Severity level for directory validation issues
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DirectoryValidationSeverity {
    /// Directory is safe
    Ok,
    /// Hard stop - operation cannot proceed
    Blocked,
    /// Warning - user can override
    Warning,
}

// ─── Blocked Paths ────────────────────────────────────────────────────────────

/// Paths where Orchestra must never create .orchestra/ — no override possible.
#[cfg(unix)]
fn unix_blocked_paths() -> HashSet<&'static str> {
    [
        "/",
        "/bin",
        "/sbin",
        "/usr",
        "/usr/bin",
        "/usr/sbin",
        "/usr/lib",
        "/usr/local",
        "/usr/local/bin",
        "/etc",
        "/var",
        "/var/tmp",
        "/dev",
        "/proc",
        "/sys",
        "/boot",
        "/lib",
        "/lib64",
        // macOS-specific
        "/System",
        "/Library",
        "/Applications",
        "/Volumes",
        "/private",
        "/private/var",
        "/private/etc",
        "/private/tmp",
    ]
    .into_iter()
    .collect()
}

/// Paths where Orchestra must never create .orchestra/ — no override possible.
#[cfg(windows)]
fn windows_blocked_paths() -> HashSet<&'static str> {
    [
        "C:\\",
        "C:\\Windows",
        "C:\\Windows\\System32",
        "C:\\Program Files",
        "C:\\Program Files (x86)",
    ]
    .into_iter()
    .collect()
}

/// Platform-specific blocked paths
fn blocked_paths() -> HashSet<String> {
    #[cfg(unix)]
    {
        unix_blocked_paths()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[cfg(windows)]
    {
        windows_blocked_paths()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    }
}

// ─── Core Validation ────────────────────────────────────────────────────────

/// Validate whether a directory is safe for Orchestra to operate in.
///
/// # Checks
/// 1. Blocked system paths (hard stop)
/// 2. Home directory itself (hard stop)
/// 3. Temp directory root (hard stop)
/// 4. High entry count heuristic (warning)
///
/// # Arguments
/// * `dir_path` - Directory path to validate
///
/// # Returns
/// Validation result indicating safety and any issues found
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::validate_directory::*;
///
/// let result = validate_directory(Path::new("/my/project"));
/// assert!(result.safe);
/// ```
pub fn validate_directory(dir_path: &Path) -> DirectoryValidationResult {
    // Resolve to absolute + follow symlinks so aliases can't bypass checks
    let resolved = match fs::canonicalize(dir_path) {
        Ok(canonicalized) => canonicalized,
        Err(_) => dir_path.to_path_buf(),
    };

    // Normalize trailing slashes for consistent comparison
    let normalized = normalize_path(&resolved);

    // ── Check 1: Blocked system paths ─────────────────────────────────
    let blocked = blocked_paths();
    if blocked.contains(&normalized) {
        return DirectoryValidationResult {
            safe: false,
            severity: DirectoryValidationSeverity::Blocked,
            reason: Some(format!(
                "Refusing to run in system directory: {}. Orchestra must be run inside a project directory.",
                normalized
            )),
        };
    }

    // ── Check 2: Home directory itself (not subdirs) ─────────────────────
    if let Some(home_dir) = dirs::home_dir() {
        let normalized_home = normalize_path(&home_dir);
        if normalized == normalized_home {
            return DirectoryValidationResult {
                safe: false,
                severity: DirectoryValidationSeverity::Blocked,
                reason: Some(format!(
                    "Refusing to run in your home directory ({}). Orchestra must be run inside a project directory, not $HOME.",
                    normalized
                )),
            };
        }
    }

    // ── Check 3: Temp directory root ───────────────────────────────────
    let temp_dir = std::env::temp_dir();
    // Canonicalize temp_dir to handle symlinks (e.g., /var -> /private/var on macOS)
    let temp_dir_resolved = match fs::canonicalize(&temp_dir) {
        Ok(canonicalized) => canonicalized,
        Err(_) => temp_dir.clone(),
    };
    let normalized_temp = normalize_path(&temp_dir_resolved);
    if normalized == normalized_temp {
        return DirectoryValidationResult {
            safe: false,
            severity: DirectoryValidationSeverity::Blocked,
            reason: Some(format!(
                "Refusing to run in the system temp directory ({}). Use a project subdirectory instead.",
                normalized
            )),
        };
    }

    // ── Check 4: Suspiciously large directory (heuristic warning) ──────
    if let Ok(entries) = fs::read_dir(&normalized) {
        let count = entries.filter_map(|e| e.ok()).count();
        if count > 200 {
            return DirectoryValidationResult {
                safe: false,
                severity: DirectoryValidationSeverity::Warning,
                reason: Some(format!(
                    "This directory has {} entries, which suggests it may not be a project directory. Are you sure you want to initialize Orchestra here?",
                    count
                )),
            };
        }
    }

    DirectoryValidationResult {
        safe: true,
        severity: DirectoryValidationSeverity::Ok,
        reason: None,
    }
}

/// Assert that a directory is safe for Orchestra operations.
///
/// # Arguments
/// * `dir_path` - Directory path to validate
///
/// # Returns
/// Validation result for warnings (caller decides how to handle)
///
/// # Errors
/// Returns an error if the directory is blocked (severity = "blocked")
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::validate_directory::*;
///
/// let result = assert_safe_directory(Path::new("/my/project"))?;
/// assert!(result.safe);
/// ```
pub fn assert_safe_directory(dir_path: &Path) -> Result<DirectoryValidationResult> {
    let result = validate_directory(dir_path);
    if result.severity == DirectoryValidationSeverity::Blocked {
        anyhow::bail!(result.reason.unwrap_or_default());
    }
    Ok(result)
}

/// Normalize a path by removing trailing slashes (except for root).
///
/// Special cases:
/// - "/" → "/" (not "")
/// - "C:\" → "C:\" (not "C:")
fn normalize_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();

    // Remove trailing slashes
    let trimmed = path_str.trim_end_matches(['/', '\\']);

    // Handle root directories
    if trimmed.is_empty() {
        if cfg!(unix) {
            return "/".to_string();
        } else if cfg!(windows) && path_str.len() >= 2 && path_str.as_bytes()[1] == b':' {
            // Drive root like "C:"
            return format!("{}\\", &path_str[..2]);
        }
    }

    // Handle Windows drive root
    #[cfg(windows)]
    if let Some(drive) = extract_drive_letter(&trimmed) {
        return format!("{}\\", drive);
    }

    trimmed.to_string()
}

/// Extract drive letter from Windows path (e.g., "C:" from "C:\path").
#[cfg(windows)]
fn extract_drive_letter(path: &str) -> Option<String> {
    if path.len() >= 2 {
        let bytes = path.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return Some(format!("{}:", &path[..2]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_unix() {
        let cases = [
            ("/", "/"),
            ("/usr", "/usr"),
            ("/usr/", "/usr"),
            ("/home/user/project", "/home/user/project"),
            ("/home/user/project/", "/home/user/project"),
        ];

        for (input, expected) in cases {
            assert_eq!(normalize_path(Path::new(input)), expected);
        }
    }

    #[test]
    #[cfg(windows)]
    fn test_normalize_path_windows() {
        let cases = [
            (r"C:\", r"C:\"),
            (r"C:\Users", r"C:\Users"),
            (r"C:\Users\", r"C:\Users"),
            (r"D:\Projects\my-project", r"D:\Projects\my-project"),
        ];

        for (input, expected) in cases {
            assert_eq!(normalize_path(Path::new(input)), expected);
        }
    }

    #[test]
    fn test_validate_directory_safe_path() {
        let temp_dir = std::env::temp_dir();
        let project_dir = temp_dir.join("test-project");
        let result = validate_directory(&project_dir);
        assert!(result.safe);
        assert_eq!(result.severity, DirectoryValidationSeverity::Ok);
    }

    #[test]
    #[cfg(unix)]
    fn test_validate_directory_blocked_system_path() {
        let result = validate_directory(Path::new("/usr"));
        assert!(!result.safe);
        assert_eq!(result.severity, DirectoryValidationSeverity::Blocked);
        assert!(result.reason.as_ref().unwrap().contains("system directory"));
    }

    #[test]
    #[cfg(unix)]
    fn test_validate_directory_home_directory() {
        if let Some(home) = dirs::home_dir() {
            let result = validate_directory(&home);
            assert!(!result.safe);
            assert_eq!(result.severity, DirectoryValidationSeverity::Blocked);
            assert!(result.reason.as_ref().unwrap().contains("home directory"));
        }
    }

    #[test]
    fn test_validate_directory_temp_directory() {
        let temp_dir = std::env::temp_dir();
        let result = validate_directory(&temp_dir);
        assert!(!result.safe);
        assert_eq!(result.severity, DirectoryValidationSeverity::Blocked);
        assert!(result.reason.as_ref().unwrap().contains("temp directory"));
    }

    #[test]
    fn test_assert_safe_directory_blocked() {
        let result = assert_safe_directory(Path::new("/usr"));
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_safe_directory_safe() {
        let temp_dir = std::env::temp_dir();
        let project_dir = temp_dir.join("test-project");
        let result = assert_safe_directory(&project_dir);
        assert!(result.is_ok());
        assert!(result.unwrap().safe);
    }
}
