//! Headless context loading — stdin reading, file context, and project bootstrapping.
//!
//! Handles loading context from files or stdin for headless new-milestone,
//! and bootstraps the .orchestra/ directory structure when needed.
//!
//! Matches orchestra-2's headless-context.ts implementation.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Options for loading context
#[derive(Debug, Clone, Default)]
pub struct ContextOptions {
    /// File path to read context from, or "-" for stdin
    pub context: Option<String>,
    /// Inline text context
    pub context_text: Option<String>,
}

/// Read all data from stdin
///
/// # Returns
/// The complete stdin content as a String
///
/// # Errors
/// Returns an error if reading from stdin fails
///
/// # Examples
/// ```no_run
/// use rustycode_orchestra::headless_context::read_stdin;
///
/// let input = read_stdin().expect("Failed to read stdin");
/// println!("Got {} bytes from stdin", input.len());
/// ```
pub fn read_stdin() -> io::Result<String> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

/// Load context from the provided options
///
/// # Arguments
/// * `options` - ContextOptions specifying where to load context from
///
/// # Returns
/// The loaded context as a String
///
/// # Errors
/// Returns an error if:
/// - No context source is provided
/// - Reading from the file fails
/// - Reading from stdin fails
///
/// # Examples
/// ```no_run
/// use rustycode_orchestra::headless_context::{load_context, ContextOptions};
///
/// // Load from file
/// let options = ContextOptions {
///     context: Some("/path/to/context.md".to_string()),
///     context_text: None,
/// };
/// let context = load_context(options).expect("Failed to load context");
///
/// // Load from stdin
/// let options = ContextOptions {
///     context: Some("-".to_string()),
///     context_text: None,
/// };
/// let context = load_context(options).expect("Failed to load context");
///
/// // Load from inline text
/// let options = ContextOptions {
///     context: None,
///     context_text: Some("Inline context here".to_string()),
/// };
/// let context = load_context(options).expect("Failed to load context");
/// ```
pub fn load_context(options: ContextOptions) -> io::Result<String> {
    // Priority 1: Inline text
    if let Some(text) = options.context_text {
        return Ok(text);
    }

    // Priority 2: File or stdin
    if let Some(context) = options.context {
        if context == "-" {
            // Read from stdin
            return read_stdin();
        } else {
            // Read from file
            return fs::read_to_string(context);
        }
    }

    // No context source provided
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "No context provided. Use --context <file> or --context-text <text>",
    ))
}

