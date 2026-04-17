// rustycode-orchestra/src/error.rs
//! Error types for Orchestra v2
//!
//! Provides a typed error hierarchy for diagnostics and crash recovery.
//! All Orchestra-specific errors carry a stable `code` string suitable for
//! programmatic matching.

use std::path::PathBuf;
use thiserror::Error;

// ─── Error Codes ─────────────────────────────────────────────────────────────

/// Error code constants for programmatic matching
pub mod codes {
    pub const ORCHESTRA_STALE_STATE: &str = "ORCHESTRA_STALE_STATE";
    pub const ORCHESTRA_LOCK_HELD: &str = "ORCHESTRA_LOCK_HELD";
    pub const ORCHESTRA_ARTIFACT_MISSING: &str = "ORCHESTRA_ARTIFACT_MISSING";
    pub const ORCHESTRA_GIT_ERROR: &str = "ORCHESTRA_GIT_ERROR";
    pub const ORCHESTRA_MERGE_CONFLICT: &str = "ORCHESTRA_MERGE_CONFLICT";
    pub const ORCHESTRA_PARSE_ERROR: &str = "ORCHESTRA_PARSE_ERROR";
    pub const ORCHESTRA_IO_ERROR: &str = "ORCHESTRA_IO_ERROR";
}

// ─── Base Error ───────────────────────────────────────────────────────────────

