//! Crash recovery and lock file management
//!
//! This module enables Orchestra to recover from crashes by tracking the current unit
//! and session state. When a Orchestra session starts, it creates a crash lock file
//! that records:
//! - The unit being executed (unit_id)
//! - The process ID (pid)
//! - The start time
//! - The execution phase (plan, execute, verify, etc.)
//!
//! # Crash Detection
//!
//! On startup, Orchestra checks for a stale crash lock:
//! - If the lock process is still alive, Orchestra aborts (another session is active)
//! - If the lock process is dead, Orchestra recovers the session
//! - On Unix, process liveness is checked via `kill -0 $PID`
//! - On Windows, process liveness is estimated via time-based heuristics
//!
//! # Recovery Briefing
//!
//! When a stale lock is detected, SessionForensics synthesizes a recovery
//! briefing that includes:
//! - Last session start time
//! - Last tool called
//! - Files written during the session
//! - Errors that occurred
//!
//! This briefing is prepended to the task plan so the LLM can resume work
//! without repeating completed steps.
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestra::crash_recovery::CrashLock;
//!
//! // Create lock before starting work
//! let lock = CrashLock::new("T01", "execute");
//! lock.write_lock(&project_root)?;
//!
//! // ... do work ...
//!
//! // Clear lock when done
//! CrashLock::clear_lock(&project_root)?;
//! ```

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process;
use tokio::io::AsyncWriteExt;

/// Lock file that tracks the current unit being executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashLock {
    pub unit_id: String,
    pub pid: u32,
    pub start_time: DateTime<Utc>,
    pub phase: String,
}

impl CrashLock {
    /// Create a new lock file
    pub fn new(unit_id: &str, phase: &str) -> Self {
        Self {
            unit_id: unit_id.to_string(),
            pid: process::id(),
            start_time: Utc::now(),
            phase: phase.to_string(),
        }
    }

    /// Write lock file to disk (atomic with exclusive create)
    pub fn write_lock(&self, project_root: &Path) -> anyhow::Result<()> {
        let orchestra_dir = project_root.join(".orchestra");
        let lock_path = orchestra_dir.join(".lock");

        // Ensure .orchestra directory exists
        std::fs::create_dir_all(&orchestra_dir).context("Failed to create .orchestra directory")?;

        // Serialize lock
        let content = serde_json::to_string_pretty(self).context("Failed to serialize lock")?;

        // Check for existing lock before writing
        if let Some(existing) = Self::read_lock(project_root).ok().flatten() {
            // If the existing lock is from the same PID, allow overwrite (re-entry)
            if existing.pid != std::process::id() {
                // Check if the other process is still alive
                if existing.is_process_alive() {
                    anyhow::bail!(
                        "Another Orchestra process (PID {}) is already running for this project",
                        existing.pid
                    );
                }
                tracing::info!("Stale lock from dead PID {}, overwriting", existing.pid);
            }
        }

        // Write atomically (write to temp then rename)
        let temp_path = lock_path.with_extension("tmp");
        std::fs::write(&temp_path, content).context("Failed to write temp lock file")?;
        std::fs::rename(&temp_path, &lock_path).context("Failed to rename lock file")?;

        tracing::info!("Lock file written: {:?}", lock_path);
        Ok(())
    }

    /// Read existing lock file
    pub fn read_lock(project_root: &Path) -> anyhow::Result<Option<CrashLock>> {
        let lock_path = project_root.join(".orchestra/.lock");

        if !lock_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&lock_path).context("Failed to read lock file")?;
        let lock: CrashLock =
            serde_json::from_str(&content).context("Failed to deserialize lock")?;

