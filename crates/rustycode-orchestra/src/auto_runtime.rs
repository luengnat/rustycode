//! Top-level autonomous loop orchestration for Autonomous Mode.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::anyhow;
use tracing::{debug, error, info, warn};

use crate::auto_supervisor::is_shutdown_requested;
use crate::crash_recovery::{ActivityEvent, ActivityType, CrashLock, SessionForensics};
use crate::post_unit_runtime::{
    apply_metrics_and_budget, finalize_successful_task, ExecutionStats,
};
use crate::skill_discovery::{
    clear_skill_snapshot, detect_new_skills, snapshot_skills, DiscoveredSkill,
};
use crate::unit_lifecycle_runtime::{complete_slice, validate_milestone};
use crate::verification_retry_state::{
    clear_pending_verification_retry, fingerprint_task_plan, is_stale_pending_verification_retry,
    load_pending_verification_retry, render_pending_verification_retry_context,
    retry_matches_task_plan, retry_state_age_ms,
};
use crate::Orchestra2Executor;
use rustycode_llm::TaskType;

fn apply_recovered_briefing(
    unit_id: &str,
    task_plan: String,
    recovered_briefing: Option<(String, String)>,
) -> String {
    let Some((recovered_unit_id, briefing)) = recovered_briefing else {
        return task_plan;
    };

    if recovered_unit_id == unit_id {
        format!("{}\n\n{}", briefing, task_plan)
    } else {
        task_plan
    }
}

async fn persist_recovery_briefing(
    project_root: &Path,
    unit_id: &str,
    briefing: &str,
) -> anyhow::Result<PathBuf> {
    let recovery_dir = project_root.join(".orchestra").join("recovery");
    tokio::fs::create_dir_all(&recovery_dir).await?;
    let recovery_path = recovery_dir.join(format!("{}-RECOVERY.md", unit_id));
    tokio::fs::write(&recovery_path, briefing).await?;
    Ok(recovery_path)
}

impl Orchestra2Executor {
    /// Main execution loop
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("🚀 Autonomous Mode Auto Mode Starting");
        info!("📁 Project: {}", self.project_root.display());

        let mut recovered_briefing = self.preflight_crash_recovery().await?;

        let agent_dir =
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                .join(".claude");
        if let Err(e) = snapshot_skills(&agent_dir) {
            debug!("Failed to snapshot skills: {}", e);
        } else {
            info!("🔍 Skill discovery enabled");
        }

