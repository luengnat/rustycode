// rustycode-orchestra/src/json_persistence.rs
//! JSON file persistence utilities with validation and atomic writes.
//!
//! Provides safe JSON file loading/saving with:
//! - Validation via type predicates
//! - Default values on failure
//! - Atomic writes (temp file + rename)
//! - Automatic directory creation
//! - Non-fatal error handling

use std::fs;
use std::io;
use std::path::Path;

/// Load a JSON file with validation, returning a default on failure.
///
/// Handles missing files, corrupt JSON, and schema mismatches uniformly.
/// Returns the result of `default_factory` if the file doesn't exist,
/// can't be parsed, or fails validation.
///
/// # Type Parameters
/// * `T` - The target type (must implement serde::Deserialize)
///
/// # Arguments
/// * `file_path` - Path to the JSON file
/// * `validate` - Validation function that checks if parsed data is valid
/// * `default_factory` - Function that produces a default value on failure
///
/// # Returns
/// * The parsed and validated data, or the default value
///
/// # Examples
/// ```rust,no_run
/// use serde_json::Value;
/// use rustycode_orchestra::json_persistence::load_json_file;
///
/// let data = load_json_file(
///     "/path/to/config.json",
///     |v: &Value| v.get("key").is_some(),
///     || Value::Object(Default::default()),
/// );
/// ```
pub fn load_json_file<F, T>(file_path: &Path, validate: F, default_factory: fn() -> T) -> T
where
    T: for<'de> serde::Deserialize<'de>,
    F: FnOnce(&T) -> bool,
{
    load_json_file_or_null(file_path, validate).unwrap_or_else(default_factory)
}

/// Load a JSON file with validation, returning None on failure.
///
/// For callers that need to distinguish "no data" from "default data".
/// Returns None if the file doesn't exist, can't be parsed,
/// or fails validation.
///
/// # Type Parameters
/// * `T` - The target type (must implement serde::Deserialize)
///
/// # Arguments
/// * `file_path` - Path to the JSON file
/// * `validate` - Validation function that checks if parsed data is valid
///
/// # Returns
/// * Some(parsed data) if successful and valid
/// * None if file missing, parse error, or validation failure
///
/// # Examples
/// ```rust,no_run
/// use serde_json::Value;
/// use rustycode_orchestra::json_persistence::load_json_file_or_null;
///
/// let data = load_json_file_or_null(
///     "/path/to/config.json",
///     |v: &Value| v.is_object(),
/// );
/// ```
pub fn load_json_file_or_null<F, T>(file_path: &Path, validate: F) -> Option<T>
where
    T: for<'de> serde::Deserialize<'de>,
    F: FnOnce(&T) -> bool,
{
    // Check if file exists
    if !file_path.exists() {
        return None;
    }

    // Read file content
    let raw = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(_) => return None,
    };

    // Parse JSON
    let parsed: T = match serde_json::from_str(&raw) {
        Ok(data) => data,
        Err(_) => return None,
    };

    // Validate
    if validate(&parsed) {
        Some(parsed)
    } else {
        None
    }
}

/// Save a JSON file, creating parent directories as needed.
///
/// Non-fatal — swallows errors to prevent persistence from breaking operations.
/// Writes with pretty-printing (2-space indentation) and trailing newline.
///
/// # Type Parameters
/// * `T` - The type to serialize (must implement serde::Serialize)
///
/// # Arguments
/// * `file_path` - Path where the JSON file should be saved
/// * `data` - The data to serialize and write
///
/// # Examples
/// ```rust,no_run
/// use serde_json::json;
/// use rustycode_orchestra::json_persistence::save_json_file;
/// use std::path::Path;
///
/// save_json_file(
///     &Path::new("/path/to/output.json"),
///     &json!({ "key": "value" }),
/// );
/// ```
pub fn save_json_file<T>(file_path: &Path, data: &T) -> io::Result<()>
where
    T: ?Sized + serde::Serialize,
{
    save_json_file_inner(file_path, data)
}

/// Inner implementation that returns Result
fn save_json_file_inner<T>(file_path: &Path, data: &T) -> io::Result<()>
where
    T: ?Sized + serde::Serialize,
{
    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Serialize with pretty-printing
    let json = serde_json::to_string_pretty(data)?;

    // Write to file with trailing newline
    fs::write(file_path, format!("{}\n", json))?;

    Ok(())
}

/// Write a JSON file atomically (write to .tmp, then rename).
///
/// Creates parent directories as needed. Non-fatal on error.
/// This is safer than `save_json_file` for concurrent access or crash recovery.
///
/// # Type Parameters
/// * `T` - The type to serialize (must implement serde::Serialize)
///
/// # Arguments
/// * `file_path` - Path where the JSON file should be saved
/// * `data` - The data to serialize and write
///
/// # Examples
/// ```rust,no_run
/// use serde_json::json;
/// use rustycode_orchestra::json_persistence::write_json_file_atomic;
/// use std::path::Path;
///
/// write_json_file_atomic(
///     &Path::new("/path/to/critical.json"),
///     &json!({ "key": "value" }),
/// );
/// ```
pub fn write_json_file_atomic<T>(file_path: &Path, data: &T) -> io::Result<()>
where
    T: ?Sized + serde::Serialize,
{
    write_json_file_atomic_inner(file_path, data)
}

