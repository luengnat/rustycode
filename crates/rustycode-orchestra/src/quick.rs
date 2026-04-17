// rustycode-orchestra/src/quick.rs
//! Orchestra Quick Mode — /orchestra quick <task>
//!
//! Lightweight task execution with Orchestra guarantees (atomic commits, state
//! tracking) but without the full milestone/slice ceremony.
//!
//! Quick tasks live in `.orchestra/quick/` and are tracked in STATE.md's
//! "Quick Tasks Completed" table.

use std::fs;
use std::path::{Path, PathBuf};

/// Generate a URL-friendly slug from a description.
///
/// Converts to lowercase, replaces non-alphanumeric characters with hyphens,
/// and limits to 40 characters.
///
/// # Arguments
/// * `text` - Description text to slugify
///
/// # Returns
/// URL-friendly slug (lowercase, hyphens, max 40 chars)
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::quick::slugify;
///
/// assert_eq!(slugify("Fix login button"), "fix-login-button");
/// assert_eq!(slugify("Test@#$%API"), "test-api");
/// assert!(slugify("a very long description that should be truncated").len() <= 40);
/// ```
pub fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(40)
        .collect()
}

/// Determine the next quick task number by scanning existing directories.
///
/// Scans the quick directory for existing task directories (format: NNN-slug)
/// and returns the next sequential number.
///
/// # Arguments
/// * `quick_dir` - Path to the quick tasks directory
///
/// # Returns
/// Next task number (1 if no existing tasks found)
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::quick::get_next_task_num;
/// use std::path::Path;
///
/// // Empty directory returns 1
/// assert_eq!(get_next_task_num(Path::new("/nonexistent")), 1);
/// ```
pub fn get_next_task_num(quick_dir: &Path) -> u32 {
    if !quick_dir.exists() {
        return 1;
    }

    let entries = match fs::read_dir(quick_dir) {
        Ok(entries) => entries,
        Err(_) => return 1,
    };

    let mut max_num = 0u32;

    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name_str = match name.to_str() {
            Some(s) => s,
            None => continue,
        };

        // Parse "NNN-slug" format
        if let Some(num_str) = name_str.split('-').next() {
            if let Ok(num) = num_str.parse::<u32>() {
                if num > max_num {
                    max_num = num;
                }
            }
        }
    }

    max_num + 1
}

/// Ensure the quick task directory structure exists.
///
/// Creates the quick task directory if it doesn't exist.
///
/// # Arguments
/// * `orchestra_root` - Path to the .orchestra directory
/// * `task_num` - Task number
/// * `slug` - Task slug
///
/// # Returns
/// Path to the created task directory
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::quick::ensure_quick_dir;
/// use std::path::Path;
///
/// let temp_dir = tempfile::tempdir().unwrap();
/// let task_dir = ensure_quick_dir(temp_dir.path(), 1, "test-task");
/// assert!(task_dir.exists());
/// assert!(task_dir.ends_with("quick/001-test-task"));
/// ```
pub fn ensure_quick_dir(orchestra_root: &Path, task_num: u32, slug: &str) -> PathBuf {
    let quick_dir = orchestra_root.join("quick");
    let task_dir = quick_dir.join(format!("{:03}-{}", task_num, slug));

    fs::create_dir_all(&task_dir).unwrap_or_else(|e| {
        tracing::warn!("Failed to create quick task directory: {}", e);
    });

    task_dir
}

/// Get the relative path to a quick task directory from project root.
///
/// # Arguments
/// * `task_num` - Task number
/// * `slug` - Task slug
///
/// # Returns
/// Relative path string (e.g., ".orchestra/quick/001-test-task")
pub fn get_quick_task_rel_path(task_num: u32, slug: &str) -> String {
    format!(".orchestra/quick/{:03}-{}", task_num, slug)
}