        loop {
            if is_shutdown_requested() {
                info!("🛑 Shutdown requested, stopping Autonomous Mode auto mode");
                break;
            }

            let state = self.state_deriver.derive_state()?;
            info!("📍 Phase: {}", state.phase.as_str());

            let (unit_id, unit_type) = match state.phase {
                crate::phases::Phase::Plan => {
                    if let Some(ref slice) = state.active_slice {
                        (slice.id.clone(), "plan-slice")
                    } else {
                        info!("✅ No active slice, complete");
                        break;
                    }
                }
                crate::phases::Phase::Execute => {
                    if let Some(ref task) = state.active_task {
                        (task.id.clone(), "execute-task")
                    } else {
                        info!("✅ No active task, moving to complete");
                        continue;
                    }
                }
                crate::phases::Phase::Complete => {
                    if let Some(ref slice) = state.active_slice {
                        (slice.id.clone(), "complete-slice")
                    } else {
                        info!("✅ No active slice, moving to validation");
                        continue;
                    }
                }
                crate::phases::Phase::Validate => {
                    if let Some(ref milestone) = state.active_milestone {
                        (milestone.id.clone(), "validate-milestone")
                    } else {
                        info!("✅ No active milestone, complete");
                        break;
                    }
                }
                _ => {
                    warn!("Unknown phase: {:?}", state.phase);
                    break;
                }
            };

            info!("▶️  Current unit: {} ({})", unit_id, unit_type);

            if let Err(e) = self.state_deriver.write_state_cache(&state) {
                warn!("Failed to write STATE.md cache: {}", e);
            }

            {
                let budget = self
                    .budget_tracker
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let remaining = budget.remaining();
                info!("💰 Budget remaining: ${:.2}", remaining);
            }

            let lock = CrashLock::new(&unit_id, "execute");
            lock.write_lock(&self.project_root)?;

            self.activity_log
                .log(ActivityEvent {
                    timestamp: chrono::Utc::now(),
                    unit_id: unit_id.clone(),
                    event_type: ActivityType::SessionStart,
                    detail: serde_json::json!({"unit": unit_id}),
                })
                .await?;

            let base_task_plan = if let Some(ref task) = state.active_task {
                let plan_path = task.path.join(format!("{}-PLAN.md", unit_id));
                if plan_path.exists() {
                    tokio::fs::read_to_string(&plan_path)
                        .await
                        .unwrap_or_else(|_| format!("# Task {}\n\nNo plan found.", unit_id))
                } else {
                    format!(
                        "# Task {}\n\nNo plan file found. Execute the task based on its ID.",
                        unit_id
                    )
                }
            } else {
                format!("# Task {}\n\nExecute this task.", unit_id)
            };

            let task_plan_fingerprint = fingerprint_task_plan(&base_task_plan);
            let mut task_plan = if unit_type == "execute-task" {
                match load_pending_verification_retry(&self.project_root, &unit_id) {
                    Ok(Some(retry)) => {
                        const MAX_PENDING_RETRY_AGE_MS: u64 = 24 * 60 * 60 * 1000;
                        let retry_age_ms = retry_state_age_ms(&retry);
                        if is_stale_pending_verification_retry(&retry, MAX_PENDING_RETRY_AGE_MS) {
                            warn!(
                                "Pending verification retry for {} is stale (age {}ms > {}ms); clearing",
                                unit_id,
                                retry_age_ms,
                                MAX_PENDING_RETRY_AGE_MS
                            );
                            let _ = self
                                .activity_log
                                .log(ActivityEvent {
                                    timestamp: chrono::Utc::now(),
                                    unit_id: unit_id.clone(),
                                    event_type: ActivityType::Error,
                                    detail: serde_json::json!({
                                        "kind": "verification_retry_discarded",
                                        "reason": "stale",
                                        "age_ms": retry_age_ms,
                                        "max_age_ms": MAX_PENDING_RETRY_AGE_MS,
                                        "plan_fingerprint": &retry.plan_fingerprint,
                                    }),
                                })
                                .await;
                            if let Err(e) =
                                clear_pending_verification_retry(&self.project_root, &unit_id)
                            {
                                warn!(
                                    "Failed to clear stale verification retry for {}: {}",
                                    unit_id, e
                                );
                            }
                            base_task_plan
                        } else if !retry_matches_task_plan(&retry, &base_task_plan) {
                            warn!(
                                "Pending verification retry for {} does not match the current task plan (stored fingerprint: {}, current fingerprint: {}); clearing",
                                unit_id,
                                retry.plan_fingerprint,
                                task_plan_fingerprint
                            );
                            let _ = self
                                .activity_log
                                .log(ActivityEvent {
                                    timestamp: chrono::Utc::now(),
                                    unit_id: unit_id.clone(),
                                    event_type: ActivityType::Error,
                                    detail: serde_json::json!({
                                        "kind": "verification_retry_discarded",
                                        "reason": "plan_mismatch",
                                        "stored_fingerprint": &retry.plan_fingerprint,
                                        "current_fingerprint": &task_plan_fingerprint,
                                    }),
                                })
                                .await;
                            if let Err(e) =
                                clear_pending_verification_retry(&self.project_root, &unit_id)
                            {
                                warn!(
                                    "Failed to clear mismatched verification retry for {}: {}",
                                    unit_id, e
                                );
                            }
                            base_task_plan
                        } else {
                            format!(
                                "{}\n\n{}",
                                base_task_plan,
                                render_pending_verification_retry_context(&retry)
                            )
                        }
                    }
                    Ok(None) => base_task_plan,
                    Err(e) => {
                        warn!(
                            "Failed to load pending verification retry for {}: {}",
                            unit_id, e
                        );
                        base_task_plan
                    }
                }
            } else {
                base_task_plan
            };

            if let Some((recovered_unit_id, _)) = recovered_briefing.as_ref() {
                if recovered_unit_id == &unit_id {
                    info!("♻️  Applying crash recovery briefing for {}", unit_id);
                } else {
                    warn!(
                        "Recovered crash briefing was for {}, but current unit is {}; skipping resume context",
                        recovered_unit_id, unit_id
                    );
                }
            }
            task_plan = apply_recovered_briefing(&unit_id, task_plan, recovered_briefing.take());
            info!("📋 Task plan loaded: {} bytes", task_plan.len());

            let agent_dir =
                std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                    .join(".claude");
            let discovered_skills: Vec<DiscoveredSkill> = match detect_new_skills(&agent_dir) {
                Ok(skills) => {
                    if !skills.is_empty() {
                        info!("🆕 Detected {} new skills", skills.len());
                        for skill in &skills {
                            debug!("  - {}", skill.name);
                        }
                    }
                    skills
                }
                Err(e) => {
                    debug!("Skill detection failed: {}", e);
                    Vec::new()
                }
            };

            let start_time = Instant::now();
            let mut timeout_state = self.timeout_supervisor.start_unit(&unit_id);

            // Select appropriate model based on task type for cost optimization
            let task_type = match unit_type {
                "plan-slice" => TaskType::Planning,
                "execute-task" => TaskType::CodeGeneration,
                "validate-milestone" => TaskType::Research,
                "complete-slice" => TaskType::General,
                _ => TaskType::General,
            };

            let original_model = self.get_model();
            let target_model = self.select_model_for_task(task_type);

            if target_model != original_model {
                self.set_model(target_model.clone());
                info!(
                    "🎯 Model routing: {} → {} (task: {:?})",
                    original_model, target_model, task_type
                );
            }

            let execution_stats = match unit_type {
                "plan-slice" => {
                    let milestone_id = state
                        .active_milestone
                        .as_ref()
                        .map(|m| m.id.as_str())
                        .unwrap_or("M01");
                    let slice_title = state
                        .active_slice
                        .as_ref()
                        .map(|s| s.title.as_str())
                        .unwrap_or("Unknown Slice");
                    self.execute_plan_slice(
                        &unit_id,
                        milestone_id,
                        slice_title,
                        &mut timeout_state,
                        &discovered_skills,
                    )
                    .await
                    .map(|_| ExecutionStats::default())
                }
                "complete-slice" => {
                    let milestone_id = state
                        .active_milestone
                        .as_ref()
                        .map(|m| m.id.as_str())
                        .unwrap_or("M01");
                    let slice_title = state
                        .active_slice
                        .as_ref()
                        .map(|s| s.title.as_str())
                        .unwrap_or("Unknown Slice");
                    complete_slice(&self.project_root, &unit_id, milestone_id, slice_title)
                        .map(|_| ExecutionStats::default())
                }
                "validate-milestone" => {
                    let milestone_id = state
                        .active_milestone
                        .as_ref()
                        .map(|m| m.id.as_str())
                        .unwrap_or("M01");
                    let milestone_title = state
                        .active_milestone
                        .as_ref()
                        .map(|m| m.title.as_str())
                        .unwrap_or("Unknown Milestone");
                    validate_milestone(&self.project_root, milestone_id, milestone_title)
                        .map(|_| ExecutionStats::default())
                }
                _ => {
                    self.execute_unit(&unit_id, &task_plan, &mut timeout_state, &discovered_skills)
                        .await
                }
            };

            // Restore default model after execution
            if target_model != original_model {
                self.restore_default_model();
                debug!("🔄 Restored model to {}", original_model);
            }

            let duration_ms = start_time.elapsed().as_millis() as u64;

            let execution_success = execution_stats.is_ok();
            let execution_error = execution_stats.as_ref().err().map(|e| e.to_string());
            let mut execution_stats = execution_stats.unwrap_or_default();

            let verification_passed = if unit_type == "execute-task" && execution_success {
                match self
                    .run_task_verification_with_retries(
                        &state,
                        &unit_id,
                        &task_plan,
                        &task_plan_fingerprint,
                        &mut execution_stats,
                        &mut timeout_state,
                        &discovered_skills,
                    )
                    .await
                {
                    Ok(outcome) => outcome.passed,
                    Err(e) => {
                        warn!("Verification flow failed for unit {}: {}", unit_id, e);
                        false
                    }
                }
            } else {
                execution_success
            };

            apply_metrics_and_budget(
                &self.metrics_ledger,
                &self.budget_tracker,
                &self.get_model(),
                &unit_id,
                &execution_stats,
                duration_ms,
                verification_passed,
            )?;

            if verification_passed {
                if let Err(e) = finalize_successful_task(&self.project_root, &unit_id, &state).await
                {
                    warn!("Failed to finalize successful task {}: {}", unit_id, e);
                }
            }

            CrashLock::clear_lock(&self.project_root)?;

            self.activity_log
                .log(ActivityEvent {
                    timestamp: chrono::Utc::now(),
                    unit_id: unit_id.clone(),
                    event_type: ActivityType::SessionEnd,
                    detail: serde_json::json!({
                        "unit": unit_id,
                        "success": execution_success,
                        "duration_ms": duration_ms,
                    }),
                })
                .await?;

            if execution_success {
                info!("✅ Unit {} complete ({}ms)", unit_id, duration_ms);
            } else if let Some(err) = execution_error {
                error!("❌ Unit {} failed: {}", unit_id, err);
                return Err(anyhow!(err));
            }
        }

