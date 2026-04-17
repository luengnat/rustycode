// rustycode-orchestra/src/migrate_external.rs
//! Orchestra External State Migration
//!
//! Migrates legacy in-project `.orchestra/` directories to the external
//! `~/.orchestra/projects/<hash>/` state directory. After migration, a
//! symlink replaces the original directory so all paths remain valid.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Result of migrating .orchestra directory to external state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalMigrationResult {
    pub migrated: bool,
    pub error: Option<String>,
}

/// Get the external Orchestra root path for a project.
///
/// Computes a hash of the project path to create a unique external directory.
fn external_orchestra_root(base_path: &Path) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Hash the base path to create a unique identifier
    let mut hasher = DefaultHasher::new();
    base_path.hash(&mut hasher);
    let hash = hasher.finish();

    // Use ~/.orchestra/projects/<hash>/ as external directory
    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home_dir
        .join(".orchestra")
        .join("projects")
        .join(format!("{:016x}", hash))
}

/// Migrate a legacy in-project `.orchestra/` directory to external storage.
///
/// Algorithm:
/// 1. If `<project>/.orchestra` is a symlink or doesn't exist -> skip
/// 2. If `<project>/.orchestra` is a real directory:
///    a. Compute external path from repo hash
///    b. mkdir -p external dir
///    c. Rename `.orchestra` -> `.orchestra.migrating` (atomic on same FS, acts as lock)
///    d. Copy contents to external dir (skip `worktrees/` subdirectory)
///    e. Create symlink `.orchestra -> external path`
///    f. Remove `.orchestra.migrating`
/// 3. On failure: rename `.orchestra.migrating` back to `.orchestra` (rollback)
///
/// # Arguments
/// * `base_path` - Path to the project root
///
/// # Returns
/// MigrationResult indicating success/failure
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::migrate_external::migrate_to_external_state;
/// use std::path::Path;
///
/// let result = migrate_to_external_state(Path::new("/project"));
/// if result.migrated {
///     println!("Successfully migrated to external state");
/// }
/// ```
pub fn migrate_to_external_state(base_path: &Path) -> ExternalMigrationResult {
    let local_orchestra = base_path.join(".orchestra");

    // Skip if doesn't exist
    if !local_orchestra.exists() {
        return ExternalMigrationResult {
            migrated: false,
            error: None,
        };
    }

    // Skip if already a symlink
    let metadata = match fs::symlink_metadata(&local_orchestra) {
        Ok(meta) => meta,
        Err(err) => {
            return ExternalMigrationResult {
                migrated: false,
                error: Some(format!("Cannot stat .orchestra: {}", err)),
            };
        }
    };

    if metadata.file_type().is_symlink() {
        return ExternalMigrationResult {
            migrated: false,
            error: None,
        };
    }

    if !metadata.is_dir() {
        return ExternalMigrationResult {
            migrated: false,
            error: Some(".orchestra exists but is not a directory or symlink".to_string()),
        };
    }

    let external_path = external_orchestra_root(base_path);
    let migrating_path = base_path.join(".orchestra.migrating");

    // Attempt migration with rollback on failure
    let migration_result = do_migration(&local_orchestra, &external_path, &migrating_path);

    // If migration failed, attempt rollback
    if let Some(ref error) = migration_result.error {
        rollback_migration(&local_orchestra, &migrating_path);
        return ExternalMigrationResult {
            migrated: false,
            error: Some(format!("Migration failed: {}", error)),
        };
    }

    migration_result
}

/// Perform the actual migration operation.
fn do_migration(
    local_orchestra: &Path,
    external_path: &Path,
    migrating_path: &Path,
) -> ExternalMigrationResult {
    // mkdir -p the external dir
    if let Err(err) = fs::create_dir_all(external_path) {
        return ExternalMigrationResult {
            migrated: false,
            error: Some(format!("Failed to create external dir: {}", err)),
        };
    }

    // Rename .orchestra -> .orchestra.migrating (atomic lock)
    if let Err(err) = fs::rename(local_orchestra, migrating_path) {
        return ExternalMigrationResult {
            migrated: false,
            error: Some(format!("Failed to create .orchestra.migrating: {}", err)),
        };
    }

    // Copy contents to external dir, skipping worktrees/
    let entries = match fs::read_dir(migrating_path) {
        Ok(entries) => entries,
        Err(err) => {
            // Rollback
            let _ = fs::rename(migrating_path, local_orchestra);
            return ExternalMigrationResult {
                migrated: false,
                error: Some(format!("Failed to read .orchestra.migrating: {}", err)),
            };
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // Non-fatal: skip this entry
        };

        let name = entry.file_name();
        let name_str = match name.to_str() {
            Some(s) => s,
            None => continue, // Skip invalid UTF-8
        };

        // Skip worktrees directory (stays local)
        if name_str == "worktrees" {
            continue;
        }

        let src = migrating_path.join(&name);
        let dst = external_path.join(&name);

        // Copy entry (non-fatal: continue on error)
        let _ = copy_entry(&src, &dst);
    }

    // Create symlink .orchestra -> external path
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        if let Err(err) = symlink(external_path, local_orchestra) {
            return ExternalMigrationResult {
                migrated: false,
                error: Some(format!("Failed to create symlink: {}", err)),
            };
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_dir;
        if let Err(err) = symlink_dir(external_path, local_orchestra) {
            return ExternalMigrationResult {
                migrated: false,
                error: Some(format!("Failed to create symlink: {}", err)),
            };
        }
    }

    // Remove .orchestra.migrating
    let _ = fs::remove_dir_all(migrating_path);

    ExternalMigrationResult {
        migrated: true,
        error: None,
    }
}