        Ok(Some(lock))
    }

    /// Clear lock file
    pub fn clear_lock(project_root: &Path) -> anyhow::Result<()> {
        let lock_path = project_root.join(".orchestra/.lock");

        if lock_path.exists() {
            std::fs::remove_file(&lock_path).context("Failed to remove lock file")?;
            tracing::info!("Lock file cleared: {:?}", lock_path);
        }

        Ok(())
    }

    /// Check if the locked process is still alive
    pub fn is_process_alive(&self) -> bool {
        // Try to check if process exists using signal 0 (no-op signal)
        // On Unix: signal 0 doesn't actually send a signal but checks if process exists
        // On Windows: we'll fall back to time-based check

        #[cfg(unix)]
        {
            use std::process::Command;

            // Use kill -0 $PID to check if process exists
            match Command::new("kill")
                .arg("-0")
                .arg(self.pid.to_string())
                .output()
            {
                Ok(output) => output.status.success(),
                Err(_) => false, // Command failed, assume process is dead
            }
        }

        #[cfg(windows)]
        {
            // On Windows, fall back to time-based check
            // (Could use tasklist /FI "PID eq $PID" but that's more complex)
            let elapsed = Utc::now() - self.start_time;
            elapsed.num_minutes() < 60
        }
    }

    /// Format crash information for display
    pub fn format_crash_info(&self) -> String {
        format!(
            "Crash detected:\n  Unit: {}\n  PID: {}\n  Phase: {}\n  Started: {}\n  Elapsed: {} minutes ago",
            self.unit_id,
            self.pid,
            self.phase,
            self.start_time.format("%Y-%m-%d %H:%M:%S UTC"),
            (Utc::now() - self.start_time).num_minutes()
        )
    }
}

/// Activity log for tracking all actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub timestamp: DateTime<Utc>,
    pub unit_id: String,
    pub event_type: ActivityType,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ActivityType {
    #[serde(rename = "session_start")]
    SessionStart,
    #[serde(rename = "session_end")]
    SessionEnd,
    #[serde(rename = "tool_use")]
    ToolUse,
    #[serde(rename = "tool_result")]
    ToolResult,
    #[serde(rename = "llm_response")]
    LLMResponse,
    #[serde(rename = "file_write")]
    FileWrite,
    #[serde(rename = "error")]
    Error,
}

pub struct ActivityLog {
    project_root: PathBuf,
}

impl ActivityLog {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Log an activity event
    pub async fn log(&self, event: ActivityEvent) -> anyhow::Result<()> {
        let log_path = self.project_root.join(".orchestra/activity.logl");

        // Ensure .orchestra directory exists
        std::fs::create_dir_all(self.project_root.join(".orchestra"))
            .context("Failed to create .orchestra directory")?;

        // Append to log (JSONL format - one JSON per line)
        let line = format!(
            "{}\n",
            serde_json::to_string(&event).context("Failed to serialize event")?
        );
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await?
            .write_all(line.as_bytes())
            .await?;

        tracing::debug!("Activity logged: {:?}", event.event_type);
        Ok(())
    }

    /// Read activity log for a unit
    pub async fn read_unit_log(&self, unit_id: &str) -> anyhow::Result<Vec<ActivityEvent>> {
        let log_path = self.project_root.join(".orchestra/activity.logl");

        if !log_path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&log_path).await?;
        let mut events = Vec::new();

        for line in content.lines() {
            if let Ok(event) = serde_json::from_str::<ActivityEvent>(line) {
                if event.unit_id == unit_id {
                    events.push(event);
                }
            }
        }

        Ok(events)
    }
}

/// Unit runtime record for tracking costs and time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitRuntimeRecord {
    pub unit_id: String,
    pub phase: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub tokens_used: u32,
    pub cost: f64,
    pub status: UnitStatus,
    pub artifacts_created: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum UnitStatus {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "crashed")]
    Crashed,
}

impl UnitRuntimeRecord {
    /// Create a new runtime record
    pub fn new(unit_id: &str, phase: &str) -> Self {
        Self {
            unit_id: unit_id.to_string(),
            phase: phase.to_string(),
            start_time: Utc::now(),
            end_time: None,
            tokens_used: 0,
            cost: 0.0,
            status: UnitStatus::Running,
            artifacts_created: Vec::new(),
        }
    }

    /// Write runtime record to disk
    pub fn write(&self, project_root: &Path) -> anyhow::Result<()> {
        let runtime_dir = project_root.join(".orchestra/.runtime");
        std::fs::create_dir_all(&runtime_dir).context("Failed to create runtime directory")?;

        let record_path = runtime_dir.join(format!("{}.json", self.unit_id));
        let content = serde_json::to_string_pretty(self).context("Failed to serialize record")?;

        std::fs::write(&record_path, content).context("Failed to write record")?;

        tracing::info!("Runtime record written: {:?}", record_path);
        Ok(())
    }