/// Orchestra v2 error types with stable error codes
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum OrchestraV2Error {
    /// Project not initialized
    #[error("Project not initialized: run 'orchestra init' first")]
    NotInitialized,

    /// Project already initialized
    #[error("Project already initialized at {0}")]
    AlreadyInitialized(PathBuf),

    /// Phase not found
    #[error("Phase not found: {0}")]
    PhaseNotFound(String),

    /// Task not found
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    /// Invalid state
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// LLM error
    #[error("LLM error: {0}")]
    Llm(String),

    /// Tool execution error
    #[error("Tool execution error: {0}")]
    ToolExecution(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Parsing error
    #[error("Parsing error: {0}")]
    Parse(String),

    /// Checkpoint required
    #[error("Checkpoint required: {0}")]
    CheckpointRequired(String),

    /// Dependency error
    #[error("Dependency error: {0}")]
    Dependency(String),

    /// Auto-mode error
    #[error("Auto-mode error: {0}")]
    AutoMode(String),

    /// Debug session error
    #[error("Debug session error: {0}")]
    DebugSession(String),

    /// Progress tracking error
    #[error("Progress tracking error: {0}")]
    Progress(String),

    /// Task execution error
    #[error("Task execution error: {0}")]
    TaskExecution(String),

    /// Model routing error
    #[error("Model routing error: {0}")]
    ModelRouting(String),

    /// LLM integration error
    #[error("LLM integration error: {0}")]
    LlmIntegration(String),

    /// Worktree error
    #[error("Worktree error: {0}")]
    Worktree(String),

    /// Stale state error
    #[error("Stale state: {0}")]
    StaleState(String),

    /// Lock held error
    #[error("Lock held: {0}")]
    LockHeld(String),

    /// Artifact missing error
    #[error("Artifact missing: {0}")]
    ArtifactMissing(String),

    /// Git error
    #[error("Git error: {0}")]
    Git(String),

    /// Merge conflict error
    #[error("Merge conflict: {0}")]
    MergeConflict(String),

    /// IO error with context
    #[error("{context}: {source}")]
    IoError {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// Prompt load error
    #[error("Prompt load error for '{template}': missing variables: {missing:?}. {hint}")]
    PromptLoadError {
        template: String,
        missing: Vec<String>,
        hint: String,
    },
}

impl OrchestraV2Error {
    /// Get the stable error code for this error
    pub fn code(&self) -> &str {
        match self {
            OrchestraV2Error::StaleState(_) => codes::ORCHESTRA_STALE_STATE,
            OrchestraV2Error::LockHeld(_) => codes::ORCHESTRA_LOCK_HELD,
            OrchestraV2Error::ArtifactMissing(_) => codes::ORCHESTRA_ARTIFACT_MISSING,
            OrchestraV2Error::Git(_) => codes::ORCHESTRA_GIT_ERROR,
            OrchestraV2Error::MergeConflict(_) => codes::ORCHESTRA_MERGE_CONFLICT,
            OrchestraV2Error::Parse(_) => codes::ORCHESTRA_PARSE_ERROR,
            OrchestraV2Error::Io(_) => codes::ORCHESTRA_IO_ERROR,
            _ => "Orchestra_UNKNOWN",
        }
    }

    /// Create a stale state error
    pub fn stale_state(msg: impl Into<String>) -> Self {
        OrchestraV2Error::StaleState(msg.into())
    }

    /// Create a lock held error
    pub fn lock_held(msg: impl Into<String>) -> Self {
        OrchestraV2Error::LockHeld(msg.into())
    }

    /// Create an artifact missing error
    pub fn artifact_missing(msg: impl Into<String>) -> Self {
        OrchestraV2Error::ArtifactMissing(msg.into())
    }

    /// Create a git error
    pub fn git(msg: impl Into<String>) -> Self {
        OrchestraV2Error::Git(msg.into())
    }

    /// Create a merge conflict error
    pub fn merge_conflict(msg: impl Into<String>) -> Self {
        OrchestraV2Error::MergeConflict(msg.into())
    }
}

/// Orchestra v2 result type
pub type Result<T> = std::result::Result<T, OrchestraV2Error>;

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes_defined() {
        assert_eq!(codes::ORCHESTRA_STALE_STATE, "ORCHESTRA_STALE_STATE");
        assert_eq!(codes::ORCHESTRA_LOCK_HELD, "ORCHESTRA_LOCK_HELD");
        assert_eq!(
            codes::ORCHESTRA_ARTIFACT_MISSING,
            "ORCHESTRA_ARTIFACT_MISSING"
        );
        assert_eq!(codes::ORCHESTRA_GIT_ERROR, "ORCHESTRA_GIT_ERROR");
        assert_eq!(codes::ORCHESTRA_MERGE_CONFLICT, "ORCHESTRA_MERGE_CONFLICT");
        assert_eq!(codes::ORCHESTRA_PARSE_ERROR, "ORCHESTRA_PARSE_ERROR");
        assert_eq!(codes::ORCHESTRA_IO_ERROR, "ORCHESTRA_IO_ERROR");
    }

    #[test]
    fn test_error_code_method() {
        let err = OrchestraV2Error::stale_state("test message");
        assert_eq!(err.code(), codes::ORCHESTRA_STALE_STATE);

        let err = OrchestraV2Error::lock_held("test message");
        assert_eq!(err.code(), codes::ORCHESTRA_LOCK_HELD);

        let err = OrchestraV2Error::artifact_missing("test message");
        assert_eq!(err.code(), codes::ORCHESTRA_ARTIFACT_MISSING);

        let err = OrchestraV2Error::git("test message");
        assert_eq!(err.code(), codes::ORCHESTRA_GIT_ERROR);

        let err = OrchestraV2Error::merge_conflict("test message");
        assert_eq!(err.code(), codes::ORCHESTRA_MERGE_CONFLICT);

        let err = OrchestraV2Error::Parse("test message".to_string());
        assert_eq!(err.code(), codes::ORCHESTRA_PARSE_ERROR);

        let err = OrchestraV2Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "test"));
        assert_eq!(err.code(), codes::ORCHESTRA_IO_ERROR);
    }

    #[test]
    fn test_error_constructors() {
        let err = OrchestraV2Error::stale_state("state is stale");
        assert!(matches!(err, OrchestraV2Error::StaleState(_)));
        assert_eq!(err.to_string(), "Stale state: state is stale");

        let err = OrchestraV2Error::lock_held("lock is held");
        assert!(matches!(err, OrchestraV2Error::LockHeld(_)));
        assert_eq!(err.to_string(), "Lock held: lock is held");

        let err = OrchestraV2Error::artifact_missing("artifact not found");
        assert!(matches!(err, OrchestraV2Error::ArtifactMissing(_)));
        assert_eq!(err.to_string(), "Artifact missing: artifact not found");

        let err = OrchestraV2Error::git("git failed");
        assert!(matches!(err, OrchestraV2Error::Git(_)));
        assert_eq!(err.to_string(), "Git error: git failed");

        let err = OrchestraV2Error::merge_conflict("conflict detected");
        assert!(matches!(err, OrchestraV2Error::MergeConflict(_)));
        assert_eq!(err.to_string(), "Merge conflict: conflict detected");
    }

    #[test]
    fn test_unknown_error_code() {
        let err = OrchestraV2Error::NotInitialized;
        assert_eq!(err.code(), "Orchestra_UNKNOWN");

        let err = OrchestraV2Error::Llm("LLM failed".to_string());
        assert_eq!(err.code(), "Orchestra_UNKNOWN");
    }

    #[test]
    fn test_result_type() {
        fn returns_ok() -> Result<String> {
            Ok("success".to_string())
        }

        fn returns_err() -> Result<String> {
            Err(OrchestraV2Error::stale_state("test"))
        }

        assert!(returns_ok().is_ok());
        assert!(returns_err().is_err());
        assert_eq!(returns_ok().unwrap(), "success");
        assert_eq!(
            returns_err().unwrap_err().code(),
            codes::ORCHESTRA_STALE_STATE
        );
    }
}
