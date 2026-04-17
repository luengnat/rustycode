// rustycode-orchestra/src/migrate_validator.rs
//! Old .planning directory validator
//!
//! Pre-flight checks for minimum viable .planning directory.
//! Pure functions with zero external dependencies — uses only std::fs and std::path.

use std::path::Path;

/// Migration validation severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum MigrationValidationSeverity {
    /// Fatal error - operation cannot proceed
    #[serde(rename = "fatal")]
    Fatal,

    /// Warning - operation can proceed with reduced data
    #[serde(rename = "warning")]
    Warning,
}

/// A migration validation issue found during directory validation
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MigrationValidationIssue {
    /// File or directory path that has the issue
    pub file: String,
    /// Severity of the issue
    pub severity: MigrationValidationSeverity,
    /// Human-readable description of the issue
    pub message: String,
}

/// Result of validating a directory for migration
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MigrationValidationResult {
    /// Whether validation passed (no fatal issues)
    pub valid: bool,
    /// List of all issues found
    pub issues: Vec<MigrationValidationIssue>,
}

/// Validate that a .planning directory has the minimum required structure.
///
/// Returns structured issues with severity levels:
/// - fatal: directory doesn't exist (migration cannot proceed)
/// - warning: optional files missing (migration can proceed with reduced data)
///
/// # Arguments
/// * `path` - Path to the .planning directory
///
/// # Returns
/// MigrationValidationResult with valid flag and list of issues
///
/// # Examples
/// ```rust,no_run
/// use rustycode_orchestra::migrate_validator::validate_planning_directory;
/// use std::path::Path;
///
/// let result = validate_planning_directory(Path::new("/project/.planning"));
/// if !result.valid {
///     for issue in &result.issues {
///         eprintln!("{}: {}", issue.file, issue.message);
///     }
/// }
/// ```
pub fn validate_planning_directory(path: &Path) -> MigrationValidationResult {
    let mut issues = Vec::new();

    // Check directory exists
    if !path.exists() {
        issues.push(MigrationValidationIssue {
            file: path.display().to_string(),
            severity: MigrationValidationSeverity::Fatal,
            message: "Directory does not exist".to_string(),
        });
        return MigrationValidationResult {
            valid: false,
            issues,
        };
    }

    if !path.is_dir() {
        issues.push(MigrationValidationIssue {
            file: path.display().to_string(),
            severity: MigrationValidationSeverity::Fatal,
            message: "Path is not a directory".to_string(),
        });
        return MigrationValidationResult {
            valid: false,
            issues,
        };
    }

    // ROADMAP.md — warn if missing (transformer falls back to filesystem phases)
    let roadmap_path = path.join("ROADMAP.md");
    if !roadmap_path.exists() {
        issues.push(MigrationValidationIssue {
            file: "ROADMAP.md".to_string(),
            severity: MigrationValidationSeverity::Warning,
            message:
                "ROADMAP.md not found — milestone structure will be inferred from phases/ directory"
                    .to_string(),
        });
    }

    // Optional files — warn if missing
    let project_path = path.join("PROJECT.md");
    if !project_path.exists() {
        issues.push(MigrationValidationIssue {
            file: "PROJECT.md".to_string(),
            severity: MigrationValidationSeverity::Warning,
            message: "PROJECT.md not found — project metadata will be empty".to_string(),
        });
    }

    let requirements_path = path.join("REQUIREMENTS.md");
    if !requirements_path.exists() {
        issues.push(MigrationValidationIssue {
            file: "REQUIREMENTS.md".to_string(),
            severity: MigrationValidationSeverity::Warning,
            message: "REQUIREMENTS.md not found — requirements will be empty".to_string(),
        });
    }

    let state_path = path.join("STATE.md");
    if !state_path.exists() {
        issues.push(MigrationValidationIssue {
            file: "STATE.md".to_string(),
            severity: MigrationValidationSeverity::Warning,
            message: "STATE.md not found — state information will be empty".to_string(),
        });
    }

    // phases/ directory
    let phases_path = path.join("phases");
    if !phases_path.exists() || !phases_path.is_dir() {
        issues.push(MigrationValidationIssue {
            file: "phases/".to_string(),
            severity: MigrationValidationSeverity::Warning,
            message: "phases/ directory not found — no phase data will be parsed".to_string(),
        });
    }

    let has_fatal = issues
        .iter()
        .any(|i| i.severity == MigrationValidationSeverity::Fatal);
    MigrationValidationResult {
        valid: !has_fatal,
        issues,
    }
}