/// Bootstrap .orchestra/ directory structure for headless new-milestone.
///
/// Mirrors the bootstrap logic from guided-flow.ts showSmartEntry().
///
/// # Arguments
/// * `base_path` - Base directory path where .orchestra will be created
///
/// # Errors
/// Returns an error if directory creation fails
///
/// # Examples
/// ```no_run
/// use rustycode_orchestra::headless_context::bootstrap_orchestra_project;
/// use std::path::Path;
///
/// let project_dir = Path::new("/my/project");
/// bootstrap_orchestra_project(project_dir).expect("Failed to bootstrap .orchestra directory");
/// ```
///
/// # Directory Structure
/// Creates the following structure:
/// ```text
/// base_path/
/// └── .orchestra/
///     ├── milestones/
///     └── runtime/
/// ```
pub fn bootstrap_orchestra_project<P: AsRef<Path>>(base_path: P) -> io::Result<()> {
    let orchestra_dir = base_path.as_ref().join(".orchestra");
    let milestones_dir = orchestra_dir.join("milestones");
    let runtime_dir = orchestra_dir.join("runtime");

    fs::create_dir_all(&milestones_dir)?;
    fs::create_dir_all(&runtime_dir)?;

    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_bootstrap_orchestra_project_creates_directories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let base_path = temp_dir.path();

        bootstrap_orchestra_project(base_path).expect("Failed to bootstrap");

        let orchestra_dir = base_path.join(".orchestra");
        let milestones_dir = orchestra_dir.join("milestones");
        let runtime_dir = orchestra_dir.join("runtime");

        assert!(orchestra_dir.exists(), ".orchestra directory should exist");
        assert!(orchestra_dir.is_dir(), ".orchestra should be a directory");
        assert!(milestones_dir.exists(), "milestones directory should exist");
        assert!(runtime_dir.exists(), "runtime directory should exist");
    }

    #[test]
    fn test_bootstrap_orchestra_project_idempotent() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let base_path = temp_dir.path();

        // Bootstrap twice
        bootstrap_orchestra_project(base_path).expect("Failed to bootstrap first time");
        bootstrap_orchestra_project(base_path).expect("Failed to bootstrap second time");

        // Should still work
        let orchestra_dir = base_path.join(".orchestra");
        assert!(orchestra_dir.exists());
    }

    #[test]
    fn test_load_context_from_inline_text() {
        let options = ContextOptions {
            context: None,
            context_text: Some("Hello, world!".to_string()),
        };

        let result = load_context(options).expect("Failed to load context");
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_load_context_from_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("context.txt");

        let mut file = File::create(&file_path).expect("Failed to create file");
        file.write_all(b"File content")
            .expect("Failed to write to file");

        let options = ContextOptions {
            context: Some(file_path.to_string_lossy().to_string()),
            context_text: None,
        };

        let result = load_context(options).expect("Failed to load context");
        assert_eq!(result, "File content");
    }

    #[test]
    fn test_load_context_from_nonexistent_file() {
        let options = ContextOptions {
            context: Some("/nonexistent/path.txt".to_string()),
            context_text: None,
        };

        let result = load_context(options);
        assert!(result.is_err(), "Should fail for nonexistent file");
    }

    #[test]
    fn test_load_context_no_source() {
        let options = ContextOptions {
            context: None,
            context_text: None,
        };

        let result = load_context(options);
        assert!(
            result.is_err(),
            "Should fail when no context source provided"
        );

        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("No context provided"));
    }

    #[test]
    fn test_context_options_default() {
        let options = ContextOptions::default();
        assert!(options.context.is_none());
        assert!(options.context_text.is_none());
    }

    #[test]
    fn test_bootstrap_creates_nested_directories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let base_path = temp_dir.path();

        bootstrap_orchestra_project(base_path).expect("Failed to bootstrap");

        // Verify full paths exist
        let orchestra_dir = base_path.join(".orchestra");
        let milestones_dir = orchestra_dir.join("milestones");
        let runtime_dir = orchestra_dir.join("runtime");

        assert!(milestones_dir.is_dir());
        assert!(runtime_dir.is_dir());
    }

    #[test]
    fn test_load_context_inline_text_priority_over_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("context.txt");

        let mut file = File::create(&file_path).expect("Failed to create file");
        file.write_all(b"File content")
            .expect("Failed to write to file");

        // Both inline text and file provided - inline should win
        let options = ContextOptions {
            context: Some(file_path.to_string_lossy().to_string()),
            context_text: Some("Inline content".to_string()),
        };

        let result = load_context(options).expect("Failed to load context");
        assert_eq!(result, "Inline content", "Inline text should take priority");
    }

    #[test]
    fn test_bootstrap_with_relative_path() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Use relative path from current directory
        let base_path = temp_dir.path().join("subdir");

        bootstrap_orchestra_project(&base_path).expect("Failed to bootstrap");

        let orchestra_dir = base_path.join(".orchestra");
        assert!(orchestra_dir.exists());
    }

    #[test]
    fn test_bootstrap_directory_permissions() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let base_path = temp_dir.path();

        bootstrap_orchestra_project(base_path).expect("Failed to bootstrap");

        let orchestra_dir = base_path.join(".orchestra");

        // Check that directories are readable and writable
        let metadata = fs::metadata(&orchestra_dir).expect("Failed to get metadata");
        assert!(metadata.is_dir(), "Should be a directory");
        assert!(!metadata.permissions().readonly(), "Should be writable");
    }
}
