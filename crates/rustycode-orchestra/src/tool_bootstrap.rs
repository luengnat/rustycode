//! Tool bootstrapping — finding and provisioning managed tools.
//!
//! Finds tools (fd, rg) in the system PATH and copies or symlinks them
//! to a target directory for managed tool distribution.
//!
//! Matches orchestra-2's tool-bootstrap.ts implementation.

use std::env;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_unix;
use std::path::{Path, PathBuf};

/// Managed tools that can be bootstrapped
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ManagedTool {
    Fd,
    Rg,
}

impl ManagedTool {
    /// Get all managed tools
    pub fn all() -> &'static [ManagedTool] {
        &[ManagedTool::Fd, ManagedTool::Rg]
    }

    /// Get tool name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            ManagedTool::Fd => "fd",
            ManagedTool::Rg => "rg",
        }
    }
}

/// Tool specification for finding and provisioning
#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// Target name in the managed directory
    pub target_name: &'static str,
    /// Candidate names to search for in PATH
    pub candidates: &'static [&'static str],
}

/// Get tool specification for a managed tool
pub fn get_tool_spec(tool: ManagedTool) -> ToolSpec {
    match tool {
        ManagedTool::Fd => {
            if cfg!(windows) {
                ToolSpec {
                    target_name: "fd.exe",
                    candidates: &["fd.exe", "fd", "fdfind.exe", "fdfind"],
                }
            } else {
                ToolSpec {
                    target_name: "fd",
                    candidates: &["fd", "fdfind"],
                }
            }
        }
        ManagedTool::Rg => {
            if cfg!(windows) {
                ToolSpec {
                    target_name: "rg.exe",
                    candidates: &["rg.exe", "rg"],
                }
            } else {
                ToolSpec {
                    target_name: "rg",
                    candidates: &["rg"],
                }
            }
        }
    }
}

/// Split PATH environment variable into directories
///
/// # Arguments
/// * `path_value` - PATH string (or None to use env::var("PATH"))
///
/// # Returns
/// Vector of directory paths
///
/// # Examples
/// ```
/// use rustycode_orchestra::tool_bootstrap::split_path;
///
/// let dirs = split_path(Some("/usr/bin:/usr/local/bin"));
/// assert_eq!(dirs, vec!["/usr/bin", "/usr/local/bin"]);
/// ```
pub fn split_path(path_value: Option<String>) -> Vec<String> {
    let path_value = match path_value {
        Some(p) => p,
        None => match env::var("PATH") {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        },
    };

    let delimiter = if cfg!(windows) { ";" } else { ":" };
    path_value
        .split(delimiter)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Get candidate names for a tool (platform-specific)
///
/// On Windows, adds .exe, .cmd, .bat extensions if not present.
///
/// # Arguments
/// * `name` - Base tool name
///
/// # Returns
/// Vector of candidate names
///
/// # Examples
/// ```
/// use rustycode_orchestra::tool_bootstrap::get_candidate_names;
///
/// let names = get_candidate_names("fd");
/// // On Unix: ["fd"]
/// // On Windows: ["fd", "fd.exe", "fd.cmd", "fd.bat"]
/// ```
pub fn get_candidate_names(name: &str) -> Vec<String> {
    if !cfg!(windows) {
        return vec![name.to_string()];
    }

    let lower = name.to_lowercase();
    if lower.ends_with(".exe") || lower.ends_with(".cmd") || lower.ends_with(".bat") {
        return vec![name.to_string()];
    }

    vec![
        name.to_string(),
        format!("{}.exe", name),
        format!("{}.cmd", name),
        format!("{}.bat", name),
    ]
}

/// Check if a path is a regular file or symlink
///
/// # Arguments
/// * `path` - Path to check
///
/// # Returns
/// true if the path exists and is a regular file or symlink
///
/// # Examples
/// ```
/// use rustycode_orchestra::tool_bootstrap::is_regular_file;
///
/// if is_regular_file("/usr/bin/rg") {
///     println!("ripgrep exists");
/// }
/// ```
pub fn is_regular_file<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref();
    match fs::symlink_metadata(path) {
        Ok(metadata) => metadata.is_file() || metadata.file_type().is_symlink(),
        Err(_) => false,
    }
}

