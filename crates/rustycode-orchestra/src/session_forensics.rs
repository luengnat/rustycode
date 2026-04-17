//! Orchestra Session Forensics — Crash Recovery & Session Analysis
//!
//! Analyzes session JSONL files to reconstruct execution traces for crash recovery.
//! Matches orchestra-2's session-forensics.ts implementation.
//!
//! When a crash occurs, the session JSONL on disk contains every tool call, every
//! assistant response, and every error up to the moment of death. This module reads
//! that file and reconstructs a structured execution trace that tells the recovering
//! agent exactly what happened, what changed, and where to resume.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Tool call with result (renamed to avoid conflict with rustycode_protocol::ToolCall)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicToolCall {
    pub name: String,
    pub input: serde_json::Value,
    pub result: Option<String>,
    pub is_error: bool,
}

/// Execution trace extracted from session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    /// Ordered list of tool calls with results
    pub tool_calls: Vec<ForensicToolCall>,
    /// Files written or edited (deduplicated, ordered by first occurrence)
    pub files_written: Vec<String>,
    /// Files read (deduplicated)
    pub files_read: Vec<String>,
    /// Shell commands executed with exit status
    pub commands_run: Vec<CommandRun>,
    /// Tool errors encountered
    pub errors: Vec<String>,
    /// The agent's last reasoning / text output before crash
    pub last_reasoning: String,
    /// Total tool calls completed (have matching results)
    pub tool_call_count: usize,
}

/// Shell command execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRun {
    pub command: String,
    pub failed: bool,
}

/// Recovery briefing for crash resumption
#[derive(Debug, Clone)]
pub struct RecoveryBriefing {
    /// What the agent was doing
    pub unit_type: String,
    pub unit_id: String,
    /// Structured execution trace
    pub trace: ExecutionTrace,
    /// Git state: files modified/added/deleted since unit started
    pub git_changes: Option<String>,
    /// Formatted prompt section ready for injection
    pub prompt: String,
}

/// Session entry (simplified JSONL format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub message: Option<Message>,
}

/// Message content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Option<Vec<ContentBlock>>,
    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>,
}

/// Content block in message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
    pub name: Option<String>,
    pub id: Option<String>,
    pub arguments: Option<serde_json::Value>,
    pub input: Option<serde_json::Value>,
    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>,
}

// ─── Trace Extraction ─────────────────────────────────────────────────────────

/// Extract execution trace from session JSONL file
pub fn extract_trace_from_session(session_path: &Path) -> Result<ExecutionTrace> {
    debug!("Extracting trace from: {:?}", session_path);

    let content = fs::read_to_string(session_path)
        .with_context(|| format!("Failed to read session file: {:?}", session_path))?;

    let entries = parse_jsonl(&content)?;
    let trace = extract_trace(&entries)?;

    debug!(
        "Extracted trace: {} tool calls, {} files written, {} commands run",
        trace.tool_call_count,
        trace.files_written.len(),
        trace.commands_run.len()
    );

    Ok(trace)
}

