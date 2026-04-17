//! Verification Evidence — JSON persistence and markdown table formatting.
//!
//! Two main functions:
//!   - write_verification_json: persists a machine-readable T##-VERIFY.json artifact
//!   - format_evidence_table:   returns a markdown evidence table string
//!
//! JSON schema uses schemaVersion: 1 for forward-compatibility.
//! stdout/stderr are intentionally excluded from the JSON to avoid unbounded file sizes.
//!
//! Matches orchestra-2's verification-evidence.ts implementation.

use crate::error::{OrchestraV2Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;

// ─── Types ───────────────────────────────────────────────────────────────────

/// Verification check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCheck {
    /// Command that was run
    pub command: String,
    /// Exit code (0 = pass)
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Runtime error from bg-shell or browser
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeError {
    /// Source of the error
    pub source: RuntimeErrorSource,
    /// Severity level
    pub severity: RuntimeErrorSeverity,
    /// Error message
    pub message: String,
    /// Whether this blocks completion
    pub blocking: bool,
}

/// Source of runtime error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum RuntimeErrorSource {
    /// Background shell process
    BgShell,
    /// Browser console
    Browser,
}

impl fmt::Display for RuntimeErrorSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeErrorSource::BgShell => write!(f, "bg-shell"),
            RuntimeErrorSource::Browser => write!(f, "browser"),
        }
    }
}

/// Severity of runtime error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum RuntimeErrorSeverity {
    /// Process crashed
    Crash,
    /// Error occurred
    Error,
    /// Warning occurred
    Warning,
}

impl fmt::Display for RuntimeErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeErrorSeverity::Crash => write!(f, "crash"),
            RuntimeErrorSeverity::Error => write!(f, "error"),
            RuntimeErrorSeverity::Warning => write!(f, "warning"),
        }
    }
}

/// Dependency vulnerability warning from npm audit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditWarning {
    /// Package name
    pub name: String,
    /// Vulnerability severity
    pub severity: AuditSeverity,
    /// Vulnerability title
    pub title: String,
    /// Advisory URL
    pub url: String,
    /// Whether a fix is available
    pub fix_available: bool,
}

/// Severity of audit warning
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AuditSeverity {
    /// Low severity
    Low,
    /// Moderate severity
    Moderate,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

impl fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditSeverity::Low => write!(f, "low"),
            AuditSeverity::Moderate => write!(f, "moderate"),
            AuditSeverity::High => write!(f, "high"),
            AuditSeverity::Critical => write!(f, "critical"),
        }
    }
}

/// Aggregate result from verification gate
/// Note: This is a different type from verification_gate::VerificationResult
/// This version includes timestamp, runtime_errors, and audit_warnings fields
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// true if all checks passed (or no checks discovered)
    pub passed: bool,
    /// Per-command results
    pub checks: Vec<VerificationCheck>,
    /// Where checks were discovered
    pub discovery_source: DiscoverySource,
    /// Timestamp at gate start
    pub timestamp: i64,
    /// Optional runtime errors
    pub runtime_errors: Option<Vec<RuntimeError>>,
    /// Optional audit warnings
    pub audit_warnings: Option<Vec<AuditWarning>>,
}

/// Where verification checks were discovered
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DiscoverySource {
    /// From user preferences
    Preference,
    /// From task plan must-haves
    TaskPlan,
    /// From package.json scripts
    PackageJson,
    /// No checks discovered
    None,
}

// ─── JSON Evidence Types ─────────────────────────────────────────────────────

/// Verification check for JSON serialization (excludes stdout/stderr)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCheckJson {
    /// Command that was run
    pub command: String,
    /// Exit code
    pub exit_code: i32,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Verdict (pass/fail)
    pub verdict: Verdict,
}

/// Pass/fail verdict
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Verdict {
    /// Check passed
    Pass,
    /// Check failed
    Fail,
}

/// Runtime error for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeErrorJson {
    /// Source of the error
    #[serde(rename = "source")]
    pub source: RuntimeErrorSource,
    /// Severity level
    pub severity: RuntimeErrorSeverity,
    /// Error message
    pub message: String,
    /// Whether this blocks completion
    pub blocking: bool,
}