        clear_skill_snapshot();
        info!("🏁 Autonomous Mode Auto Mode Complete");
        Ok(())
    }

    async fn preflight_crash_recovery(&self) -> anyhow::Result<Option<(String, String)>> {
        let Some(lock) = CrashLock::read_lock(&self.project_root)? else {
            return Ok(None);
        };

        if lock.is_process_alive() {
            return Err(anyhow!(
                "Another Orchestra session appears to be active: {}",
                lock.format_crash_info()
            ));
        }

        warn!("Recovering stale crash lock: {}", lock.format_crash_info());

        let briefing = match SessionForensics::new(self.project_root.clone())
            .synthesize_recovery(&lock.unit_id)
            .await
        {
            Ok(briefing) => briefing,
            Err(e) => {
                warn!(
                    "Failed to synthesize crash recovery briefing for {}: {}",
                    lock.unit_id, e
                );
                lock.format_crash_info()
            }
        };

        match persist_recovery_briefing(&self.project_root, &lock.unit_id, &briefing).await {
            Ok(recovery_path) => {
                info!(
                    "📝 Crash recovery briefing persisted at {}",
                    recovery_path.display()
                );
            }
            Err(e) => {
                warn!(
                    "Failed to persist crash recovery briefing for {}: {}",
                    lock.unit_id, e
                );
            }
        }

        let _ = self
            .activity_log
            .log(ActivityEvent {
                timestamp: chrono::Utc::now(),
                unit_id: lock.unit_id.clone(),
                event_type: ActivityType::Error,
                detail: serde_json::json!({
                    "kind": "crash_lock_recovered",
                    "unit_id": lock.unit_id,
                    "phase": lock.phase,
                    "pid": lock.pid,
                    "started_at": lock.start_time.to_rfc3339(),
                    "briefing": briefing,
                }),
            })
            .await;

        CrashLock::clear_lock(&self.project_root)?;
        Ok(Some((lock.unit_id.clone(), briefing)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_bootstrap::bootstrap_default_project;
    use chrono::{Duration, Utc};
    use rustycode_llm::MockProvider;
    use std::sync::Arc;

    #[tokio::test]
    async fn preflight_crash_recovery_clears_stale_lock() {
        let temp = tempfile::tempdir().unwrap();
        let project_root = temp.path().to_path_buf();

        bootstrap_default_project(&project_root).await.unwrap();

        let executor = Orchestra2Executor::new(
            project_root.clone(),
            Arc::new(MockProvider::from_text("mock")),
            "mock".to_string(),
            10.0,
        );

        // Use a PID guaranteed not to exist on any system (i32::MAX = 2147483647).
        // Using a real PID like 1234 causes flaky tests when that PID happens to exist.
        let stale_lock = CrashLock {
            unit_id: "T01".to_string(),
            pid: i32::MAX as u32,
            start_time: Utc::now() - Duration::hours(2),
            phase: "execute".to_string(),
        };
        stale_lock.write_lock(&project_root).unwrap();

        executor.preflight_crash_recovery().await.unwrap();
        assert!(CrashLock::read_lock(&project_root).unwrap().is_none());
    }

    #[tokio::test]
    async fn preflight_crash_recovery_returns_briefing_for_resume() {
        let temp = tempfile::tempdir().unwrap();
        let project_root = temp.path().to_path_buf();

        bootstrap_default_project(&project_root).await.unwrap();

        let activity_log = crate::crash_recovery::ActivityLog::new(project_root.clone());
        activity_log
            .log(ActivityEvent {
                timestamp: Utc::now(),
                unit_id: "T01".to_string(),
                event_type: ActivityType::SessionStart,
                detail: serde_json::json!({"unit": "T01"}),
            })
            .await
            .unwrap();
        activity_log
            .log(ActivityEvent {
                timestamp: Utc::now(),
                unit_id: "T01".to_string(),
                event_type: ActivityType::ToolUse,
                detail: serde_json::json!({"tool": "bash", "command": "cargo test"}),
            })
            .await
            .unwrap();

        let executor = Orchestra2Executor::new(
            project_root.clone(),
            Arc::new(MockProvider::from_text("mock")),
            "mock".to_string(),
            10.0,
        );

        let stale_lock = CrashLock {
            unit_id: "T01".to_string(),
            pid: i32::MAX as u32,
            start_time: Utc::now() - Duration::hours(2),
            phase: "execute".to_string(),
        };
        stale_lock.write_lock(&project_root).unwrap();

        let recovered = executor.preflight_crash_recovery().await.unwrap();
        let (unit_id, briefing) = recovered.expect("expected recovery briefing");
        assert_eq!(unit_id, "T01");
        assert!(briefing.contains("CRASH RECOVERY BRIEFING"));
        assert!(briefing.contains("T01"));
        let recovery_path = project_root.join(".orchestra/recovery/T01-RECOVERY.md");
        let persisted = tokio::fs::read_to_string(&recovery_path).await.unwrap();
        assert!(persisted.contains("CRASH RECOVERY BRIEFING"));
        assert!(CrashLock::read_lock(&project_root).unwrap().is_none());
    }

    #[test]
    fn apply_recovered_briefing_only_prefixes_matching_unit() {
        let task_plan = "Do the thing".to_string();
        let briefing = "=== CRASH RECOVERY BRIEFING ===\nResume carefully.".to_string();

        let applied = apply_recovered_briefing(
            "T01",
            task_plan.clone(),
            Some(("T01".to_string(), briefing.clone())),
        );
        assert_eq!(applied, format!("{}\n\n{}", briefing, task_plan));

        let skipped = apply_recovered_briefing(
            "T02",
            task_plan.clone(),
            Some(("T01".to_string(), briefing)),
        );
        assert_eq!(skipped, task_plan);
    }
}
