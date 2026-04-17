//! Persisted state for verification retry handoff.
//!
//! When execute-task verification fails with retries remaining, we persist the
//! failure context to disk so a later session can resume with the same repair
//! prompt instead of losing that context on restart.
//!
//! # Problem
//!
//! In autonomous development, a task may pass execution but fail verification
//! (e.g., tests fail). When retries remain, we want to:
//! 1. Remember the failure across process restarts
//! 2. Preserve the repair context for the LLM
//! 3. Avoid re-executing the successful task code
//!
//! # Solution
//!
//! We persist a `PendingVerificationRetry` to `.orchestra/verification-retries/<unit>.json`
//! containing:
//! - **Plan fingerprint**: Ensures task plan hasn't changed
//! - **Failure context**: What went wrong (test output, errors)
//! - **Attempt count**: Which retry we're on
//! - **Timestamps**: For staleness detection
//!
//! # Retry Flow
//!
//! 1. Task executes successfully → verification fails
//! 2. Check if retries remain → save retry state
//! 3. On restart → load retry state
//! 4. Verify fingerprint matches → inject failure context into prompt
//! 5. LLM attempts repair → verification passes
//! 6. Clear retry state → mark task done
//!
//! # Stale Retry Detection
//!
//! Retries older than 24 hours are considered stale and discarded:
//! - Task plan may have changed
//! - Codebase may have evolved
//! - Failure context is no longer relevant
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::verification_retry_state::{
//!     save_pending_verification_retry, load_pending_verification_retry,
//!     PendingVerificationRetry
//! };
//!
//! // Save failed verification for retry
//! save_pending_verification_retry(&project_root, &PendingVerificationRetry {
//!     unit_id: "T01".to_string(),
//!     plan_fingerprint: "abc123".to_string(),
//!     failure_context: "Tests failed: assert_eq!(1, 2)".to_string(),
//!     attempt: 1,
//!     max_retries: 3,
//!     created_at_ms: 1234567890,
//!     updated_at_ms: 1234567890,
//! })?;
//!
//! // Later, load and check if still valid
//! if let Some(retry) = load_pending_verification_retry(&project_root, "T01")? {
//!     if is_stale_pending_verification_retry(&retry) {
//!         clear_pending_verification_retry(&project_root, "T01")?;
//!     }
//! }
//! ```

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingVerificationRetry {
    pub unit_id: String,
    pub plan_fingerprint: String,
    pub failure_context: String,
    pub attempt: u32,
    pub max_retries: u32,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

fn retry_state_dir(project_root: &Path) -> PathBuf {
    project_root.join(".orchestra").join("verification-retries")
}

fn retry_state_path(project_root: &Path, unit_id: &str) -> PathBuf {
    let file_name = unit_id.replace('/', "__");
    retry_state_dir(project_root).join(format!("{}.json", file_name))
}

pub fn save_pending_verification_retry(
    project_root: &Path,
    retry: &PendingVerificationRetry,
) -> Result<()> {
    std::fs::create_dir_all(retry_state_dir(project_root))?;
    let json = serde_json::to_string_pretty(retry)?;
    std::fs::write(retry_state_path(project_root, &retry.unit_id), json)?;
    Ok(())
}

pub fn load_pending_verification_retry(
    project_root: &Path,
    unit_id: &str,
) -> Result<Option<PendingVerificationRetry>> {
    let path = retry_state_path(project_root, unit_id);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    let retry = serde_json::from_str::<PendingVerificationRetry>(&content)?;
    Ok(Some(retry))
}

pub fn clear_pending_verification_retry(project_root: &Path, unit_id: &str) -> Result<()> {
    let path = retry_state_path(project_root, unit_id);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn new_pending_verification_retry(
    unit_id: impl Into<String>,
    plan_fingerprint: impl Into<String>,
    failure_context: impl Into<String>,
    attempt: u32,
    max_retries: u32,
) -> PendingVerificationRetry {
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    PendingVerificationRetry {
        unit_id: unit_id.into(),
        plan_fingerprint: plan_fingerprint.into(),
        failure_context: failure_context.into(),
        attempt,
        max_retries,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
    }
}

pub fn touch_pending_verification_retry(retry: &mut PendingVerificationRetry) {
    retry.updated_at_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
}

pub fn retry_state_age_ms(retry: &PendingVerificationRetry) -> u64 {
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    now_ms.saturating_sub(retry.updated_at_ms)
}

pub fn is_stale_pending_verification_retry(
    retry: &PendingVerificationRetry,
    max_age_ms: u64,
) -> bool {
    retry_state_age_ms(retry) > max_age_ms
}

pub fn render_pending_verification_retry_context(retry: &PendingVerificationRetry) -> String {
    format!(
        "## Verification Retry Context\n\nAttempt {} needs a repair pass before finishing.\n\n{}",
        retry.attempt + 1,
        retry.failure_context
    )
}

pub fn fingerprint_task_plan(task_plan: &str) -> String {
    let digest = Sha256::digest(task_plan.as_bytes());
    format!("{:x}", digest)
}

pub fn retry_matches_task_plan(retry: &PendingVerificationRetry, task_plan: &str) -> bool {
    retry.plan_fingerprint == fingerprint_task_plan(task_plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_pending_retry_state() {
        let temp = tempfile::tempdir().unwrap();
        let retry = PendingVerificationRetry {
            unit_id: "T01".to_string(),
            plan_fingerprint: "abc123".to_string(),
            failure_context: "stderr: nope".to_string(),
            attempt: 1,
            max_retries: 2,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        save_pending_verification_retry(temp.path(), &retry).unwrap();
        let loaded = load_pending_verification_retry(temp.path(), "T01")
            .unwrap()
            .unwrap();
        assert_eq!(loaded, retry);

        clear_pending_verification_retry(temp.path(), "T01").unwrap();
        assert!(load_pending_verification_retry(temp.path(), "T01")
            .unwrap()
            .is_none());
    }

    #[test]
    fn creates_retry_state_with_timestamps() {
        let retry = new_pending_verification_retry("T01", "abc123", "stderr: nope", 1, 2);
        assert!(retry.created_at_ms > 0);
        assert_eq!(retry.created_at_ms, retry.updated_at_ms);
    }

    #[test]
    fn detects_stale_retry_state() {
        let retry = PendingVerificationRetry {
            unit_id: "T01".to_string(),
            plan_fingerprint: "abc123".to_string(),
            failure_context: "stderr: nope".to_string(),
            attempt: 1,
            max_retries: 2,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        assert!(is_stale_pending_verification_retry(&retry, 0));
    }

    #[test]
    fn renders_retry_context_consistently() {
        let retry = PendingVerificationRetry {
            unit_id: "T01".to_string(),
            plan_fingerprint: "abc123".to_string(),
            failure_context: "stderr: nope".to_string(),
            attempt: 1,
            max_retries: 2,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let rendered = render_pending_verification_retry_context(&retry);
        assert!(rendered.contains("Attempt 2 needs a repair pass"));
        assert!(rendered.contains("stderr: nope"));
    }

    #[test]
    fn matches_plan_fingerprint() {
        let plan = "# Task T01\n\nDo the thing.";
        let retry = PendingVerificationRetry {
            unit_id: "T01".to_string(),
            plan_fingerprint: fingerprint_task_plan(plan),
            failure_context: "stderr: nope".to_string(),
            attempt: 1,
            max_retries: 2,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        assert!(retry_matches_task_plan(&retry, plan));
        assert!(!retry_matches_task_plan(
            &retry,
            "# Task T01\n\nDifferent thing."
        ));
    }
}