/// Audit warning for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditWarningJson {
    /// Package name
    pub name: String,
    /// Vulnerability severity
    pub severity: String,
    /// Vulnerability title
    pub title: String,
    /// Advisory URL
    pub url: String,
    /// Whether a fix is available
    pub fix_available: bool,
}

/// Evidence JSON structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceJson {
    /// Schema version for forward-compatibility
    pub schema_version: u32,
    /// Task ID
    pub task_id: String,
    /// Unit ID
    pub unit_id: String,
    /// Timestamp at gate start
    pub timestamp: i64,
    /// Whether all checks passed
    pub passed: bool,
    /// Where checks were discovered
    pub discovery_source: String,
    /// Verification checks
    pub checks: Vec<EvidenceCheckJson>,
    /// Retry attempt number (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_attempt: Option<u32>,
    /// Max retries (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    /// Runtime errors (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_errors: Option<Vec<RuntimeErrorJson>>,
    /// Audit warnings (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_warnings: Option<Vec<AuditWarningJson>>,
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Write a T##-VERIFY.json artifact to the tasks directory.
///
/// Creates the directory if it doesn't exist.
///
/// stdout/stderr are excluded from the JSON — the full output lives in VerificationResult
/// in memory and is logged to stderr during the gate run.
///
/// # Arguments
/// * `result` - Verification result to persist
/// * `tasks_dir` - Path to tasks directory
/// * `task_id` - Task ID (e.g., "T01")
/// * `unit_id` - Optional unit ID (defaults to task_id if not provided)
/// * `retry_attempt` - Optional retry attempt number
/// * `max_retries` - Optional max retries
///
/// # Returns
/// Result indicating success or failure
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::verification_evidence::*;
///
/// let result = VerificationResult {
///     passed: true,
///     checks: vec![],
///     discovery_source: DiscoverySource::Preference,
///     timestamp: 1234567890,
///     runtime_errors: None,
///     audit_warnings: None,
/// };
///
/// write_verification_json(
///     &result,
///     Path::new("/project/.orchestra/milestones/M001/tasks"),
///     "T01",
///     Some("M001-T01"),
///     Some(1),
///     Some(3),
/// )?;
/// ```
pub fn write_verification_json(
    result: &VerificationResult,
    tasks_dir: &Path,
    task_id: &str,
    unit_id: Option<&str>,
    retry_attempt: Option<u32>,
    max_retries: Option<u32>,
) -> Result<()> {
    // Create tasks directory if it doesn't exist
    fs::create_dir_all(tasks_dir)?;

    let checks: Vec<EvidenceCheckJson> = result
        .checks
        .iter()
        .map(|check| EvidenceCheckJson {
            command: check.command.clone(),
            exit_code: check.exit_code,
            duration_ms: check.duration_ms,
            verdict: if check.exit_code == 0 {
                Verdict::Pass
            } else {
                Verdict::Fail
            },
        })
        .collect();

    let mut evidence = EvidenceJson {
        schema_version: 1,
        task_id: task_id.to_string(),
        unit_id: unit_id.unwrap_or(task_id).to_string(),
        timestamp: result.timestamp,
        passed: result.passed,
        discovery_source: format!("{:?}", result.discovery_source).to_lowercase(),
        checks,
        retry_attempt: None,
        max_retries: None,
        runtime_errors: None,
        audit_warnings: None,
    };

    if let Some(attempt) = retry_attempt {
        evidence.retry_attempt = Some(attempt);
    }
    if let Some(max) = max_retries {
        evidence.max_retries = Some(max);
    }

    if let Some(errors) = &result.runtime_errors {
        if !errors.is_empty() {
            evidence.runtime_errors = Some(
                errors
                    .iter()
                    .map(|e| RuntimeErrorJson {
                        source: e.source.clone(),
                        severity: e.severity.clone(),
                        message: e.message.clone(),
                        blocking: e.blocking,
                    })
                    .collect(),
            );
        }
    }

    if let Some(warnings) = &result.audit_warnings {
        if !warnings.is_empty() {
            evidence.audit_warnings = Some(
                warnings
                    .iter()
                    .map(|w| AuditWarningJson {
                        name: w.name.clone(),
                        severity: format!("{:?}", w.severity).to_lowercase(),
                        title: w.title.clone(),
                        url: w.url.clone(),
                        fix_available: w.fix_available,
                    })
                    .collect(),
            );
        }
    }

    let file_path = tasks_dir.join(format!("{}-VERIFY.json", task_id));
    let json = serde_json::to_string_pretty(&evidence).map_err(|e| {
        OrchestraV2Error::Serialization(format!("Failed to serialize evidence JSON: {}", e))
    })?;
    fs::write(&file_path, json + "\n").map_err(OrchestraV2Error::Io)?;

    Ok(())
}