/// Inner implementation that returns Result
fn write_json_file_atomic_inner<T>(file_path: &Path, data: &T) -> io::Result<()>
where
    T: ?Sized + serde::Serialize,
{
    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write to temporary file
    let tmp_path = file_path.with_extension("tmp");
    let json = serde_json::to_string_pretty(data)?;
    fs::write(&tmp_path, json)?;

    // Atomic rename
    fs::rename(&tmp_path, file_path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestData {
        name: String,
        value: i32,
    }

    impl Default for TestData {
        fn default() -> Self {
            TestData {
                name: "default".to_string(),
                value: 42,
            }
        }
    }

    #[test]
    fn test_load_json_file_missing() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.json");

        let result = load_json_file(&file_path, |d: &TestData| d.value > 0, TestData::default);

        assert_eq!(result, TestData::default());
    }

    #[test]
    fn test_load_json_file_valid() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        let data = TestData {
            name: "test".to_string(),
            value: 100,
        };
        let _ = save_json_file(&file_path, &data);

        let result = load_json_file(&file_path, |d: &TestData| d.value > 0, TestData::default);

        assert_eq!(result, data);
    }

    #[test]
    fn test_load_json_file_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("invalid.json");

        // Write invalid JSON
        fs::write(&file_path, "{ invalid json }").unwrap();

        let result = load_json_file(&file_path, |d: &TestData| d.value > 0, TestData::default);

        assert_eq!(result, TestData::default());
    }

    #[test]
    fn test_load_json_file_validation_fails() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        let data = TestData {
            name: "test".to_string(),
            value: -1, // Invalid (negative)
        };
        let _ = save_json_file(&file_path, &data);

        let result = load_json_file(
            &file_path,
            |d: &TestData| d.value > 0, // Requires positive value
            TestData::default,
        );

        assert_eq!(result, TestData::default());
    }

    #[test]
    fn test_load_json_file_or_null_missing() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.json");

        let result = load_json_file_or_null(&file_path, |d: &TestData| d.value > 0);

        assert!(result.is_none());
    }

    #[test]
    fn test_load_json_file_or_null_valid() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        let data = TestData {
            name: "test".to_string(),
            value: 100,
        };
        let _ = save_json_file(&file_path, &data);

        let result = load_json_file_or_null(&file_path, |d: &TestData| d.value > 0);

        assert_eq!(result, Some(data));
    }

    #[test]
    fn test_load_json_file_or_null_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("invalid.json");

        fs::write(&file_path, "{ invalid json }").unwrap();

        let result = load_json_file_or_null(&file_path, |d: &TestData| d.value > 0);

        assert!(result.is_none());
    }

    #[test]
    fn test_load_json_file_or_null_validation_fails() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        let data = TestData {
            name: "test".to_string(),
            value: -1,
        };
        let _ = save_json_file(&file_path, &data);

        let result = load_json_file_or_null(&file_path, |d: &TestData| d.value > 0);

        assert!(result.is_none());
    }

    #[test]
    fn test_save_json_file_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested/dir/test.json");

        let data = TestData {
            name: "test".to_string(),
            value: 100,
        };

        let _ = save_json_file(&file_path, &data);

        assert!(file_path.exists());
        assert!(file_path.parent().unwrap().exists());
    }

    #[test]
    fn test_save_json_file_formatting() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        let data = json!({ "name": "test", "value": 100 });

        let _ = save_json_file(&file_path, &data);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("\n")); // Pretty-printed
        assert!(content.ends_with("\n")); // Trailing newline
    }

    #[test]
    fn test_save_json_file_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        let original = TestData {
            name: "test".to_string(),
            value: 100,
        };

        let _ = save_json_file(&file_path, &original);

        let loaded = load_json_file(
            &file_path,
            |_d| true,
            || TestData {
                name: "wrong".to_string(),
                value: 0,
            },
        );

        assert_eq!(loaded, original);
    }

    #[test]
    fn test_write_json_file_atomic_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested/dir/test.json");

        let data = TestData {
            name: "test".to_string(),
            value: 100,
        };

        write_json_file_atomic(&file_path, &data).unwrap();

        assert!(file_path.exists());
        assert!(file_path.parent().unwrap().exists());
    }

    #[test]
    fn test_write_json_file_atomic_no_tmp_leftover() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");
        let tmp_path = file_path.with_extension("tmp");

        let data = TestData {
            name: "test".to_string(),
            value: 100,
        };

        write_json_file_atomic(&file_path, &data).unwrap();

        assert!(file_path.exists());
        assert!(!tmp_path.exists()); // Temp file should be renamed, not left behind
    }

    #[test]
    fn test_write_json_file_atomic_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        let original = TestData {
            name: "test".to_string(),
            value: 100,
        };

        write_json_file_atomic(&file_path, &original).unwrap();

        let loaded = load_json_file(
            &file_path,
            |_d| true,
            || TestData {
                name: "wrong".to_string(),
                value: 0,
            },
        );

        assert_eq!(loaded, original);
    }

    #[test]
    fn test_complex_type() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("complex.json");

        let mut original = HashMap::new();
        original.insert("key1".to_string(), 100);
        original.insert("key2".to_string(), 200);

        let _ = save_json_file(&file_path, &original);

        let loaded = load_json_file(
            &file_path,
            |m: &HashMap<String, i32>| m.len() == 2,
            HashMap::new,
        );

        assert_eq!(loaded, original);
    }

    #[test]
    fn test_validation_rejects_wrong_type() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        // Write a JSON object when we expect an array
        fs::write(&file_path, "{\"key\":\"value\"}").unwrap();

        let result: Option<Vec<String>> = load_json_file_or_null(&file_path, |_| true);

        assert!(result.is_none());
    }

    #[test]
    fn test_save_json_file_overwrites() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");

        let data1 = TestData {
            name: "first".to_string(),
            value: 1,
        };
        let _ = save_json_file(&file_path, &data1);

        let data2 = TestData {
            name: "second".to_string(),
            value: 2,
        };
        let _ = save_json_file(&file_path, &data2);

        let loaded = load_json_file(&file_path, |_d| true, TestData::default);

        assert_eq!(loaded, data2);
    }
}