/// Resolve a tool from PATH
///
/// Searches for a tool in the PATH directories using candidate names.
///
/// # Arguments
/// * `tool` - Managed tool to find
/// * `path_value` - Optional PATH value (uses env::var("PATH") if None)
///
/// # Returns
/// Some(full_path) if found, None otherwise
///
/// # Examples
/// ```no_run
/// use rustycode_orchestra::tool_bootstrap::{resolve_tool_from_path, ManagedTool};
///
/// if let Some(path) = resolve_tool_from_path(ManagedTool::Rg, None) {
///     println!("Found ripgrep at: {}", path.display());
/// }
/// ```
pub fn resolve_tool_from_path(tool: ManagedTool, path_value: Option<String>) -> Option<PathBuf> {
    let spec = get_tool_spec(tool);

    for dir in split_path(path_value) {
        for candidate in spec.candidates {
            for name in get_candidate_names(candidate) {
                let full_path = PathBuf::from(&dir).join(&name);
                if full_path.exists() && is_regular_file(&full_path) {
                    return Some(full_path);
                }
            }
        }
    }

    None
}

/// Provision a tool to the target directory
///
/// Copies or symlinks the tool from source to target directory.
/// Tries symlink first, falls back to copy if symlink fails.
///
/// # Arguments
/// * `target_dir` - Target directory path
/// * `tool` - Managed tool to provision
/// * `source_path` - Path to the source tool binary
///
/// # Returns
/// Path to the provisioned tool
///
/// # Examples
/// ```no_run
/// use rustycode_orchestra::tool_bootstrap::{provision_tool, ManagedTool};
/// use std::path::PathBuf;
///
/// let source = PathBuf::from("/usr/bin/rg");
/// let target = provision_tool("/managed/tools", ManagedTool::Rg, &source);
/// ```
///
/// # Errors
/// Returns an error if:
/// - Target directory creation fails
/// - Copy operation fails
/// - Permission setting fails
pub fn provision_tool<P: AsRef<Path>>(target_dir: P, tool: ManagedTool, source_path: P) -> PathBuf {
    let target_dir = target_dir.as_ref();
    let source_path = source_path.as_ref();
    let spec = get_tool_spec(tool);
    let target_path = target_dir.join(spec.target_name);

    // Skip if already exists
    if target_path.exists() {
        return target_path;
    }

    // Create target directory
    fs::create_dir_all(target_dir).expect("Failed to create target directory");

    // Try symlink first (Unix only)
    #[cfg(unix)]
    {
        if symlink_unix(source_path, &target_path).is_ok() {
            return target_path;
        }
    }

    // Fallback to copy
    let _ = fs::remove_file(&target_path); // Remove if exists
    fs::copy(source_path, &target_path).expect("Failed to copy tool");

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&target_path)
            .expect("Failed to get metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target_path, perms).expect("Failed to set permissions");
    }

    target_path
}