/// Create a migration validation issue
#[allow(dead_code)] // Kept for future use
fn issue(
    file: impl Into<String>,
    severity: MigrationValidationSeverity,
    message: impl Into<String>,
) -> MigrationValidationIssue {
    MigrationValidationIssue {
        file: file.into(),
        severity,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_validate_planning_directory_missing() {
        let temp_dir = TempDir::new().unwrap();
        let planning_path = temp_dir.path().join(".planning");

        let result = validate_planning_directory(&planning_path);

        assert!(!result.valid);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(
            result.issues[0].severity,
            MigrationValidationSeverity::Fatal
        );
        assert!(result.issues[0].message.contains("does not exist"));
    }

    #[test]
    fn test_validate_planning_directory_not_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("not_a_dir");
        fs::write(&file_path, "test").unwrap();

        let result = validate_planning_directory(&file_path);

        assert!(!result.valid);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(
            result.issues[0].severity,
            MigrationValidationSeverity::Fatal
        );
        assert!(result.issues[0].message.contains("not a directory"));
    }

    #[test]
    fn test_validate_planning_directory_empty() {
        let temp_dir = TempDir::new().unwrap();
        let planning_path = temp_dir.path().join(".planning");
        fs::create_dir(&planning_path).unwrap();

        let result = validate_planning_directory(&planning_path);

        // Empty directory is valid (has all optional files missing)
        assert!(result.valid);
        assert_eq!(result.issues.len(), 5); // All warnings

        // All should be warnings
        for issue in &result.issues {
            assert_eq!(issue.severity, MigrationValidationSeverity::Warning);
        }
    }

    #[test]
    fn test_validate_planning_directory_with_roadmap() {
        let temp_dir = TempDir::new().unwrap();
        let planning_path = temp_dir.path().join(".planning");
        fs::create_dir(&planning_path).unwrap();
        fs::write(planning_path.join("ROADMAP.md"), "# Roadmap").unwrap();

        let result = validate_planning_directory(&planning_path);

        assert!(result.valid);
        // Should have 4 warnings (all except ROADMAP.md)
        assert_eq!(result.issues.len(), 4);
    }

    #[test]
    fn test_validate_planning_directory_complete() {
        let temp_dir = TempDir::new().unwrap();
        let planning_path = temp_dir.path().join(".planning");
        fs::create_dir(&planning_path).unwrap();
        fs::write(planning_path.join("ROADMAP.md"), "# Roadmap").unwrap();
        fs::write(planning_path.join("PROJECT.md"), "# Project").unwrap();
        fs::write(planning_path.join("REQUIREMENTS.md"), "# Requirements").unwrap();
        fs::write(planning_path.join("STATE.md"), "# State").unwrap();
        fs::create_dir(planning_path.join("phases")).unwrap();

        let result = validate_planning_directory(&planning_path);

        assert!(result.valid);
        assert_eq!(result.issues.len(), 0);
    }

    #[test]
    fn test_validate_planning_directory_with_phases_file() {
        let temp_dir = TempDir::new().unwrap();
        let planning_path = temp_dir.path().join(".planning");
        fs::create_dir(&planning_path).unwrap();
        fs::write(planning_path.join("ROADMAP.md"), "# Roadmap").unwrap();

        // Create phases as a file instead of directory
        fs::write(planning_path.join("phases"), "not a directory").unwrap();

        let result = validate_planning_directory(&planning_path);

        assert!(result.valid);
        // Should have warning for phases not being a directory
        let phases_issue = result.issues.iter().find(|i| i.file == "phases/");
        assert!(phases_issue.is_some());
    }

    #[test]
    fn test_validation_severity_fatal() {
        let temp_dir = TempDir::new().unwrap();
        let planning_path = temp_dir.path().join(".planning");

        let result = validate_planning_directory(&planning_path);

        assert!(!result.valid);
        assert!(result
            .issues
            .iter()
            .any(|i| i.severity == MigrationValidationSeverity::Fatal));
    }

    #[test]
    fn test_validation_severity_warning() {
        let temp_dir = TempDir::new().unwrap();
        let planning_path = temp_dir.path().join(".planning");
        fs::create_dir(&planning_path).unwrap();

        let result = validate_planning_directory(&planning_path);

        assert!(result.valid);
        assert!(!result
            .issues
            .iter()
            .any(|i| i.severity == MigrationValidationSeverity::Fatal));
        assert!(result
            .issues
            .iter()
            .all(|i| i.severity == MigrationValidationSeverity::Warning));
    }

    #[test]
    fn test_validation_issue_content() {
        let temp_dir = TempDir::new().unwrap();
        let planning_path = temp_dir.path().join(".planning");
        fs::create_dir(&planning_path).unwrap();

        let result = validate_planning_directory(&planning_path);

        // Check that messages are descriptive
        for issue in &result.issues {
            assert!(!issue.message.is_empty());
            assert!(!issue.file.is_empty());
        }
    }

    #[test]
    fn test_validation_issue_files() {
        let temp_dir = TempDir::new().unwrap();
        let planning_path = temp_dir.path().join(".planning");
        fs::create_dir(&planning_path).unwrap();

        let result = validate_planning_directory(&planning_path);

        let files: Vec<&str> = result.issues.iter().map(|i| i.file.as_str()).collect();

        // Should warn about all expected files
        assert!(files.contains(&"ROADMAP.md"));
        assert!(files.contains(&"PROJECT.md"));
        assert!(files.contains(&"REQUIREMENTS.md"));
        assert!(files.contains(&"STATE.md"));
        assert!(files.contains(&"phases/"));
    }

    #[test]
    fn test_validation_result_serialization() {
        // Ensure MigrationValidationResult can be serialized (useful for APIs)
        let result = MigrationValidationResult {
            valid: true,
            issues: vec![],
        };

        // This will fail to compile if serde attributes are wrong
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"valid\":true"));
    }

    #[test]
    fn test_validation_issue_serialization() {
        let issue = MigrationValidationIssue {
            file: "test.txt".to_string(),
            severity: MigrationValidationSeverity::Warning,
            message: "Test message".to_string(),
        };

        let json = serde_json::to_string(&issue).unwrap();
        assert!(json.contains("\"file\":\"test.txt\""));
        assert!(json.contains("\"severity\":\"warning\""));
    }
}