/// Generate a markdown evidence table from a VerificationResult.
///
/// Returns a "no checks" note if result.checks is empty.
/// Otherwise returns a 5-column markdown table: #, Command, Exit Code, Verdict, Duration.
///
/// # Arguments
/// * `result` - Verification result to format
///
/// # Returns
/// Markdown table as string
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestra::verification_evidence::*;
///
/// let result = VerificationResult {
///     passed: true,
///     checks: vec![],
///     discovery_source: DiscoverySource::Preference,
///     timestamp: 1234567890,
///     runtime_errors: None,
///     audit_warnings: None,
/// };
///
/// let table = format_evidence_table(&result);
/// assert_eq!(table, "_No verification checks discovered._");
/// ```
pub fn format_evidence_table(result: &VerificationResult) -> String {
    if result.checks.is_empty() {
        return "_No verification checks discovered._".to_string();
    }

    let mut lines = vec![
        "| # | Command | Exit Code | Verdict | Duration |".to_string(),
        "|---|---------|-----------|---------|----------|".to_string(),
    ];

    for (i, check) in result.checks.iter().enumerate() {
        let num = i + 1;
        let verdict = if check.exit_code == 0 {
            "✅ pass"
        } else {
            "❌ fail"
        };
        let duration = format_duration_secs(check.duration_ms);

        lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            num, check.command, check.exit_code, verdict, duration
        ));
    }

    if let Some(errors) = &result.runtime_errors {
        if !errors.is_empty() {
            lines.push(String::new());
            lines.push("**Runtime Errors**".to_string());
            lines.push(String::new());
            lines.push("| # | Source | Severity | Blocking | Message |".to_string());
            lines.push("|---|--------|----------|----------|---------|".to_string());
            for (i, err) in errors.iter().enumerate() {
                let block_icon = if err.blocking {
                    "🚫 yes"
                } else {
                    "ℹ️ no"
                };
                let message = if err.message.len() > 100 {
                    format!("{}...", err.message.chars().take(97).collect::<String>())
                } else {
                    err.message.clone()
                };
                lines.push(format!(
                    "| {} | {} | {} | {} | {} |",
                    i + 1,
                    err.source,
                    err.severity,
                    block_icon,
                    message
                ));
            }
        }
    }

    if let Some(warnings) = &result.audit_warnings {
        if !warnings.is_empty() {
            lines.push(String::new());
            lines.push("**Audit Warnings**".to_string());
            lines.push(String::new());
            lines.push("| # | Package | Severity | Title | Fix Available |".to_string());
            lines.push("|---|---------|----------|-------|---------------|".to_string());
            for (i, w) in warnings.iter().enumerate() {
                let emoji = match w.severity {
                    AuditSeverity::Critical => "🔴",
                    AuditSeverity::High => "🟠",
                    AuditSeverity::Moderate => "🟡",
                    AuditSeverity::Low => "⚪",
                };
                let fix = if w.fix_available { "✅ yes" } else { "❌ no" };
                lines.push(format!(
                    "| {} | {} | {} {} | {} | {} |",
                    i + 1,
                    w.name,
                    emoji,
                    w.severity,
                    w.title,
                    fix
                ));
            }
        }
    }

    lines.join("\n")
}

// ─── Internals ─────────────────────────────────────────────────────────────

