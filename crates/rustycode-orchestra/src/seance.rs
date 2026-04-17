// rustycode-orchestra/src/seance.rs
//! Session history querying inspired by Gastown's "Seance" concept.
//!
//! Gastown's Seance lets you query past session transcripts — "what did I do
//! yesterday?", "which files did task T03 touch?", "show me failed sessions".
//!
//! We adapt this for RustyCode: read the JSONL activity logs from `.orchestra/activity/`
//! and provide structured queries over past session history.

use crate::error::{OrchestraV2Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A single parsed session from an activity log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    /// Source file path
    pub source_file: String,
    /// Parsed unit type (e.g., "execute-task", "plan-slice")
    pub unit_type: String,
    /// Parsed unit ID (e.g., "M01-S01-T03")
    pub unit_id: String,
    /// Sequence number from filename
    pub sequence: u64,
    /// Number of entries in the session
    pub entry_count: usize,
    /// Tool calls found in the session
    pub tool_calls: Vec<ToolCallRecord>,
    /// Files that were read
    pub files_read: Vec<String>,
    /// Files that were written/edited
    pub files_written: Vec<String>,
    /// Errors encountered
    pub errors: Vec<String>,
    /// Session outcome
    pub outcome: SessionOutcome,
    /// Earliest timestamp found (if any)
    pub started_at: Option<DateTime<Utc>>,
    /// Latest timestamp found (if any)
    pub ended_at: Option<DateTime<Utc>>,
}

/// Outcome of a past session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionOutcome {
    /// Session completed successfully
    Success,
    /// Session ended with errors
    Failed,
    /// Session was interrupted (crash, signal, etc.)
    Interrupted,
    /// Outcome could not be determined
    Unknown,
}

/// A tool call extracted from session history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub detail: serde_json::Value,
}

/// Query parameters for searching session history
#[derive(Debug, Clone)]
pub struct SeanceQuery {
    /// Filter by unit type prefix (e.g., "execute")
    pub unit_type_prefix: Option<String>,
    /// Filter by unit ID (exact or contains)
    pub unit_id_contains: Option<String>,
    /// Filter by outcome
    pub outcome: Option<SessionOutcome>,
    /// Only sessions that touched this file path
    pub file_touched: Option<String>,
    /// Only sessions with errors
    pub has_errors: bool,
    /// Maximum results to return
    pub limit: Option<usize>,
    /// Sort newest first (default: true)
    pub newest_first: bool,
}

impl Default for SeanceQuery {
    fn default() -> Self {
        Self {
            unit_type_prefix: None,
            unit_id_contains: None,
            outcome: None,
            file_touched: None,
            has_errors: false,
            limit: None,
            newest_first: true,
        }
    }
}

/// Manages session history queries
pub struct Seance {
    activity_dir: PathBuf,
}

impl Seance {
    /// Create a new Seance query engine for a project
    pub fn new(project_root: &Path) -> Result<Self> {
        let activity_dir = project_root.join(".orchestra").join("activity");
        Ok(Self { activity_dir })
    }

    /// List all available session records
    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        let mut records = Vec::new();
        if !self.activity_dir.exists() {
            return Ok(records);
        }