/// Copy a file or directory recursively.
fn copy_entry(src: &Path, dst: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(src)?;

    if metadata.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = src.join(entry.file_name());
            let dst_path = dst.join(entry.file_name());
            copy_entry(&src_path, &dst_path)?;
        }
    } else {
        fs::copy(src, dst)?;
    }

    Ok(())
}

/// Rollback a failed migration by renaming .orchestra.migrating back to .orchestra.
fn rollback_migration(local_orchestra: &Path, migrating_path: &Path) {
    if migrating_path.exists() && !local_orchestra.exists() {
        let _ = fs::rename(migrating_path, local_orchestra);
    }
}

/// Recover from a failed migration (`.orchestra.migrating` exists).
///
/// Moves `.orchestra.migrating` back to `.orchestra` if `.orchestra` doesn't exist.
///
/// # Arguments
/// * `base_path` - Path to the project root
///
/// # Returns
/// true if recovery was performed, false otherwise
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::migrate_external::recover_failed_migration;
/// use std::path::Path;
///
/// let recovered = recover_failed_migration(Path::new("/project"));
/// if recovered {
///     println!("Recovered from failed migration");
/// }
/// ```
pub fn recover_failed_migration(base_path: &Path) -> bool {
    let local_orchestra = base_path.join(".orchestra");
    let migrating_path = base_path.join(".orchestra.migrating");

    if !migrating_path.exists() {
        return false;
    }
    if local_orchestra.exists() {
        return false; // Both exist -- ambiguous, don't touch
    }

    fs::rename(&migrating_path, &local_orchestra).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_migration_result_success() {
        let result = ExternalMigrationResult {
            migrated: true,
            error: None,
        };
        assert!(result.migrated);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_migration_result_failure() {
        let result = ExternalMigrationResult {
            migrated: false,
            error: Some("Test error".to_string()),
        };
        assert!(!result.migrated);
        assert_eq!(result.error, Some("Test error".to_string()));
    }

    #[test]
    fn test_migrate_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let result = migrate_to_external_state(temp_dir.path());

        assert!(!result.migrated);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_migrate_already_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let external = temp_dir.path().join(".orchestra.external");
        let local_orchestra = temp_dir.path().join(".orchestra");

        fs::create_dir_all(&external).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&external, &local_orchestra).unwrap();
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_dir;
            symlink_dir(&external, &local_orchestra).unwrap();
        }

        let result = migrate_to_external_state(temp_dir.path());

        assert!(!result.migrated);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_migrate_file_instead_of_directory() {
        let temp_dir = TempDir::new().unwrap();
        let local_orchestra = temp_dir.path().join(".orchestra");

        fs::write(&local_orchestra, "not a directory").unwrap();

        let result = migrate_to_external_state(temp_dir.path());

        assert!(!result.migrated);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("not a directory"));
    }

    #[test]
    fn test_recover_failed_migration_no_migrating_dir() {
        let temp_dir = TempDir::new().unwrap();
        let recovered = recover_failed_migration(temp_dir.path());

        assert!(!recovered);
    }

    #[test]
    fn test_recover_failed_migration_both_exist() {
        let temp_dir = TempDir::new().unwrap();
        let local_orchestra = temp_dir.path().join(".orchestra");
        let migrating_path = temp_dir.path().join(".orchestra.migrating");

        fs::create_dir(&local_orchestra).unwrap();
        fs::create_dir(&migrating_path).unwrap();

        let recovered = recover_failed_migration(temp_dir.path());

        assert!(!recovered); // Ambiguous - both exist
        assert!(local_orchestra.exists());
        assert!(migrating_path.exists());
    }

    #[test]
    fn test_recover_failed_migration_successful() {
        let temp_dir = TempDir::new().unwrap();
        let migrating_path = temp_dir.path().join(".orchestra.migrating");

        fs::create_dir(&migrating_path).unwrap();

        let recovered = recover_failed_migration(temp_dir.path());

        assert!(recovered);
        assert!(!migrating_path.exists());
        assert!(temp_dir.path().join(".orchestra").exists());
    }

    #[test]
    fn test_external_orchestra_root_is_consistent() {
        let path = Path::new("/test/project");
        let root1 = external_orchestra_root(path);
        let root2 = external_orchestra_root(path);

        assert_eq!(root1, root2);
    }

    #[test]
    fn test_external_orchestra_root_different_paths() {
        let path1 = Path::new("/test/project1");
        let path2 = Path::new("/test/project2");
        let root1 = external_orchestra_root(path1);
        let root2 = external_orchestra_root(path2);

        assert_ne!(root1, root2);
    }

    #[test]
    fn test_copy_entry_file() {
        let temp_dir = TempDir::new().unwrap();
        let src = temp_dir.path().join("test.txt");
        let dst = temp_dir.path().join("copy.txt");

        fs::write(&src, "test content").unwrap();

        assert!(copy_entry(&src, &dst).is_ok());
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "test content");
    }

    #[test]
    fn test_copy_entry_directory() {
        let temp_dir = TempDir::new().unwrap();
        let src = temp_dir.path().join("src_dir");
        let dst = temp_dir.path().join("dst_dir");

        fs::create_dir(&src).unwrap();
        fs::write(src.join("file.txt"), "content").unwrap();

        assert!(copy_entry(&src, &dst).is_ok());
        assert!(dst.exists());
        assert!(dst.join("file.txt").exists());
    }
}
