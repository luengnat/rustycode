//! Extension discovery — finding extension entry points in directories.
//!
//! Discovers extension entry-point files by scanning directories and resolving
//! package.json manifests.
//!
//! Matches orchestra-2's extension-discovery.ts implementation.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Check if a filename is an extension file (.ts or .js)
///
/// # Arguments
/// * `name` - Filename to check
///
/// # Returns
/// true if the filename ends with .ts or .js
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_discovery::is_extension_file;
///
/// assert!(is_extension_file("index.ts"));
/// assert!(is_extension_file("extension.js"));
/// assert!(!is_extension_file("README.md"));
/// assert!(!is_extension_file("data.json"));
/// ```
pub fn is_extension_file(name: &str) -> bool {
    name.ends_with(".ts") || name.ends_with(".js")
}

/// Resolves the entry-point file(s) for a single extension directory.
///
/// # Resolution Strategy
/// 1. If the directory contains a package.json with a `pi.extensions` array,
///    each entry is resolved relative to the directory and returned (if it exists).
/// 2. Otherwise falls back to `index.ts` → `index.js`.
///
/// # Arguments
/// * `dir` - Directory path to resolve
///
/// # Returns
/// Vector of resolved entry-point file paths (empty if none found)
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_discovery::resolve_extension_entries;
///
/// // Directory with package.json
/// let entries = resolve_extension_entries("/path/to/extension");
/// // Returns paths declared in package.json pi.extensions
///
/// // Directory with index.ts
/// let entries = resolve_extension_entries("/path/to/extension");
/// // Returns ["/path/to/extension/index.ts"]
/// ```
///
/// # Errors
/// Returns an empty vector if:
/// - Directory doesn't exist
/// - No entry points are found
/// - package.json is malformed (falls back to index.ts/index.js)
pub fn resolve_extension_entries<P: AsRef<Path>>(dir: P) -> Vec<PathBuf> {
    let dir = dir.as_ref();
    let package_json_path = dir.join("package.json");

    // Try package.json first
    if package_json_path.exists() {
        if let Ok(content) = fs::read_to_string(&package_json_path) {
            if let Ok(pkg) = serde_json::from_str::<Value>(&content) {
                if let Some(declared) = pkg
                    .get("pi")
                    .and_then(|pi| pi.get("extensions"))
                    .and_then(|ext| ext.as_array())
                {
                    let resolved: Vec<PathBuf> = declared
                        .iter()
                        .filter_map(|entry| entry.as_str())
                        .map(|entry| dir.join(entry))
                        .filter(|entry| entry.exists())
                        .collect();

                    if !resolved.is_empty() {
                        return resolved;
                    }
                }
            }
        }
        // If package.json exists but is malformed or has no valid entries,
        // fall through to index.ts/index.js discovery
    }

    // Fallback to index.ts
    let index_ts = dir.join("index.ts");
    if index_ts.exists() {
        return vec![index_ts];
    }

    // Fallback to index.js
    let index_js = dir.join("index.js");
    if index_js.exists() {
        return vec![index_js];
    }

    // No entry points found
    Vec::new()
}