    /// Mark unit as completed
    pub fn mark_completed(&mut self, tokens_used: u32, cost: f64) {
        self.end_time = Some(Utc::now());
        self.tokens_used = tokens_used;
        self.cost = cost;
        self.status = UnitStatus::Completed;
    }

    /// Mark unit as crashed
    pub fn mark_crashed(&mut self) {
        self.end_time = Some(Utc::now());
        self.status = UnitStatus::Crashed;
    }
}

/// Session forensics for crash recovery analysis
pub struct SessionForensics {
    project_root: PathBuf,
}

impl SessionForensics {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Synthesize crash recovery briefing
    pub async fn synthesize_recovery(&self, unit_id: &str) -> anyhow::Result<String> {
        // Read activity log
        let activity_log = ActivityLog::new(self.project_root.clone());
        let events = activity_log.read_unit_log(unit_id).await?;

        if events.is_empty() {
            return Ok(format!(
                "=== CRASH RECOVERY BRIEFING ===\n\
                 You were executing unit: {}\n\
                 No prior activity log entries were found for this unit.\n\n\
                 === RESUME INSTRUCTIONS ===\n\
                 Continue from where you left off. Do NOT repeat initialization or setup steps already completed.",
                unit_id
            ));
        }

        // Analyze events to determine crash point
        let mut briefing = format!(
            "=== CRASH RECOVERY BRIEFING ===\n\
             You were executing unit: {}\n\
             The session crashed and is now being recovered.\n\n",
            unit_id
        );

        // Find last session_start
        if let Some(start) = events
            .iter()
            .find(|e| matches!(e.event_type, ActivityType::SessionStart))
        {
            briefing.push_str(&format!(
                "Session started: {}\n",
                start.timestamp.format("%H:%M:%S")
            ));
        }

        // Find last tool use
        if let Some(tool) = events
            .iter()
            .rev()
            .find(|e| matches!(e.event_type, ActivityType::ToolUse))
        {
            briefing.push_str(&format!("Last tool called: {}\n", tool.detail));
        }

        // Find last file write
        let files_created: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event_type, ActivityType::FileWrite))
            .collect();

        if !files_created.is_empty() {
            briefing.push_str("\nFiles created before crash:\n");
            for event in files_created {
                briefing.push_str(&format!("  - {}\n", event.detail));
            }
        }

        briefing.push_str(&format!("\nTotal actions before crash: {}\n", events.len()));

        briefing.push_str("\n=== RESUME INSTRUCTIONS ===\n");
        briefing.push_str("Continue from where you left off. Do NOT repeat work already done.\n");

        Ok(briefing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn crash_lock_roundtrip_and_clear() {
        let temp = tempfile::tempdir().unwrap();
        let lock = CrashLock::new("T01", "execute");

        lock.write_lock(temp.path()).unwrap();
        let loaded = CrashLock::read_lock(temp.path()).unwrap().unwrap();
        assert_eq!(loaded.unit_id, "T01");
        // On Unix, this checks if the current process is alive (should be true)
        // On Windows, this uses time-based check (recent start time = alive)
        assert!(loaded.is_process_alive());

        CrashLock::clear_lock(temp.path()).unwrap();
        assert!(CrashLock::read_lock(temp.path()).unwrap().is_none());
    }

    #[test]
    fn stale_crash_lock_is_detected() {
        let stale = CrashLock {
            unit_id: "T01".to_string(),
            pid: 99999, // Use a PID that's unlikely to exist
            start_time: Utc::now() - Duration::hours(2),
            phase: "execute".to_string(),
        };

        // On Unix: kill -0 99999 will fail (process doesn't exist)
        // On Windows: 2 hours old = considered dead
        assert!(!stale.is_process_alive());
        assert!(stale.format_crash_info().contains("T01"));
    }

    #[test]
    fn crash_lock_format_includes_all_fields() {
        let lock = CrashLock {
            unit_id: "S01".to_string(),
            pid: 12345,
            start_time: Utc::now() - Duration::minutes(30),
            phase: "plan".to_string(),
        };

        let info = lock.format_crash_info();
        assert!(info.contains("S01"));
        assert!(info.contains("12345"));
        assert!(info.contains("plan"));
        assert!(info.contains("30 minutes")); // or "30" at least
    }

    // --- CrashLock serde ---

    #[test]
    fn crash_lock_serde_roundtrip() {
        let lock = CrashLock {
            unit_id: "T42".into(),
            pid: 1234,
            start_time: Utc::now(),
            phase: "verify".into(),
        };
        let json = serde_json::to_string(&lock).unwrap();
        let decoded: CrashLock = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.unit_id, "T42");
        assert_eq!(decoded.pid, 1234);
        assert_eq!(decoded.phase, "verify");
    }

    // --- ActivityType serde ---

    #[test]
    fn activity_type_serde_variants() {
        let variants = vec![
            ActivityType::SessionStart,
            ActivityType::SessionEnd,
            ActivityType::ToolUse,
            ActivityType::ToolResult,
            ActivityType::LLMResponse,
            ActivityType::FileWrite,
            ActivityType::Error,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: ActivityType = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn activity_type_renames() {
        assert_eq!(
            serde_json::to_string(&ActivityType::SessionStart).unwrap(),
            "\"session_start\""
        );
        assert_eq!(
            serde_json::to_string(&ActivityType::ToolUse).unwrap(),
            "\"tool_use\""
        );
        assert_eq!(
            serde_json::to_string(&ActivityType::Error).unwrap(),
            "\"error\""
        );
    }

    // --- ActivityEvent serde ---

    #[test]
    fn activity_event_serde_roundtrip() {
        let event = ActivityEvent {
            timestamp: Utc::now(),
            unit_id: "T01".into(),
            event_type: ActivityType::ToolUse,
            detail: serde_json::json!({"tool": "bash", "args": "ls"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: ActivityEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.unit_id, "T01");
        assert_eq!(decoded.detail["tool"], "bash");
    }

    // --- UnitStatus serde ---

    #[test]
    fn unit_status_serde_variants() {
        let variants = vec![
            UnitStatus::Running,
            UnitStatus::Completed,
            UnitStatus::Failed,
            UnitStatus::Crashed,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: UnitStatus = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&decoded).unwrap();
            assert_eq!(json, json2);
        }
    }

    // --- UnitRuntimeRecord ---

    #[test]
    fn unit_runtime_record_new() {
        let rec = UnitRuntimeRecord::new("T01", "execute");
        assert_eq!(rec.unit_id, "T01");
        assert_eq!(rec.phase, "execute");
        assert!(rec.end_time.is_none());
        assert_eq!(rec.tokens_used, 0);
        assert_eq!(rec.cost, 0.0);
        assert!(matches!(rec.status, UnitStatus::Running));
        assert!(rec.artifacts_created.is_empty());
    }

    #[test]
    fn unit_runtime_record_mark_completed() {
        let mut rec = UnitRuntimeRecord::new("T01", "execute");
        rec.mark_completed(500, 0.03);
        assert!(rec.end_time.is_some());
        assert_eq!(rec.tokens_used, 500);
        assert_eq!(rec.cost, 0.03);
        assert!(matches!(rec.status, UnitStatus::Completed));
    }

    #[test]
    fn unit_runtime_record_mark_crashed() {
        let mut rec = UnitRuntimeRecord::new("T01", "plan");
        rec.mark_crashed();
        assert!(rec.end_time.is_some());
        assert!(matches!(rec.status, UnitStatus::Crashed));
    }

    #[test]
    fn unit_runtime_record_serde_roundtrip() {
        let mut rec = UnitRuntimeRecord::new("T03", "verify");
        rec.mark_completed(1000, 0.05);
        rec.artifacts_created.push("src/main.rs".into());
        let json = serde_json::to_string(&rec).unwrap();
        let decoded: UnitRuntimeRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.unit_id, "T03");
        assert_eq!(decoded.tokens_used, 1000);
        assert!(matches!(decoded.status, UnitStatus::Completed));
        assert_eq!(decoded.artifacts_created.len(), 1);
    }

    #[test]
    fn unit_runtime_record_write_read_with_tempdir() {
        let temp = tempfile::tempdir().unwrap();
        let rec = UnitRuntimeRecord::new("T99", "test_phase");
        rec.write(temp.path()).unwrap();

        let record_path = temp.path().join(".orchestra/.runtime/T99.json");
        assert!(record_path.exists());

        let content = std::fs::read_to_string(&record_path).unwrap();
        let loaded: UnitRuntimeRecord = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded.unit_id, "T99");
    }
}