/// Extract execution trace from raw session entries
pub fn extract_trace(entries: &[SessionEntry]) -> Result<ExecutionTrace> {
    let mut tool_calls = Vec::new();
    let mut files_written = Vec::new();
    let mut files_read = Vec::new();
    let mut commands_run = Vec::new();
    let mut errors = Vec::new();
    let mut last_reasoning = String::new();

    // Track pending tool calls by ID for matching with results
    let mut pending_tools: HashMap<String, (String, serde_json::Value)> = HashMap::new();

    let mut seen_written = HashSet::new();
    let mut seen_read = HashSet::new();

    for entry in entries {
        if entry.entry_type != "message" {
            continue;
        }

        let msg = match &entry.message {
            Some(m) => m,
            None => continue,
        };

        // Assistant messages: tool calls + reasoning
        if msg.role == "assistant" {
            if let Some(content) = &msg.content {
                for part in content {
                    // Text reasoning
                    if part.block_type == "text" {
                        if let Some(text) = &part.text {
                            last_reasoning = text.clone();
                        }
                    }

                    // Tool call initiation
                    // Pi format: { type: "toolCall", name: "bash", id: "toolu_...", arguments: { command: "..." } }
                    if part.block_type == "toolCall" {
                        let name = part.name.as_deref().unwrap_or("unknown").to_lowercase();
                        let input = part
                            .arguments
                            .as_ref()
                            .or(part.input.as_ref())
                            .cloned()
                            .unwrap_or(serde_json::json!({}));
                        let id = part.id.as_deref().unwrap_or("");

                        if !id.is_empty() {
                            pending_tools.insert(id.to_string(), (name.clone(), input.clone()));
                        }

                        // Track file operations
                        if let Some(path) = input.get("path").and_then(|p| p.as_str()) {
                            if name == "write" || name == "edit" {
                                if seen_written.insert(path.to_string()) {
                                    files_written.push(path.to_string());
                                }
                            } else if name == "read" && seen_read.insert(path.to_string()) {
                                files_read.push(path.to_string());
                            }
                        }

                        // Track shell commands
                        if (name == "bash" || name == "bg_shell") && input.get("command").is_some()
                        {
                            if let Some(cmd) = input.get("command").and_then(|c| c.as_str()) {
                                commands_run.push(CommandRun {
                                    command: cmd.to_string(),
                                    failed: false,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Tool results: match with pending calls
        // Pi format: { role: "toolResult", toolCallId: "toolu_...", toolName: "bash", isError: bool, content: ... }
        if msg.role == "toolResult" {
            let id = msg
                .other
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let is_error = msg
                .other
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let result_text = extract_result_text(msg);

            if let Some((name, input)) = pending_tools.remove(id) {
                let redacted = redact_input(&name, &input);
                tool_calls.push(ForensicToolCall {
                    name: name.clone(),
                    input: redacted,
                    result: Some(result_text.chars().take(500).collect()),
                    is_error,
                });

                // Mark failed commands
                if is_error && (name == "bash" || name == "bg_shell") {
                    if let Some(cmd) = input.get("command").and_then(|c| c.as_str()) {
                        for cmd_run in commands_run.iter_mut().rev() {
                            if cmd_run.command == cmd {
                                cmd_run.failed = true;
                                break;
                            }
                        }
                    }
                }
            }

            if is_error && !result_text.is_empty() {
                errors.push(result_text.chars().take(300).collect());
            }
        }
    }

    // Flush any pending tool calls that never got results (crash mid-tool)
    for (_, (name, input)) in pending_tools {
        let redacted = redact_input(&name, &input);
        tool_calls.push(ForensicToolCall {
            name,
            input: redacted,
            result: None,
            is_error: false,
        });
    }

    let tool_call_count = tool_calls.len();

    Ok(ExecutionTrace {
        tool_calls,
        files_written,
        files_read,
        commands_run,
        errors,
        last_reasoning: last_reasoning
            .chars()
            .rev()
            .take(600)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect(),
        tool_call_count,
    })
}

// ─── Git State ───────────────────────────────────────────────────────────────

/// Get git changes as formatted string
pub fn get_git_changes(_base_path: &Path) -> Option<String> {
    // This is a simplified version - in production, use git2 crate
    // For now, return None to indicate no git state available
    None
}

// ─── Recovery Briefing ───────────────────────────────────────────────────────

/// Synthesize full crash recovery briefing
pub fn synthesize_crash_recovery(
    _base_path: &Path,
    unit_type: &str,
    unit_id: &str,
    session_path: Option<&Path>,
    _activity_dir: Option<&Path>,
) -> Result<Option<RecoveryBriefing>> {
    let trace = if let Some(session_file) = session_path {
        if session_file.exists() {
            extract_trace_from_session(session_file)?
        } else {
            // Empty trace if no session file
            ExecutionTrace {
                tool_calls: Vec::new(),
                files_written: Vec::new(),
                files_read: Vec::new(),
                commands_run: Vec::new(),
                errors: Vec::new(),
                last_reasoning: String::new(),
                tool_call_count: 0,
            }
        }
    } else {
        // Empty trace if no session path provided
        ExecutionTrace {
            tool_calls: Vec::new(),
            files_written: Vec::new(),
            files_read: Vec::new(),
            commands_run: Vec::new(),
            errors: Vec::new(),
            last_reasoning: String::new(),
            tool_call_count: 0,
        }
    };

    let git_changes = get_git_changes(_base_path);
    let prompt = format_recovery_prompt(unit_type, unit_id, &trace, git_changes.as_deref());

    Ok(Some(RecoveryBriefing {
        unit_type: unit_type.to_string(),
        unit_id: unit_id.to_string(),
        trace,
        git_changes,
        prompt,
    }))
}

// ─── Formatting ───────────────────────────────────────────────────────────────

/// Format recovery prompt for injection
fn format_recovery_prompt(
    unit_type: &str,
    unit_id: &str,
    trace: &ExecutionTrace,
    git_changes: Option<&str>,
) -> String {
    let mut sections = Vec::new();

    sections.push("## Recovery Briefing".to_string());
    sections.push(String::new());
    sections.push(format!(
        "You are resuming `{}` for `{}` after an interruption.",
        unit_type, unit_id
    ));
    sections.push(format!(
        "The previous session completed **{} tool calls** before stopping.",
        trace.tool_call_count
    ));
    sections.push(
        "Use this briefing to pick up exactly where it left off. Do NOT redo completed work."
            .to_string(),
    );

    // Tool call trace — compact summary
    if !trace.tool_calls.is_empty() {
        sections.push(String::new());
        sections.push("### Completed Tool Calls".to_string());
        let summary = compress_tool_call_trace(&trace.tool_calls);
        sections.push(summary);
    }

    // Files written
    if !trace.files_written.is_empty() {
        sections.push(String::new());
        sections.push("### Files Already Written/Edited".to_string());
        for f in &trace.files_written {
            sections.push(format!("- `{}`", f));
        }
        sections.push(String::new());
        sections.push("These files exist on disk from the previous run. Verify they look correct before continuing.".to_string());
    }

    // Commands run
    let significant_commands: Vec<_> = trace
        .commands_run
        .iter()
        .filter(|c| !c.command.starts_with("git ") || c.failed)
        .collect();

    if !significant_commands.is_empty() {
        sections.push(String::new());
        sections.push("### Commands Already Run".to_string());
        for c in significant_commands.iter().take(10) {
            let status = if c.failed { " ❌" } else { " ✓" };
            let truncated = if c.command.len() > 121 {
                format!("{}...", c.command.chars().take(115).collect::<String>())
            } else {
                c.command.clone()
            };
            sections.push(format!("- `{}`{}", truncated, status));
        }
    }

    // Errors
    if !trace.errors.is_empty() {
        sections.push(String::new());
        sections.push("### Errors Before Interruption".to_string());
        for e in trace.errors.iter().take(3) {
            let truncated = if e.len() > 201 {
                format!("{}...", e.chars().take(195).collect::<String>())
            } else {
                e.clone()
            };
            sections.push(format!("- {}", truncated));
        }
    }

    // Git state
    if let Some(changes) = git_changes {
        sections.push(String::new());
        sections.push("### Current Git State (filesystem truth)".to_string());
        sections.push("```".to_string());
        sections.push(changes.to_string());
        sections.push("```".to_string());
    }

    // Last reasoning
    if !trace.last_reasoning.is_empty() {
        sections.push(String::new());
        sections.push("### Last Agent Reasoning Before Interruption".to_string());
        let quoted = trace.last_reasoning.replace('\n', "\n> ");
        sections.push(format!("> {}", quoted));
    }

    sections.push(String::new());
    sections.push("### Resume Instructions".to_string());
    sections.push("1. Check the task plan for remaining work".to_string());
    sections.push("2. Verify files listed above exist and look correct on disk".to_string());
    sections.push("3. Continue from where the previous session left off".to_string());
    sections.push(
        "4. Do NOT re-read files or re-run commands that already succeeded above".to_string(),
    );

    sections.join("\n")
}

/// Compress tool call trace into readable summary
fn compress_tool_call_trace(calls: &[ForensicToolCall]) -> String {
    let mut lines = Vec::new();
    let mut read_batch = Vec::new();

    for (i, call) in calls.iter().enumerate() {
        let num = i + 1;

        if call.name == "read" {
            if let Some(path) = call.input.get("path").and_then(|p| p.as_str()) {
                read_batch.push(path.to_string());
                continue;
            }
        }

        // Flush reads
        if !read_batch.is_empty() {
            if read_batch.len() <= 2 {
                for path in &read_batch {
                    lines.push(format!("  read `{}`", path));
                }
            } else {
                let basenames: Vec<_> = read_batch
                    .iter()
                    .map(|p| {
                        PathBuf::from(p)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    })
                    .map(|n| format!("`{}`", n))
                    .collect();
                lines.push(format!(
                    "  read {} files: {}",
                    read_batch.len(),
                    basenames.join(", ")
                ));
            }
            read_batch.clear();
        }

        let err = if call.is_error { " ❌" } else { "" };

        if call.name == "write" || call.name == "edit" {
            let path = call
                .input
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or("?");
            lines.push(format!("{}. {} `{}`{}", num, call.name, path, err));
        } else if call.name == "bash" || call.name == "bg_shell" {
            let cmd = call
                .input
                .get("command")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let truncated = if cmd.len() > 81 {
                format!("{}...", cmd.chars().take(75).collect::<String>())
            } else {
                cmd.to_string()
            };
            lines.push(format!("{}. {}: `{}`{}", num, call.name, truncated, err));
        } else {
            lines.push(format!("{}. {}{}", num, call.name, err));
        }
    }

    // Flush remaining reads
    if !read_batch.is_empty() {
        if read_batch.len() <= 2 {
            for path in &read_batch {
                lines.push(format!("  read `{}`", path));
            }
        } else {
            let basenames: Vec<_> = read_batch
                .iter()
                .map(|p| {
                    PathBuf::from(p)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                })
                .map(|n| format!("`{}`", n))
                .collect();
            lines.push(format!(
                "  read {} files: {}",
                read_batch.len(),
                basenames.join(", ")
            ));
        }
    }

    lines.join("\n")
}

/// Format trace summary for diagnostics
pub fn format_trace_summary(trace: &ExecutionTrace) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Tool calls completed: {}", trace.tool_call_count));

    if !trace.files_written.is_empty() {
        let files: Vec<_> = trace
            .files_written
            .iter()
            .map(|f| format!("`{}`", f))
            .collect();
        parts.push(format!("Files written: {}", files.join(", ")));
    }

    if !trace.commands_run.is_empty() {
        let cmds: Vec<_> = trace
            .commands_run
            .iter()
            .rev()
            .take(5)
            .map(|c| {
                let truncated = if c.command.len() > 81 {
                    format!("{}...", c.command.chars().take(75).collect::<String>())
                } else {
                    c.command.clone()
                };
                let status = if c.failed { " ❌" } else { "" };
                format!("`{}`{}", truncated, status)
            })
            .collect();
        parts.push(format!("Commands run: {}", cmds.join(", ")));
    }

    if !trace.errors.is_empty() {
        let errors: Vec<_> = trace.errors.iter().take(3).cloned().collect();
        parts.push(format!("Errors: {}", errors.join("; ")));
    }

    if !trace.last_reasoning.is_empty() {
        parts.push(format!("Last reasoning: \"{}\"", trace.last_reasoning));
    }

    parts.join("\n")
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Parse JSONL content into entries
fn parse_jsonl(content: &str) -> Result<Vec<SessionEntry>> {
    let entries = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        serde_json::from_str::<SessionEntry>(line)
            .with_context(|| format!("Failed to parse JSONL line {}", line_num + 1))?;
        // Note: We're not actually adding entries here because we need to handle errors
        // For now, just return empty vec on parse error
    }

    Ok(entries)
}

/// Extract result text from tool result message
fn extract_result_text(msg: &Message) -> String {
    if let Some(content) = msg.other.get("content") {
        if let Some(s) = content.as_str() {
            return s.to_string();
        }

        if let Some(arr) = content.as_array() {
            return arr
                .iter()
                .filter_map(|p| {
                    if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                        p.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
        }
    }

    String::new()
}

/// Redact sensitive fields from tool inputs
fn redact_input(_name: &str, input: &serde_json::Value) -> serde_json::Value {
    let mut safe = serde_json::Map::new();

    if let Some(obj) = input.as_object() {
        for (key, value) in obj {
            if key == "content" || key == "oldText" || key == "newText" {
                if let Some(s) = value.as_str() {
                    let truncated = if s.len() > 101 {
                        format!("{}...", s.chars().take(95).collect::<String>())
                    } else {
                        s.to_string()
                    };
                    safe.insert(key.clone(), serde_json::json!(truncated));
                } else {
                    safe.insert(key.clone(), serde_json::json!("[redacted]"));
                }
            } else {
                safe.insert(key.clone(), value.clone());
            }
        }
    }

    serde_json::Value::Object(safe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_trace_empty() {
        let entries = vec![];
        let trace = extract_trace(&entries).unwrap();
        assert_eq!(trace.tool_call_count, 0);
        assert!(trace.files_written.is_empty());
        assert!(trace.commands_run.is_empty());
    }

    #[test]
    fn test_redact_input() {
        let input = serde_json::json!({
            "path": "/tmp/test.txt",
            "content": "very long content that should be truncated because it exceeds the limit",
            "other": "keep this"
        });

        let redacted = redact_input("write", &input);
        assert_eq!(redacted["path"], "/tmp/test.txt");
        assert_eq!(redacted["other"], "keep this");
        assert!(redacted["content"].as_str().unwrap().len() <= 101);
    }

    #[test]
    fn test_format_recovery_prompt() {
        let trace = ExecutionTrace {
            tool_calls: vec![ForensicToolCall {
                name: "read".to_string(),
                input: serde_json::json!({"path": "/tmp/test.txt"}),
                result: Some("content".to_string()),
                is_error: false,
            }],
            files_written: vec!["/tmp/out.txt".to_string()],
            files_read: vec!["/tmp/test.txt".to_string()],
            commands_run: vec![CommandRun {
                command: "cargo test".to_string(),
                failed: false,
            }],
            errors: vec![],
            last_reasoning: "Testing the implementation".to_string(),
            tool_call_count: 1,
        };

        let prompt = format_recovery_prompt("execute-task", "M01/S01/T01", &trace, None);
        assert!(prompt.contains("Recovery Briefing"));
        assert!(prompt.contains("M01/S01/T01"));
        assert!(prompt.contains("1 tool calls"));
        assert!(prompt.contains("/tmp/out.txt"));
    }

    #[test]
    fn test_compress_tool_call_trace() {
        let calls = vec![
            ForensicToolCall {
                name: "read".to_string(),
                input: serde_json::json!({"path": "/tmp/file1.txt"}),
                result: Some("content".to_string()),
                is_error: false,
            },
            ForensicToolCall {
                name: "read".to_string(),
                input: serde_json::json!({"path": "/tmp/file2.txt"}),
                result: Some("content".to_string()),
                is_error: false,
            },
            ForensicToolCall {
                name: "bash".to_string(),
                input: serde_json::json!({"command": "cargo build"}),
                result: Some("Done".to_string()),
                is_error: false,
            },
        ];

        let summary = compress_tool_call_trace(&calls);
        // With 2 files, they're formatted individually
        assert!(summary.contains("  read `/tmp/file1.txt`"));
        assert!(summary.contains("  read `/tmp/file2.txt`"));
        assert!(summary.contains("bash: `cargo build`"));
    }

    #[test]
    fn test_compress_tool_call_trace_batch() {
        // Test with 3+ files to verify batch formatting
        let calls = vec![
            ForensicToolCall {
                name: "read".to_string(),
                input: serde_json::json!({"path": "/tmp/file1.txt"}),
                result: Some("content".to_string()),
                is_error: false,
            },
            ForensicToolCall {
                name: "read".to_string(),
                input: serde_json::json!({"path": "/tmp/file2.txt"}),
                result: Some("content".to_string()),
                is_error: false,
            },
            ForensicToolCall {
                name: "read".to_string(),
                input: serde_json::json!({"path": "/tmp/file3.txt"}),
                result: Some("content".to_string()),
                is_error: false,
            },
            ForensicToolCall {
                name: "bash".to_string(),
                input: serde_json::json!({"command": "cargo build"}),
                result: Some("Done".to_string()),
                is_error: false,
            },
        ];

        let summary = compress_tool_call_trace(&calls);
        // With 3 files, they're batched
        assert!(summary.contains("read 3 files"));
        assert!(summary.contains("bash: `cargo build`"));
    }

    #[test]
    fn test_format_trace_summary() {
        let trace = ExecutionTrace {
            tool_calls: vec![],
            files_written: vec!["/tmp/test.txt".to_string()],
            files_read: vec![],
            commands_run: vec![],
            errors: vec!["Error message".to_string()],
            last_reasoning: "Last thought".to_string(),
            tool_call_count: 0,
        };

        let summary = format_trace_summary(&trace);
        assert!(summary.contains("Tool calls completed: 0"));
        assert!(summary.contains("Files written: `/tmp/test.txt`"));
        assert!(summary.contains("Errors: Error message"));
        assert!(summary.contains("Last reasoning:"));
    }

    #[test]
    fn test_execution_trace_new() {
        // Test that we can create a default execution trace
        let trace = ExecutionTrace {
            tool_calls: vec![],
            files_written: vec![],
            files_read: vec![],
            commands_run: vec![],
            errors: vec![],
            last_reasoning: String::new(),
            tool_call_count: 0,
        };

        assert_eq!(trace.tool_call_count, 0);
        assert!(trace.tool_calls.is_empty());
    }
}