/// Ensure all managed tools are provisioned to the target directory
///
/// Finds each tool in PATH and provisions it if not already present.
///
/// # Arguments
/// * `target_dir` - Target directory path
/// * `path_value` - Optional PATH value (uses env::var("PATH") if None)
///
/// # Returns
/// Vector of provisioned tool paths
///
/// # Examples
/// ```no_run
/// use rustycode_orchestra::tool_bootstrap::ensure_managed_tools;
///
/// let provisioned = ensure_managed_tools("/managed/tools", None);
/// println!("Provisioned {} tools", provisioned.len());
/// ```
pub fn ensure_managed_tools<P: AsRef<Path>>(
    target_dir: P,
    path_value: Option<String>,
) -> Vec<PathBuf> {
    let target_dir = target_dir.as_ref();
    let mut provisioned = Vec::new();

    for tool in ManagedTool::all() {
        let spec = get_tool_spec(*tool);
        let target_path = target_dir.join(spec.target_name);

        // Skip if already exists
        if target_path.exists() {
            continue;
        }

        // Try to find in PATH
        if let Some(source_path) = resolve_tool_from_path(*tool, path_value.clone()) {
            let provisioned_path = provision_tool(target_dir, *tool, &source_path);
            provisioned.push(provisioned_path);
        }
    }

    provisioned
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs::{self, File};

    use tempfile::TempDir;

    #[test]
    fn test_managed_tool_all() {
        let all = ManagedTool::all();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&ManagedTool::Fd));
        assert!(all.contains(&ManagedTool::Rg));
    }

    #[test]
    fn test_managed_tool_as_str() {
        assert_eq!(ManagedTool::Fd.as_str(), "fd");
        assert_eq!(ManagedTool::Rg.as_str(), "rg");
    }

    #[test]
    fn test_get_tool_spec_fd() {
        let spec = get_tool_spec(ManagedTool::Fd);
        assert_eq!(
            spec.target_name,
            if cfg!(windows) { "fd.exe" } else { "fd" }
        );
        assert!(!spec.candidates.is_empty());
    }

    #[test]
    fn test_get_tool_spec_rg() {
        let spec = get_tool_spec(ManagedTool::Rg);
        assert_eq!(
            spec.target_name,
            if cfg!(windows) { "rg.exe" } else { "rg" }
        );
        assert!(!spec.candidates.is_empty());
    }

    #[test]
    fn test_split_path_with_value() {
        let dirs = split_path(Some("/usr/bin:/usr/local/bin".to_string()));
        assert_eq!(dirs, vec!["/usr/bin", "/usr/local/bin"]);
    }

    #[test]
    fn test_split_path_empty() {
        let dirs = split_path(Some("".to_string()));
        assert!(dirs.is_empty());
    }

    #[test]
    fn test_split_path_none() {
        // Should use actual PATH env var if set
        let dirs = split_path(None);
        // Result depends on environment, just verify it returns a Vec
        let _ = dirs;
    }

    #[test]
    fn test_split_path_windows_style() {
        if cfg!(windows) {
            let dirs = split_path(Some("C:\\Tools;D:\\Tools".to_string()));
            assert_eq!(dirs, vec!["C:\\Tools", "D:\\Tools"]);
        } else {
            let dirs = split_path(Some("/usr/bin:/usr/local/bin".to_string()));
            assert_eq!(dirs, vec!["/usr/bin", "/usr/local/bin"]);
        }
    }

    #[test]
    fn test_get_candidate_names_unix() {
        let names = get_candidate_names("fd");
        assert_eq!(names, vec!["fd"]);
    }

    #[test]
    fn test_get_candidate_names_windows_with_extension() {
        let names = get_candidate_names("fd.exe");
        // Should not add duplicate extensions
        assert!(names.contains(&"fd.exe".to_string()));
    }

    #[test]
    fn test_is_regular_file_with_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path).expect("Failed to create file");

        assert!(is_regular_file(&file_path));
    }

    #[test]
    fn test_is_regular_file_nonexistent() {
        assert!(!is_regular_file("/nonexistent/path"));
    }

    #[test]
    fn test_resolve_tool_from_path_not_found() {
        let result = resolve_tool_from_path(ManagedTool::Rg, Some("/nonexistent".to_string()));
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_tool_from_path_with_candidates() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let bin_dir = temp_dir.path().join("bin");
        fs::create_dir(&bin_dir).expect("Failed to create bin dir");

        // Create a fake rg binary
        let rg_path = bin_dir.join("rg");
        File::create(&rg_path).expect("Failed to create rg");

        let path_value = format!("{}", bin_dir.display());

        let result = resolve_tool_from_path(ManagedTool::Rg, Some(path_value));
        assert!(result.is_some());
    }

    #[test]
    fn test_provision_tool_creates_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let target_dir = temp_dir.path().join("tools");
        let source_dir = temp_dir.path().join("source");
        fs::create_dir(&source_dir).expect("Failed to create source dir");

        // Create source binary
        let source_path = source_dir.join("rg");
        File::create(&source_path).expect("Failed to create source");

        let target_path = provision_tool(&target_dir, ManagedTool::Rg, &source_path);

        assert!(target_dir.exists());
        assert!(target_path.exists());
    }

    #[test]
    fn test_provision_tool_skips_existing() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let target_dir = temp_dir.path().join("tools");
        fs::create_dir(&target_dir).expect("Failed to create tools dir");

        let spec = get_tool_spec(ManagedTool::Rg);
        let target_path = target_dir.join(spec.target_name);
        File::create(&target_path).expect("Failed to create target");

        // Call provision_tool - should skip
        let source_path = temp_dir.path().join("source");
        let result = provision_tool(&target_dir, ManagedTool::Rg, &source_path);

        assert_eq!(result, target_path);
    }

    #[test]
    fn test_ensure_managed_tools_empty_path() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let target_dir = temp_dir.path().join("tools");

        let result = ensure_managed_tools(&target_dir, Some("".to_string()));

        assert!(result.is_empty());
    }

    #[test]
    fn test_ensure_managed_tools_with_tools() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let bin_dir = temp_dir.path().join("bin");
        fs::create_dir(&bin_dir).expect("Failed to create bin dir");
        let target_dir = temp_dir.path().join("tools");

        // Create fake binaries
        let rg_path = bin_dir.join("rg");
        File::create(&rg_path).expect("Failed to create rg");

        let fd_path = bin_dir.join("fd");
        File::create(&fd_path).expect("Failed to create fd");

        let path_value = format!("{}", bin_dir.display());
        let result = ensure_managed_tools(&target_dir, Some(path_value));

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_ensure_managed_tools_skips_existing() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let bin_dir = temp_dir.path().join("bin");
        fs::create_dir(&bin_dir).expect("Failed to create bin dir");
        let target_dir = temp_dir.path().join("tools");
        fs::create_dir(&target_dir).expect("Failed to create tools dir");

        // Create existing target
        let spec = get_tool_spec(ManagedTool::Rg);
        let target_path = target_dir.join(spec.target_name);
        File::create(&target_path).expect("Failed to create target");

        // Create source binary
        let rg_path = bin_dir.join("rg");
        File::create(&rg_path).expect("Failed to create rg");
        let fd_path = bin_dir.join("fd");
        File::create(&fd_path).expect("Failed to create fd");

        let path_value = format!("{}", bin_dir.display());
        let result = ensure_managed_tools(&target_dir, Some(path_value));

        // Should only provision fd (rg already exists)
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_ensure_managed_tools_partial() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let bin_dir = temp_dir.path().join("bin");
        fs::create_dir(&bin_dir).expect("Failed to create bin dir");
        let target_dir = temp_dir.path().join("tools");

        // Create only rg (not fd)
        let rg_path = bin_dir.join("rg");
        File::create(&rg_path).expect("Failed to create rg");

        let path_value = format!("{}", bin_dir.display());
        let result = ensure_managed_tools(&target_dir, Some(path_value));

        // Should only provision rg
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_managed_tool_hash_map() {
        let mut map = HashMap::new();
        map.insert(ManagedTool::Fd, "found");
        map.insert(ManagedTool::Rg, "found");

        assert_eq!(map.len(), 2);
        assert!(map.contains_key(&ManagedTool::Fd));
        assert!(map.contains_key(&ManagedTool::Rg));
    }

    #[test]
    fn test_get_candidate_names_multiple() {
        // On Windows, should return multiple candidates
        let names = get_candidate_names("rg");
        if cfg!(windows) {
            assert!(names.len() >= 2);
        } else {
            assert_eq!(names.len(), 1);
        }
    }
}
