//! Safe Filesystem Operations
//!
//! Provides safe wrappers around filesystem operations that return booleans
//! instead of panicking on errors.

use std::fs;
use std::path::Path;

/// Safely creates a directory
///
/// Returns true if successful, false on error.
///
/// # Arguments
/// * `dir_path` - Directory path to create
///
/// # Returns
/// true if directory was created or already exists, false on error
///
/// # Example
/// ```
/// use rustycode_orchestra::safe_fs::*;
///
/// let success = safe_mkdir("/path/to/dir");
/// ```
pub fn safe_mkdir(dir_path: &Path) -> bool {
    match fs::create_dir_all(dir_path) {
        Ok(()) => true,
        Err(e) => {
            tracing::debug!("safe_mkdir failed for {}: {}", dir_path.display(), e);
            false
        }
    }
}

/// Safely copies src to dst
///
/// Returns true if successful, false if src doesn't exist or copy fails.
///
/// # Arguments
/// * `src` - Source path
/// * `dst` - Destination path
///
/// # Returns
/// true if copy succeeded, false otherwise
///
/// # Example
/// ```
/// use rustycode_orchestra::safe_fs::*;
///
/// let success = safe_copy("/src/file.txt", "/dst/file.txt");
/// ```
pub fn safe_copy(src: &Path, dst: &Path) -> bool {
    if !src.exists() {
        return false;
    }
    match fs::copy(src, dst) {
        Ok(_) => true,
        Err(e) => {
            tracing::debug!(
                "safe_copy failed {} -> {}: {}",
                src.display(),
                dst.display(),
                e
            );
            false
        }
    }
}

/// Safely copies a directory recursively, creating the parent of dst if needed
///
/// Returns true if successful.
///
/// # Arguments
/// * `src` - Source directory path
/// * `dst` - Destination directory path
///
/// # Returns
/// true if recursive copy succeeded, false otherwise
///
/// # Example
/// ```
/// use rustycode_orchestra::safe_fs::*;
///
/// let success = safe_copy_recursive("/src/dir", "/dst/dir");
/// ```
pub fn safe_copy_recursive(src: &Path, dst: &Path) -> bool {
    if !src.exists() {
        return false;
    }

    // Create parent of dst if needed
    if let Some(parent) = dst.parent() {
        if !parent.exists() && fs::create_dir_all(parent).is_err() {
            tracing::debug!(
                "safe_copy_recursive: failed to create parent {}",
                parent.display()
            );
            return false;
        }
    }

    match recursive_copy(src, dst) {
        Ok(()) => true,
        Err(e) => {
            tracing::debug!(
                "safe_copy_recursive failed {} -> {}: {}",
                src.display(),
                dst.display(),
                e
            );
            false
        }
    }
}

/// Internal recursive copy implementation
fn recursive_copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let entry_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if entry_path.is_dir() {
                recursive_copy(&entry_path, &dst_path)?;
            } else {
                fs::copy(&entry_path, &dst_path)?;
            }
        }
    } else {
        fs::copy(src, dst)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_safe_mkdir() {
        let temp_dir = TempDir::new().unwrap();
        let new_dir = temp_dir.path().join("new_dir");

        assert!(!new_dir.exists());
        assert!(safe_mkdir(&new_dir));
        assert!(new_dir.exists());

        // Calling again should still return true
        assert!(safe_mkdir(&new_dir));
    }

    #[test]
    fn test_safe_mkdir_nested() {
        let temp_dir = TempDir::new().unwrap();
        let nested_dir = temp_dir.path().join("level1").join("level2").join("level3");

        assert!(!nested_dir.exists());
        assert!(safe_mkdir(&nested_dir));
        assert!(nested_dir.exists());
    }

    #[test]
    fn test_safe_copy() {
        let temp_dir = TempDir::new().unwrap();
        let src_file = temp_dir.path().join("src.txt");
        let dst_file = temp_dir.path().join("dst.txt");

        // Create source file
        fs::write(&src_file, "test content").unwrap();

        assert!(!dst_file.exists());
        assert!(safe_copy(&src_file, &dst_file));
        assert!(dst_file.exists());

        // Verify content
        let content = fs::read_to_string(&dst_file).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_safe_copy_nonexistent_source() {
        let temp_dir = TempDir::new().unwrap();
        let src_file = temp_dir.path().join("nonexistent.txt");
        let dst_file = temp_dir.path().join("dst.txt");

        assert!(!safe_copy(&src_file, &dst_file));
        assert!(!dst_file.exists());
    }

    #[test]
    fn test_safe_copy_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let src_dir = temp_dir.path().join("src");
        let dst_dir = temp_dir.path().join("dst");

        // Create source directory structure
        let sub_dir = src_dir.join("subdir");
        fs::create_dir_all(&sub_dir).unwrap();
        fs::write(sub_dir.join("file.txt"), "content").unwrap();

        assert!(!dst_dir.exists());
        assert!(safe_copy_recursive(&src_dir, &dst_dir));
        assert!(dst_dir.exists());

        // Verify structure was copied
        let dst_file = dst_dir.join("subdir").join("file.txt");
        assert!(dst_file.exists());
        let content = fs::read_to_string(&dst_file).unwrap();
        assert_eq!(content, "content");
    }

    #[test]
    fn test_safe_copy_recursive_nonexistent_source() {
        let temp_dir = TempDir::new().unwrap();
        let src_dir = temp_dir.path().join("nonexistent");
        let dst_dir = temp_dir.path().join("dst");

        assert!(!safe_copy_recursive(&src_dir, &dst_dir));
        assert!(!dst_dir.exists());
    }
}