/// Discovers all extension entry-point paths under an extensions directory.
///
/// # Discovery Strategy
/// - Top-level .ts/.js files are treated as standalone extension entry points.
/// - Subdirectories are resolved via `resolve_extension_entries()` (package.json →
///   pi.extensions, then index.ts/index.js fallback).
///
/// # Arguments
/// * `extensions_dir` - Path to extensions directory
///
/// # Returns
/// Vector of discovered entry-point file paths (empty if directory doesn't exist)
///
/// # Examples
/// ```
/// use rustycode_orchestra::extension_discovery::discover_extension_entry_paths;
///
/// let entries = discover_extension_entry_paths("/path/to/extensions");
/// // Returns all discovered .ts/.js entry points
/// ```
///
/// # Errors
/// Returns an empty vector if:
/// - Directory doesn't exist
/// - No extension files are found
/// - Subdirectories contain no valid entry points
pub fn discover_extension_entry_paths<P: AsRef<Path>>(extensions_dir: P) -> Vec<PathBuf> {
    let extensions_dir = extensions_dir.as_ref();

    if !extensions_dir.exists() {
        return Vec::new();
    }

    let mut discovered = Vec::new();

    let entries = match fs::read_dir(extensions_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let entry_path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        // Handle files and symlinks
        if file_type.is_file() || file_type.is_symlink() {
            if let Some(name) = entry_path.file_name() {
                if let Some(name_str) = name.to_str() {
                    if is_extension_file(name_str) {
                        discovered.push(entry_path);
                        continue;
                    }
                }
            }
        }

        // Handle directories and symlinks to directories
        if file_type.is_dir() || file_type.is_symlink() {
            discovered.extend(resolve_extension_entries(&entry_path));
        }
    }

    discovered
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_is_extension_file_typescript() {
        assert!(is_extension_file("index.ts"));
        assert!(is_extension_file("extension.ts"));
        assert!(is_extension_file("my-extension.ts"));
    }

    #[test]
    fn test_is_extension_file_javascript() {
        assert!(is_extension_file("index.js"));
        assert!(is_extension_file("extension.js"));
        assert!(is_extension_file("my-extension.js"));
    }

    #[test]
    fn test_is_extension_file_false_for_others() {
        assert!(!is_extension_file("README.md"));
        assert!(!is_extension_file("package.json"));
        assert!(!is_extension_file("data.txt"));
        assert!(!is_extension_file("config.yml"));
    }

    #[test]
    fn test_resolve_extension_entries_index_ts() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        let index_ts = ext_dir.join("index.ts");
        File::create(&index_ts).expect("Failed to create index.ts");

        let entries = resolve_extension_entries(&ext_dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], index_ts);
    }

    #[test]
    fn test_resolve_extension_entries_index_js() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        let index_js = ext_dir.join("index.js");
        File::create(&index_js).expect("Failed to create index.js");

        let entries = resolve_extension_entries(&ext_dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], index_js);
    }

    #[test]
    fn test_resolve_extension_entries_index_ts_preferred_over_js() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        let index_ts = ext_dir.join("index.ts");
        let index_js = ext_dir.join("index.js");
        File::create(&index_ts).expect("Failed to create index.ts");
        File::create(&index_js).expect("Failed to create index.js");

        let entries = resolve_extension_entries(&ext_dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], index_ts);
    }

    #[test]
    fn test_resolve_extension_entries_empty_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        let entries = resolve_extension_entries(&ext_dir);
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_resolve_extension_entries_nonexistent_directory() {
        let entries = resolve_extension_entries("/nonexistent/path");
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_resolve_extension_entries_with_package_json() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        // Create entry files
        let entry1 = ext_dir.join("entry1.ts");
        let entry2 = ext_dir.join("entry2.ts");
        File::create(&entry1).expect("Failed to create entry1.ts");
        File::create(&entry2).expect("Failed to create entry2.ts");

        // Create package.json
        let package_json = ext_dir.join("package.json");
        let mut file = File::create(&package_json).expect("Failed to create package.json");
        file.write_all(br#"{"pi":{"extensions":["entry1.ts","entry2.ts"]}}"#)
            .expect("Failed to write package.json");

        let entries = resolve_extension_entries(&ext_dir);
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&entry1));
        assert!(entries.contains(&entry2));
    }

    #[test]
    fn test_resolve_extension_entries_package_json_filters_nonexistent() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        // Create only one entry file
        let entry1 = ext_dir.join("entry1.ts");
        File::create(&entry1).expect("Failed to create entry1.ts");

        // Create package.json with two entries (one doesn't exist)
        let package_json = ext_dir.join("package.json");
        let mut file = File::create(&package_json).expect("Failed to create package.json");
        file.write_all(br#"{"pi":{"extensions":["entry1.ts","nonexistent.ts"]}}"#)
            .expect("Failed to write package.json");

        let entries = resolve_extension_entries(&ext_dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], entry1);
    }

    #[test]
    fn test_resolve_extension_entries_package_json_empty_array() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        // Create package.json with empty array
        let package_json = ext_dir.join("package.json");
        let mut file = File::create(&package_json).expect("Failed to create package.json");
        file.write_all(br#"{"pi":{"extensions":[]}}"#)
            .expect("Failed to write package.json");

        // Create index.ts as fallback
        let index_ts = ext_dir.join("index.ts");
        File::create(&index_ts).expect("Failed to create index.ts");

        let entries = resolve_extension_entries(&ext_dir);
        // Empty array should fall through to index.ts
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], index_ts);
    }

    #[test]
    fn test_resolve_extension_entries_malformed_package_json() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        // Create malformed package.json
        let package_json = ext_dir.join("package.json");
        let mut file = File::create(&package_json).expect("Failed to create package.json");
        file.write_all(b"{invalid json")
            .expect("Failed to write package.json");

        // Create index.ts as fallback
        let index_ts = ext_dir.join("index.ts");
        File::create(&index_ts).expect("Failed to create index.ts");

        let entries = resolve_extension_entries(&ext_dir);
        // Malformed JSON should fall through to index.ts
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], index_ts);
    }

    #[test]
    fn test_discover_extension_entry_paths_nonexistent_directory() {
        let entries = discover_extension_entry_paths("/nonexistent/path");
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_discover_extension_entry_paths_empty_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extensions");
        fs::create_dir(&ext_dir).expect("Failed to create extensions dir");

        let entries = discover_extension_entry_paths(&ext_dir);
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_discover_extension_entry_paths_top_level_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extensions");
        fs::create_dir(&ext_dir).expect("Failed to create extensions dir");

        // Create top-level extension files
        let ext1 = ext_dir.join("ext1.ts");
        let ext2 = ext_dir.join("ext2.js");
        File::create(&ext1).expect("Failed to create ext1.ts");
        File::create(&ext2).expect("Failed to create ext2.js");

        // Create non-extension file (should be ignored)
        let readme = ext_dir.join("README.md");
        File::create(&readme).expect("Failed to create README.md");

        let entries = discover_extension_entry_paths(&ext_dir);
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&ext1));
        assert!(entries.contains(&ext2));
        assert!(!entries.contains(&readme));
    }

    #[test]
    fn test_discover_extension_entry_paths_subdirectories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extensions");
        fs::create_dir(&ext_dir).expect("Failed to create extensions dir");

        // Create subdirectory with index.ts
        let subdir = ext_dir.join("my-extension");
        fs::create_dir(&subdir).expect("Failed to create subdirectory");
        let index_ts = subdir.join("index.ts");
        File::create(&index_ts).expect("Failed to create index.ts");

        let entries = discover_extension_entry_paths(&ext_dir);
        assert_eq!(entries.len(), 1);
        assert!(entries.contains(&index_ts));
    }

    #[test]
    fn test_discover_extension_entry_paths_mixed() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extensions");
        fs::create_dir(&ext_dir).expect("Failed to create extensions dir");

        // Create top-level file
        let top_level = ext_dir.join("top.ts");
        File::create(&top_level).expect("Failed to create top.ts");

        // Create subdirectory with index.ts
        let subdir = ext_dir.join("my-extension");
        fs::create_dir(&subdir).expect("Failed to create subdirectory");
        let index_ts = subdir.join("index.ts");
        File::create(&index_ts).expect("Failed to create index.ts");

        let entries = discover_extension_entry_paths(&ext_dir);
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&top_level));
        assert!(entries.contains(&index_ts));
    }

    #[test]
    fn test_discover_extension_entry_paths_symlinks_to_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extensions");
        fs::create_dir(&ext_dir).expect("Failed to create extensions dir");

        // Create a file outside extensions dir
        let external_file = temp_dir.path().join("external.ts");
        File::create(&external_file).expect("Failed to create external.ts");

        // Create symlink to the file
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let symlink_path = ext_dir.join("linked.ts");
            symlink(&external_file, &symlink_path).expect("Failed to create symlink");

            let entries = discover_extension_entry_paths(&ext_dir);
            assert_eq!(entries.len(), 1);
        }
        #[cfg(windows)]
        {
            // Symlink creation on Windows requires different handling
            // Skip this test on Windows
        }
    }

    #[test]
    fn test_resolve_extension_entries_with_path_buf() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir: PathBuf = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        let index_ts = ext_dir.join("index.ts");
        File::create(&index_ts).expect("Failed to create index.ts");

        let entries = resolve_extension_entries(&ext_dir);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_resolve_extension_entries_with_str() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        let index_ts = ext_dir.join("index.ts");
        File::create(&index_ts).expect("Failed to create index.ts");

        let entries = resolve_extension_entries(ext_dir.to_str().unwrap());
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_resolve_extension_entries_package_json_relative_paths() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let ext_dir = temp_dir.path().join("extension");
        fs::create_dir(&ext_dir).expect("Failed to create extension dir");

        // Create subdirectory with entry file
        let src_dir = ext_dir.join("src");
        fs::create_dir(&src_dir).expect("Failed to create src dir");
        let entry_ts = src_dir.join("entry.ts");
        File::create(&entry_ts).expect("Failed to create entry.ts");

        // Create package.json with relative path
        let package_json = ext_dir.join("package.json");
        let mut file = File::create(&package_json).expect("Failed to create package.json");
        file.write_all(br#"{"pi":{"extensions":["./src/entry.ts"]}}"#)
            .expect("Failed to write package.json");

        let entries = resolve_extension_entries(&ext_dir);
        assert_eq!(entries.len(), 1);
        assert!(entries.contains(&entry_ts));
    }
}