/// Generate branch name for a quick task.
///
/// # Arguments
/// * `task_num` - Task number
/// * `slug` - Task slug
///
/// # Returns
/// Branch name (e.g., "orchestra/quick/001-test-task")
pub fn get_quick_task_branch_name(task_num: u32, slug: &str) -> String {
    format!("orchestra/quick/{:03}-{}", task_num, slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Fix login button"), "fix-login-button");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("Test@#$%API"), "test-api");
    }

    #[test]
    fn test_slugify_multiple_spaces() {
        assert_eq!(slugify("test   multiple   spaces"), "test-multiple-spaces");
    }

    #[test]
    fn test_slugify_underscores() {
        assert_eq!(slugify("test_underscore_var"), "test-underscore-var");
    }

    #[test]
    fn test_slugify_truncation() {
        let long = "a very long description that should be truncated to forty characters";
        let slug = slugify(long);
        assert!(slug.len() <= 40);
    }

    #[test]
    fn test_slugify_empty() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn test_slugify_leading_trailing_hyphens() {
        assert_eq!(slugify("---test---"), "test");
    }

    #[test]
    fn test_get_next_task_num_no_directory() {
        let result = get_next_task_num(Path::new("/nonexistent"));
        assert_eq!(result, 1);
    }

    #[test]
    fn test_get_next_task_num_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let quick_dir = temp_dir.path().join("quick");
        fs::create_dir(&quick_dir).unwrap();

        let result = get_next_task_num(&quick_dir);
        assert_eq!(result, 1);
    }

    #[test]
    fn test_get_next_task_num_existing_tasks() {
        let temp_dir = TempDir::new().unwrap();
        let quick_dir = temp_dir.path().join("quick");
        fs::create_dir(&quick_dir).unwrap();

        // Create existing task directories
        fs::create_dir(quick_dir.join("001-task-one")).unwrap();
        fs::create_dir(quick_dir.join("002-task-two")).unwrap();
        fs::create_dir(quick_dir.join("005-task-five")).unwrap();

        let result = get_next_task_num(&quick_dir);
        assert_eq!(result, 6); // Max is 5, so next is 6
    }

    #[test]
    fn test_get_next_task_num_non_numeric_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let quick_dir = temp_dir.path().join("quick");
        fs::create_dir(&quick_dir).unwrap();

        // Create directories without numeric prefixes
        fs::create_dir(quick_dir.join("random-task")).unwrap();
        fs::create_dir(quick_dir.join("another-task")).unwrap();

        let result = get_next_task_num(&quick_dir);
        assert_eq!(result, 1); // No numeric tasks found
    }

    #[test]
    fn test_ensure_quick_dir_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_root = temp_dir.path();

        let task_dir = ensure_quick_dir(orchestra_root, 1, "test-task");

        assert!(task_dir.exists());
        assert!(task_dir.ends_with("quick/001-test-task"));
    }

    #[test]
    fn test_ensure_quick_dir_existing_quick_dir() {
        let temp_dir = TempDir::new().unwrap();
        let orchestra_root = temp_dir.path();
        let quick_dir = orchestra_root.join("quick");
        fs::create_dir(&quick_dir).unwrap();

        let task_dir = ensure_quick_dir(orchestra_root, 1, "test-task");

        assert!(task_dir.exists());
        assert!(quick_dir.exists());
    }

    #[test]
    fn test_get_quick_task_rel_path() {
        let path = get_quick_task_rel_path(1, "test-task");
        assert_eq!(path, ".orchestra/quick/001-test-task");
    }

    #[test]
    fn test_get_quick_task_rel_path_high_number() {
        let path = get_quick_task_rel_path(999, "important-fix");
        assert_eq!(path, ".orchestra/quick/999-important-fix");
    }

    #[test]
    fn test_get_quick_task_branch_name() {
        let branch = get_quick_task_branch_name(1, "test-task");
        assert_eq!(branch, "orchestra/quick/001-test-task");
    }

    #[test]
    fn test_get_quick_task_branch_name_high_number() {
        let branch = get_quick_task_branch_name(42, "critical-bug");
        assert_eq!(branch, "orchestra/quick/042-critical-bug");
    }

    #[test]
    fn test_slugify_preserves_numbers() {
        assert_eq!(slugify("Fix API v2 endpoint"), "fix-api-v2-endpoint");
    }

    #[test]
    fn test_slugify_consecutive_special_chars() {
        assert_eq!(slugify("test!!!@@@###api"), "test-api");
    }

    #[test]
    fn test_get_next_task_num_gaps() {
        let temp_dir = TempDir::new().unwrap();
        let quick_dir = temp_dir.path().join("quick");
        fs::create_dir(&quick_dir).unwrap();

        // Create tasks with gaps
        fs::create_dir(quick_dir.join("001-first")).unwrap();
        fs::create_dir(quick_dir.join("005-fifth")).unwrap();
        fs::create_dir(quick_dir.join("010-tenth")).unwrap();

        let result = get_next_task_num(&quick_dir);
        assert_eq!(result, 11); // Max is 10, so next is 11
    }

    #[test]
    fn test_get_next_task_num_mixed_files_and_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let quick_dir = temp_dir.path().join("quick");
        fs::create_dir(&quick_dir).unwrap();

        // Create mix of files and directories
        fs::create_dir(quick_dir.join("001-task")).unwrap();
        fs::write(quick_dir.join("README.md"), "text").unwrap();
        fs::create_dir(quick_dir.join("002-another")).unwrap();

        let result = get_next_task_num(&quick_dir);
        assert_eq!(result, 3); // Only count directories
    }
}
