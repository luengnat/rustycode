//! Orchestra Atomic Write — Crash-Safe File Operations
//!
//! Provides atomic file writes by writing to a temp file first,
//! then renaming. Prevents partial/corrupt files on crash.
//! Matches orchestra-2's atomic-write.ts implementation.
//!
//! Critical for production autonomous systems to ensure state files
//! are never corrupted due to crashes or interruptions.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Atomically writes content to a file
///
/// # Process
/// 1. Create parent directory if it doesn't exist
/// 2. Write content to a temporary file with random suffix
/// 3. Rename temp file to target path (atomic on POSIX systems)
/// 4. Clean up temp file if rename fails
///
/// # Arguments
/// * `file_path` - Target file path
/// * `content` - Content to write
///
/// # Why Atomic?
/// - Direct write: if crash occurs during write → file is corrupt
/// - Atomic write: if crash occurs → either old file or new file, never corrupt
/// - Rename is atomic on POSIX (Linux, macOS)
///
/// # Example
/// ```rust,no_run
/// use std::path::Path;
/// use rustycode_orchestra::atomic_write;
///
/// atomic_write(Path::new("/path/to/state.json"), "{\"status\": \"active\"}")
///     .expect("Failed to write state");
/// ```
pub fn atomic_write(file_path: &Path, content: &str) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = file_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }
    }

    // Generate random suffix for temp file
    let random_suffix = generate_random_suffix();
    let tmp_path = file_path.with_extension(format!("tmp.{}", random_suffix));

    // Write content to temp file
    fs::write(&tmp_path, content)
        .with_context(|| format!("Failed to write to temp file: {:?}", tmp_path))?;

    // Atomic rename from temp to target
    match fs::rename(&tmp_path, file_path) {
        Ok(_) => Ok(()),
        Err(e) => {
            // Clean up orphan temp file
            let _ = fs::remove_file(&tmp_path);
            Err(e).with_context(|| format!("Failed to rename {:?} to {:?}", tmp_path, file_path))?
        }
    }
}

/// Atomically writes content to a file (async version)
///
/// This is the async variant of `atomic_write`. In Rust, we use
/// tokio::fs for async file operations.
///
/// # Arguments
/// * `file_path` - Target file path
/// * `content` - Content to write
///
/// # Example
/// ```rust,no_run
/// use std::path::Path;
/// use rustycode_orchestra::atomic_write_async;
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// atomic_write_async(Path::new("/path/to/state.json"), "{\"status\": \"active\"}")
///     .await
///     .expect("Failed to write state");
/// # Ok(())
/// # }
/// ```
pub async fn atomic_write_async(file_path: &Path, content: &str) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = file_path.parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }
    }

    // Generate random suffix for temp file
    let random_suffix = generate_random_suffix();
    let tmp_path = file_path.with_extension(format!("tmp.{}", random_suffix));

    // Write content to temp file
    tokio::fs::write(&tmp_path, content)
        .await
        .with_context(|| format!("Failed to write to temp file: {:?}", tmp_path))?;

    // Atomic rename from temp to target
    match tokio::fs::rename(&tmp_path, file_path).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // Clean up orphan temp file
            let _ = tokio::fs::remove_file(&tmp_path).await;
            Err(e).with_context(|| format!("Failed to rename {:?} to {:?}", tmp_path, file_path))?
        }
    }
}

/// Atomically writes bytes to a file
///
/// Same as `atomic_write` but takes `&[u8]` instead of `&str`.
pub fn atomic_write_bytes(file_path: &Path, content: &[u8]) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = file_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }
    }

    // Generate random suffix for temp file
    let random_suffix = generate_random_suffix();
    let tmp_path = file_path.with_extension(format!("tmp.{}", random_suffix));

    // Write content to temp file
    fs::write(&tmp_path, content)
        .with_context(|| format!("Failed to write to temp file: {:?}", tmp_path))?;

    // Atomic rename from temp to target
    match fs::rename(&tmp_path, file_path) {
        Ok(_) => Ok(()),
        Err(e) => {
            // Clean up orphan temp file
            let _ = fs::remove_file(&tmp_path);
            Err(e).with_context(|| format!("Failed to rename {:?} to {:?}", tmp_path, file_path))?
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────────

/// Generate a random 8-character suffix for temp files
///
/// Uses the first 8 characters of a UUID v4 as the random suffix.
/// This provides sufficient randomness to avoid collisions.
fn generate_random_suffix() -> String {
    uuid::Uuid::new_v4()
        .to_string()
        .split('-')
        .next()
        .unwrap_or("xxxxxxx")
        .to_string()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_atomic_write_basic() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Hello, World!";

        atomic_write(&file_path, content).unwrap();

        // Verify file was written
        let written = fs::read_to_string(&file_path).unwrap();
        assert_eq!(written, content);
    }

    #[test]
    fn test_atomic_write_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Write initial content
        atomic_write(&file_path, "initial").unwrap();
        let content1 = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content1, "initial");

        // Overwrite with new content
        atomic_write(&file_path, "updated").unwrap();
        let content2 = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content2, "updated");
    }

    #[test]
    fn test_atomic_write_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested/dir/test.txt");
        let content = "Nested content";

        atomic_write(&file_path, content).unwrap();

        // Verify file was written
        let written = fs::read_to_string(&file_path).unwrap();
        assert_eq!(written, content);
    }

    #[test]
    fn test_atomic_write_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("binary.bin");
        let content: &[u8] = &[0x00, 0x01, 0x02, 0x03, 0xFF];

        atomic_write_bytes(&file_path, content).unwrap();

        // Verify file was written
        let written = fs::read(&file_path).unwrap();
        assert_eq!(written, content);
    }

    #[test]
    fn test_atomic_write_empty() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");

        atomic_write(&file_path, "").unwrap();

        // Verify file exists and is empty
        let written = fs::read_to_string(&file_path).unwrap();
        assert_eq!(written, "");
    }

    #[tokio::test]
    async fn test_atomic_write_async_basic() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("async.txt");
        let content = "Async content";

        atomic_write_async(&file_path, content).await.unwrap();

        // Verify file was written
        let written = fs::read_to_string(&file_path).unwrap();
        assert_eq!(written, content);
    }

    #[test]
    fn test_generate_random_suffix() {
        let suffix1 = generate_random_suffix();
        let suffix2 = generate_random_suffix();

        // Should be 8 characters (first part of UUID)
        assert_eq!(suffix1.len(), 8);
        assert_eq!(suffix2.len(), 8);

        // Should be different (very high probability)
        assert_ne!(suffix1, suffix2);
    }
}