        for entry in fs::read_dir(&self.activity_dir).map_err(OrchestraV2Error::Io)? {
            let entry = entry.map_err(OrchestraV2Error::Io)?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                if let Some(record) = self.parse_activity_file(&path) {
                    records.push(record);
                }
            }
        }

        records.sort_by(|a, b| b.sequence.cmp(&a.sequence));
        Ok(records)
    }

    /// Query session history with filters
    pub fn query(&self, q: &SeanceQuery) -> Result<Vec<SessionRecord>> {
        let mut records = self.list_sessions()?;

        if let Some(ref prefix) = q.unit_type_prefix {
            records.retain(|r| r.unit_type.starts_with(prefix));
        }

        if let Some(ref id_filter) = q.unit_id_contains {
            records.retain(|r| r.unit_id.contains(id_filter.as_str()));
        }

        if let Some(outcome) = q.outcome {
            records.retain(|r| r.outcome == outcome);
        }

        if let Some(ref file) = q.file_touched {
            records.retain(|r| {
                r.files_read.iter().any(|f| f.contains(file))
                    || r.files_written.iter().any(|f| f.contains(file))
            });
        }

        if q.has_errors {
            records.retain(|r| !r.errors.is_empty());
        }

        if !q.newest_first {
            records.reverse();
        }

        if let Some(limit) = q.limit {
            records.truncate(limit);
        }

        Ok(records)
    }

    /// Find sessions that touched a specific file
    pub fn find_by_file(&self, file_path: &str) -> Result<Vec<SessionRecord>> {
        self.query(&SeanceQuery {
            file_touched: Some(file_path.to_string()),
            ..Default::default()
        })
    }

    /// Find failed sessions
    pub fn find_failed(&self) -> Result<Vec<SessionRecord>> {
        self.query(&SeanceQuery {
            outcome: Some(SessionOutcome::Failed),
            ..Default::default()
        })
    }

    /// Get a summary of all session activity
    pub fn summary(&self) -> Result<SeanceSummary> {
        let records = self.list_sessions()?;
        let total = records.len();
        let successful = records
            .iter()
            .filter(|r| r.outcome == SessionOutcome::Success)
            .count();
        let failed = records
            .iter()
            .filter(|r| r.outcome == SessionOutcome::Failed)
            .count();
        let with_errors = records.iter().filter(|r| !r.errors.is_empty()).count();

        let all_files_written: Vec<String> = records
            .iter()
            .flat_map(|r| r.files_written.iter().cloned())
            .collect();

        let mut unique_files: Vec<String> = all_files_written
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        unique_files.sort();

        Ok(SeanceSummary {
            total_sessions: total,
            successful,
            failed,
            with_errors,
            unique_files_touched: unique_files.len(),
            files_written: unique_files,
        })
    }

    /// Parse a single activity log file into a SessionRecord
    fn parse_activity_file(&self, path: &Path) -> Option<SessionRecord> {
        let filename = path.file_name()?.to_str()?;
        let (sequence, unit_type, unit_id) = parse_activity_filename(filename)?;

        let content = fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

        let mut tool_calls = Vec::new();
        let mut files_read = Vec::new();
        let mut files_written = Vec::new();
        let mut errors = Vec::new();
        let mut has_error_entry = false;
        let mut has_success_marker = false;
        let mut started_at: Option<DateTime<Utc>> = None;
        let mut ended_at: Option<DateTime<Utc>> = None;

        for line in &lines {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                let entry_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match entry_type {
                    "tool_use" => {
                        if let Some(tool) = val.get("tool").and_then(|v| v.as_str()) {
                            let detail = val
                                .get("detail")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);

                            extract_files_from_tool(
                                tool,
                                &detail,
                                &mut files_read,
                                &mut files_written,
                            );

                            tool_calls.push(ToolCallRecord {
                                tool_name: tool.to_string(),
                                detail,
                            });
                        }
                    }
                    "error" => {
                        has_error_entry = true;
                        if let Some(msg) = val.get("message").and_then(|v| v.as_str()) {
                            errors.push(msg.to_string());
                        }
                    }
                    "session_start" | "unit_start" => {
                        started_at = extract_timestamp(&val);
                    }
                    "unit_complete" | "session_end" => {
                        ended_at = extract_timestamp(&val);
                        if val
                            .get("success")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                        {
                            has_success_marker = true;
                        }
                    }
                    _ => {}
                }

                let ts = extract_timestamp(&val);
                if ts.is_some() && started_at.is_none() {
                    started_at = ts;
                }
                if ts.is_some() {
                    ended_at = ts;
                }
            }
        }

        let outcome = if has_error_entry || !errors.is_empty() {
            if has_success_marker {
                SessionOutcome::Success
            } else {
                SessionOutcome::Failed
            }
        } else if has_success_marker {
            SessionOutcome::Success
        } else {
            SessionOutcome::Unknown
        };

        dedup(&mut files_read);
        dedup(&mut files_written);

        Some(SessionRecord {
            source_file: path.to_string_lossy().to_string(),
            unit_type,
            unit_id,
            sequence,
            entry_count: lines.len(),
            tool_calls,
            files_read,
            files_written,
            errors,
            outcome,
            started_at,
            ended_at,
        })
    }
}

/// Summary of all session history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeanceSummary {
    pub total_sessions: usize,
    pub successful: usize,
    pub failed: usize,
    pub with_errors: usize,
    pub unique_files_touched: usize,
    pub files_written: Vec<String>,
}

impl std::fmt::Display for SeanceSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "📊 {} sessions: ✅ {} succeeded, ❌ {} failed ({} with errors) | {} files touched",
            self.total_sessions,
            self.successful,
            self.failed,
            self.with_errors,
            self.unique_files_touched,
        )
    }
}

impl std::fmt::Display for SessionRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let icon = match self.outcome {
            SessionOutcome::Success => "✅",
            SessionOutcome::Failed => "❌",
            SessionOutcome::Interrupted => "⚠️",
            SessionOutcome::Unknown => "❓",
        };
        write!(
            f,
            "{} {:03} {} {} — {} entries, {} tools, {} errors",
            icon,
            self.sequence,
            self.unit_type,
            self.unit_id,
            self.entry_count,
            self.tool_calls.len(),
            self.errors.len(),
        )
    }
}