/// Format duration in milliseconds as seconds with 1 decimal place.
///
/// # Arguments
/// * `ms` - Duration in milliseconds
///
/// # Returns
/// Formatted string (e.g., "2.3s", "0.2s", "0.0s")
fn format_duration_secs(ms: u64) -> String {
    let secs = ms as f64 / 1000.0;
    // Round to 1 decimal place to match TypeScript behavior
    let rounded = (secs * 10.0).round() / 10.0;
    format!("{:.1}s", rounded)
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_secs() {
        assert_eq!(format_duration_secs(2340), "2.3s");
        assert_eq!(format_duration_secs(150), "0.2s");
        assert_eq!(format_duration_secs(0), "0.0s");
        assert_eq!(format_duration_secs(1000), "1.0s");
        assert_eq!(format_duration_secs(1234), "1.2s");
    }

    #[test]
    fn test_format_evidence_table_no_checks() {
        let result = VerificationResult {
            passed: true,
            checks: vec![],
            discovery_source: DiscoverySource::None,
            timestamp: 0,
            runtime_errors: None,
            audit_warnings: None,
        };

        let table = format_evidence_table(&result);
        assert_eq!(table, "_No verification checks discovered._");
    }

    #[test]
    fn test_format_evidence_table_with_checks() {
        let result = VerificationResult {
            passed: true,
            checks: vec![
                VerificationCheck {
                    command: "npm test".to_string(),
                    exit_code: 0,
                    stdout: "pass".to_string(),
                    stderr: String::new(),
                    duration_ms: 2340,
                },
                VerificationCheck {
                    command: "npm run lint".to_string(),
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: "error".to_string(),
                    duration_ms: 150,
                },
            ],
            discovery_source: DiscoverySource::Preference,
            timestamp: 0,
            runtime_errors: None,
            audit_warnings: None,
        };

        let table = format_evidence_table(&result);
        assert!(table.contains("| 1 | npm test | 0 | ✅ pass | 2.3s |"));
        assert!(table.contains("| 2 | npm run lint | 1 | ❌ fail | 0.2s |"));
    }

    #[test]
    fn test_format_evidence_table_with_runtime_errors() {
        // Must have at least one check for runtime errors to be displayed
        let result = VerificationResult {
            passed: false,
            checks: vec![VerificationCheck {
                command: "npm test".to_string(),
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 100,
            }],
            discovery_source: DiscoverySource::None,
            timestamp: 0,
            runtime_errors: Some(vec![RuntimeError {
                source: RuntimeErrorSource::BgShell,
                severity: RuntimeErrorSeverity::Crash,
                message: "Process crashed".to_string(),
                blocking: true,
            }]),
            audit_warnings: None,
        };

        let table = format_evidence_table(&result);
        assert!(table.contains("**Runtime Errors**"));
        assert!(table.contains("| 1 | bg-shell | crash | 🚫 yes | Process crashed |"));
    }

    #[test]
    fn test_format_evidence_table_with_audit_warnings() {
        // Must have at least one check for audit warnings to be displayed
        let result = VerificationResult {
            passed: true,
            checks: vec![VerificationCheck {
                command: "npm test".to_string(),
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 100,
            }],
            discovery_source: DiscoverySource::None,
            timestamp: 0,
            runtime_errors: None,
            audit_warnings: Some(vec![AuditWarning {
                name: "lodash".to_string(),
                severity: AuditSeverity::High,
                title: "Prototype Pollution".to_string(),
                url: "https://npmjs.com/advisories/1234".to_string(),
                fix_available: true,
            }]),
        };

        let table = format_evidence_table(&result);
        assert!(table.contains("**Audit Warnings**"));
        assert!(table.contains("| 1 | lodash | 🟠 high | Prototype Pollution | ✅ yes |"));
    }

    #[test]
    fn test_format_evidence_table_truncates_long_messages() {
        // Must have at least one check for runtime errors to be displayed
        let result = VerificationResult {
            passed: false,
            checks: vec![
                VerificationCheck {
                    command: "npm test".to_string(),
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration_ms: 100,
                },
            ],
            discovery_source: DiscoverySource::None,
            timestamp: 0,
            runtime_errors: Some(vec![
                RuntimeError {
                    source: RuntimeErrorSource::Browser,
                    severity: RuntimeErrorSeverity::Error,
                    message: "This is a very long error message that should be truncated because it exceeds one hundred characters in length".to_string(),
                    blocking: false,
                },
            ]),
            audit_warnings: None,
        };

        let table = format_evidence_table(&result);
        // Message is 110 chars, truncated to 97 chars + "..." = 100 chars total
        assert!(table.contains("This is a very long error message that should be truncated because it exceeds one hundred charact..."));
        assert!(!table.contains("ers in length"));
    }
}
