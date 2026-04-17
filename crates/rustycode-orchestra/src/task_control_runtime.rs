//! Deterministic execute-task control decisions for the Orchestra runtime.
//!
//! The executor still performs retry execution, but this module owns the
//! verification/evidence step and the pass/fail/retry decision.

use std::path::Path;

use anyhow::Result;
use tracing::{info, warn};

use crate::task_verification_runtime::run_task_verification;
use crate::verification_retry_state::PendingVerificationRetry;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TaskControlDecision {
    Passed,
    Failed,
    Retry {
        next_plan: String,
        failure_context: String,
        attempt: u32,
    },
}

const MAX_RETRY_CONTEXT_CHARS: usize = 8_000;

pub fn evaluate_task_attempt(
    project_root: &Path,
    unit_id: &str,
    task_plan: &str,
    task_dir: &Path,
    attempt: u32,
    max_retries: u32,
) -> Result<TaskControlDecision> {
    let verification = run_task_verification(
        project_root,
        unit_id,
        task_plan,
        task_dir,
        attempt,
        max_retries,
    )?;

    Ok(decide_from_verification(
        unit_id,
        task_plan,
        verification.passed,
        &verification.failure_context,
        attempt,
        max_retries,
    ))
}

fn decide_from_verification(
    unit_id: &str,
    task_plan: &str,
    passed: bool,
    failure_context: &str,
    attempt: u32,
    max_retries: u32,
) -> TaskControlDecision {
    if passed {
        info!("✅ Verification passed for unit: {}", unit_id);
        return TaskControlDecision::Passed;
    }

    warn!("❌ Verification failed for unit: {}", unit_id);
    if !failure_context.is_empty() {
        warn!("{}", failure_context);
    }

    if attempt >= max_retries {
        TaskControlDecision::Failed
    } else {
        TaskControlDecision::Retry {
            next_plan: build_retry_plan(task_plan, failure_context, attempt, max_retries),
            failure_context: failure_context.to_string(),
            attempt,
        }
    }
}

fn build_retry_plan(
    task_plan: &str,
    failure_context: &str,
    attempt: u32,
    max_retries: u32,
) -> String {
    let capped_context = cap_retry_context(failure_context);
    let retry = PendingVerificationRetry {
        unit_id: String::new(),
        plan_fingerprint: String::new(),
        failure_context: capped_context,
        attempt,
        max_retries,
        created_at_ms: 0,
        updated_at_ms: 0,
    };

    format!(
        "{}\n\n{}",
        task_plan,
        crate::verification_retry_state::render_pending_verification_retry_context(&retry)
    )
}

fn cap_retry_context(failure_context: &str) -> String {
    if failure_context.chars().count() > MAX_RETRY_CONTEXT_CHARS {
        let mut truncated = failure_context
            .chars()
            .take(MAX_RETRY_CONTEXT_CHARS)
            .collect::<String>();
        truncated.push_str("\n\n[...failure context truncated]");
        truncated
    } else {
        failure_context.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_passed_when_verification_succeeds() {
        let decision = decide_from_verification("T01", "task plan", true, "", 0, 2);

        assert!(matches!(decision, TaskControlDecision::Passed));
    }

    #[test]
    fn returns_retry_with_failure_context_when_retries_remain() {
        let decision = decide_from_verification("T01", "task plan", false, "stderr: nope", 0, 2);

        match decision {
            TaskControlDecision::Retry {
                next_plan,
                failure_context,
                attempt,
            } => {
                assert!(next_plan.contains("Verification Retry Context"));
                assert!(next_plan.contains("stderr: nope"));
                assert!(next_plan.contains("Attempt 1 needs a repair pass"));
                assert_eq!(failure_context, "stderr: nope");
                assert_eq!(attempt, 0);
            }
            other => panic!("expected retry decision, got {:?}", other),
        }
    }

    #[test]
    fn returns_failed_when_retries_exhausted() {
        let decision = decide_from_verification("T01", "task plan", false, "stderr: nope", 2, 2);

        assert!(matches!(decision, TaskControlDecision::Failed));
    }

    #[test]
    fn truncates_long_retry_context() {
        let long_context = "x".repeat(MAX_RETRY_CONTEXT_CHARS + 100);
        let next_plan = build_retry_plan("task plan", &long_context, 0, 2);

        assert!(next_plan.contains("[...failure context truncated]"));
        assert!(next_plan.len() < long_context.len() + "task plan".len() + 200);
    }
}