/// Parse activity filename: "{seq:03}-{unit_type}-{unit_id}.jsonl"
fn parse_activity_filename(filename: &str) -> Option<(u64, String, String)> {
    let name = filename.strip_suffix(".jsonl")?;
    let mut parts = name.splitn(3, '-');
    let seq: u64 = parts.next()?.parse().ok()?;
    let unit_type = parts.next()?.to_string();
    let unit_id = parts.next()?.to_string();
    Some((seq, unit_type, unit_id))
}

/// Extract timestamp from a JSON value
fn extract_timestamp(val: &serde_json::Value) -> Option<DateTime<Utc>> {
    val.get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Extract file paths from tool call details
fn extract_files_from_tool(
    tool: &str,
    detail: &serde_json::Value,
    files_read: &mut Vec<String>,
    files_written: &mut Vec<String>,
) {
    match tool {
        "read_file" | "cat" | "head" | "tail" => {
            if let Some(path) = detail.get("path").and_then(|v| v.as_str()) {
                files_read.push(path.to_string());
            }
        }
        "write_file" | "edit_file" | "atomic_write" => {
            if let Some(path) = detail.get("path").and_then(|v| v.as_str()) {
                files_written.push(path.to_string());
            }
        }
        "bash" | "shell" => {
            if let Some(cmd) = detail.get("command").and_then(|v| v.as_str()) {
                if cmd.contains(" > ") || cmd.contains(" >> ") || cmd.contains(" tee ") {
                    files_written.push(format!("(shell: {})", &cmd[..cmd.len().min(80)]));
                }
            }
        }
        _ => {}
    }
}

/// Deduplicate a vec while preserving order
fn dedup<T: Eq + std::hash::Hash + Clone>(vec: &mut Vec<T>) {
    let mut seen = std::collections::HashSet::new();
    vec.retain(|item| seen.insert(item.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_activity_filename() {
        let (seq, utype, uid) =
            parse_activity_filename("001-execute-task-M01-S01-T03.jsonl").unwrap();
        assert_eq!(seq, 1);
        assert_eq!(utype, "execute");
        assert_eq!(uid, "task-M01-S01-T03");
    }

    #[test]
    fn test_parse_activity_filename_multi_segment() {
        let (seq, utype, uid) = parse_activity_filename("042-plan-slice-M02-S01.jsonl").unwrap();
        assert_eq!(seq, 42);
        assert_eq!(utype, "plan");
        assert_eq!(uid, "slice-M02-S01");
    }

    #[test]
    fn test_parse_activity_filename_invalid() {
        assert!(parse_activity_filename("notavalidfile.txt").is_none());
        assert!(parse_activity_filename(".jsonl").is_none());
    }

    #[test]
    fn test_seance_list_empty() {
        let temp = tempfile::tempdir().unwrap();
        let seance = Seance::new(temp.path()).unwrap();
        let records = seance.list_sessions().unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn test_seance_parse_activity_file() {
        let temp = tempfile::tempdir().unwrap();
        let activity_dir = temp.path().join(".orchestra").join("activity");
        fs::create_dir_all(&activity_dir).unwrap();

        let log_path = activity_dir.join("001-execute-task-M01-S01-T01.jsonl");
        let mut file = fs::File::create(&log_path).unwrap();
        writeln!(file, r#"{{"type":"session_start","timestamp":"2024-03-19T12:00:00Z","unit_id":"M01-S01-T01"}}"#).unwrap();
        writeln!(
            file,
            r#"{{"type":"tool_use","tool":"read_file","detail":{{"path":"src/main.rs"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"tool_use","tool":"write_file","detail":{{"path":"src/lib.rs"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"tool_use","tool":"bash","detail":{{"command":"cargo test"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"unit_complete","timestamp":"2024-03-19T12:05:00Z","success":true}}"#
        )
        .unwrap();

        let seance = Seance::new(temp.path()).unwrap();
        let records = seance.list_sessions().unwrap();

        assert_eq!(records.len(), 1);
        let rec = &records[0];
        assert_eq!(rec.sequence, 1);
        assert_eq!(rec.unit_type, "execute");
        assert_eq!(rec.entry_count, 5);
        assert_eq!(rec.tool_calls.len(), 3);
        assert_eq!(rec.outcome, SessionOutcome::Success);
        assert!(rec.files_read.contains(&"src/main.rs".to_string()));
        assert!(rec.files_written.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn test_seance_query_filters() {
        let temp = tempfile::tempdir().unwrap();
        let activity_dir = temp.path().join(".orchestra").join("activity");
        fs::create_dir_all(&activity_dir).unwrap();

        let mut file1 =
            fs::File::create(activity_dir.join("001-execute-task-M01-S01-T01.jsonl")).unwrap();
        writeln!(
            file1,
            r#"{{"type":"session_start","timestamp":"2024-03-19T12:00:00Z"}}"#
        )
        .unwrap();
        writeln!(
            file1,
            r#"{{"type":"unit_complete","timestamp":"2024-03-19T12:05:00Z","success":true}}"#
        )
        .unwrap();

        let mut file2 =
            fs::File::create(activity_dir.join("002-plan-slice-M01-S02.jsonl")).unwrap();
        writeln!(
            file2,
            r#"{{"type":"session_start","timestamp":"2024-03-19T13:00:00Z"}}"#
        )
        .unwrap();
        writeln!(file2, r#"{{"type":"error","message":"LLM timeout"}}"#).unwrap();
        writeln!(
            file2,
            r#"{{"type":"unit_complete","timestamp":"2024-03-19T13:02:00Z","success":false}}"#
        )
        .unwrap();

        let seance = Seance::new(temp.path()).unwrap();

        let all = seance.list_sessions().unwrap();
        assert_eq!(all.len(), 2);

        let successful = seance
            .query(&SeanceQuery {
                outcome: Some(SessionOutcome::Success),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(successful.len(), 1);
        assert_eq!(successful[0].sequence, 1);

        let failed = seance.find_failed().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].sequence, 2);
        assert!(failed[0].errors.contains(&"LLM timeout".to_string()));

        let by_type = seance
            .query(&SeanceQuery {
                unit_type_prefix: Some("plan".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_type.len(), 1);
        assert_eq!(by_type[0].unit_type, "plan");
    }

    #[test]
    fn test_seance_query_by_file() {
        let temp = tempfile::tempdir().unwrap();
        let activity_dir = temp.path().join(".orchestra").join("activity");
        fs::create_dir_all(&activity_dir).unwrap();

        let mut file = fs::File::create(activity_dir.join("001-execute-task-T01.jsonl")).unwrap();
        writeln!(
            file,
            r#"{{"type":"tool_use","tool":"write_file","detail":{{"path":"src/auth.rs"}}}}"#
        )
        .unwrap();

        let seance = Seance::new(temp.path()).unwrap();
        let found = seance.find_by_file("auth.rs").unwrap();
        assert_eq!(found.len(), 1);

        let not_found = seance.find_by_file("nonexistent.rs").unwrap();
        assert!(not_found.is_empty());
    }

    #[test]
    fn test_seance_summary() {
        let temp = tempfile::tempdir().unwrap();
        let activity_dir = temp.path().join(".orchestra").join("activity");
        fs::create_dir_all(&activity_dir).unwrap();

        let mut f1 = fs::File::create(activity_dir.join("001-execute-task-T01.jsonl")).unwrap();
        writeln!(f1, r#"{{"type":"unit_complete","success":true}}"#).unwrap();

        let mut f2 = fs::File::create(activity_dir.join("002-execute-task-T02.jsonl")).unwrap();
        writeln!(f2, r#"{{"type":"error","message":"boom"}}"#).unwrap();

        let seance = Seance::new(temp.path()).unwrap();
        let summary = seance.summary().unwrap();
        assert_eq!(summary.total_sessions, 2);
        assert_eq!(summary.successful, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.with_errors, 1);
    }

    #[test]
    fn test_session_record_display() {
        let rec = SessionRecord {
            source_file: "/test/001-execute-task-T01.jsonl".into(),
            unit_type: "execute".into(),
            unit_id: "task-T01".into(),
            sequence: 1,
            entry_count: 10,
            tool_calls: vec![ToolCallRecord {
                tool_name: "bash".into(),
                detail: serde_json::json!({"command": "cargo test"}),
            }],
            files_read: vec![],
            files_written: vec![],
            errors: vec![],
            outcome: SessionOutcome::Success,
            started_at: None,
            ended_at: None,
        };
        let display = format!("{}", rec);
        assert!(display.contains("✅"));
        assert!(display.contains("execute"));
        assert!(display.contains("10 entries"));
    }

    #[test]
    fn test_query_limit() {
        let temp = tempfile::tempdir().unwrap();
        let activity_dir = temp.path().join(".orchestra").join("activity");
        fs::create_dir_all(&activity_dir).unwrap();

        for i in 1..=5u64 {
            let fname = format!("{:03}-execute-task-T{:02}.jsonl", i, i);
            let path = activity_dir.join(&fname);
            fs::write(&path, r#"{"type":"session_start"}"#).unwrap();
        }

        let seance = Seance::new(temp.path()).unwrap();

        let limited = seance
            .query(&SeanceQuery {
                limit: Some(3),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(limited.len(), 3);
        assert_eq!(limited[0].sequence, 5);
    }
}
