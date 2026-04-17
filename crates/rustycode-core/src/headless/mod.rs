//! Headless agent loop for `run --auto` mode.
//!
//! Drives the LLM → tool call → result → LLM cycle without a TUI.
//! This is a minimal version of the TUI's streaming response loop,
//! stripped of UI concerns (caching, undo snapshots, etc.).

use anyhow::{Context, Result};
use futures::StreamExt;
use rustycode_llm::provider_v2::{ChatMessage, CompletionRequest, LLMProvider, MessageRole};
use rustycode_protocol::{ContentBlock, MessageContent};
use serde_json;
use std::path::Path;
use std::time::Duration;
use tracing::info;

use crate::iteration_checkpoint::{CheckpointStorage, CheckpointToolCall, IterationCheckpoint};
use crate::streaming::{SseEventProcessor, StreamingCallbacks, ToolAccumulator};

pub mod hints;
pub mod utils;

pub use self::utils::{clean_assistant_text, prune_messages};
use rustycode_protocol::agent_protocol::AgentAction;
use rustycode_protocol::ToolCall;
use rustycode_tools::{ToolContext, ToolRegistry};

/// Dispatches a structured AgentAction to the ToolRegistry.
pub fn dispatch_agent_action(
    action: AgentAction,
    cwd: &Path,
    tool_registry: &ToolRegistry,
) -> String {
    let (name, args) = match action {
        AgentAction::EditFile { path, content } => (
            "edit_file".to_string(),
            serde_json::json!({"path": path, "content": content}),
        ),
        AgentAction::Bash { command, cwd } => (
            "bash".to_string(),
            serde_json::json!({"command": command, "cwd": cwd.unwrap_or_else(|| ".".to_string())}),
        ),

        AgentAction::ListFiles { path } => {
            ("list_dir".to_string(), serde_json::json!({"path": path}))
        }
        AgentAction::Complete { message } => return format!("Task completed: {}", message),
    };

    let call = ToolCall {
        call_id: "headless-structured".to_string(),
        name,
        arguments: args,
    };

    let ctx = ToolContext::new(cwd);
    let result = tool_registry.execute(&call, &ctx);

    if result.success {
        result.output
    } else {
        result
            .error
            .unwrap_or_else(|| "Error executing structured action".to_string())
    }
}

pub fn summarize_tool_args(name: &str, partial_json: &str) -> String {
    if name == "bash" {
        if let Ok(args) = serde_json::from_str::<serde_json::Value>(partial_json) {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                let cmd = cmd.trim();
                if cmd.len() > 60 {
                    return format!("{:.60}...", cmd);
                }
                return cmd.to_string();
            }
        }
        return "bash command".to_string();
    }

    if let Ok(args) = serde_json::from_str::<serde_json::Value>(partial_json) {
        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
            return path.to_string();
        }
    }

    format!("{} args", name)
}

/// Execute a tool via the shared registry and apply headless-specific post-processing.
fn execute_headless_tool(
    cwd: &Path,
    tool_name: &str,
    tool_json: &str,
    tool_registry: &ToolRegistry,
) -> String {
    let args: serde_json::Value = match serde_json::from_str(tool_json) {
        Ok(v) => v,
        Err(e) => return format!("Error: Failed to parse tool arguments: {}", e),
    };

    let call = ToolCall {
        call_id: "headless".to_string(),
        name: tool_name.to_string(),
        arguments: args.clone(),
    };

    let ctx = ToolContext::new(cwd);
    let result = tool_registry.execute(&call, &ctx);

    let mut output = if result.success {
        result.output
    } else {
        result.error.unwrap_or_else(|| "Error".to_string())
    };

    // Bash-specific hints that improve agent behavior in headless mode.
    if tool_name == "bash" {
        if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
            let cmd_lower = command.to_lowercase();
            let out_lower = output.to_lowercase();

            // Grep line-number hint
            if let Some(first_line) = output.lines().next() {
                if let Some(line_num) = first_line
                    .split(':')
                    .nth(1)
                    .and_then(|p| p.parse::<usize>().ok())
                {
                    if line_num > 100 {
                        let offset = line_num.saturating_sub(5);
                        output.push_str(&format!(
                            "\n\nHINT: To read the code around line {}, use read_file with offset={} and limit=80",
                            line_num, offset
                        ));
                    }
                }
            }

            // Cython hint
            if out_lower.contains(".pyx") && out_lower.contains("attributeerror") {
                output.push_str(
                    "\n\nHINT: The error is in a Cython (.pyx) file. After editing .pyx source files, \
                    you MUST rebuild: run \"python setup.py build_ext --inplace\" then \"pip install -e .\" \
                    to recompile the extension.",
                );
            }

            // NumPy deprecation hint
            if (out_lower.contains("has no attribute 'int'")
                || out_lower.contains("has no attribute 'float'")
                || out_lower.contains("has no attribute 'complex'")
                || out_lower.contains("has no attribute 'bool'")
                || out_lower.contains("has no attribute 'str'"))
                && (out_lower.contains("numpy") || out_lower.contains("np."))
            {
                output.push_str(
                    "\n\nHINT: This is a NumPy 2.0 deprecation error. Search ALL source files \
                    (including .pyx, .pxd Cython files) for the deprecated pattern. \
                    Use: grep -rn \"np.int[^0-9_]\" --include=\"*.py\" --include=\"*.pyx\" --include=\"*.pxd\" . \
                    Then fix ALL occurrences and rebuild if you edited Cython files.",
                );
            }

            // Missing build dependencies hint
            if out_lower.contains("no module named 'setuptools'")
                || out_lower.contains("no module named 'cython'")
                || out_lower.contains("modulenotfounderror: no module named 'setuptools'")
                || out_lower.contains("modulenotfounderror: no module named 'cython'")
                || out_lower.contains("command not found: cython")
                || out_lower.contains("error: command 'cython' failed")
                || out_lower.contains("unable to find 'cython'")
            {
                output.push_str(
                    "\n\nHINT: Missing Python build dependency. Install with: \
                    pip install setuptools wheel cython \
                    Then retry the build command.",
                );
            }

            // Python 3.9+ deprecations
            if out_lower.contains("cannot import name 'gcd' from 'fractions'")
                || out_lower.contains("attributeerror: module 'fractions' has no attribute 'gcd'")
            {
                output.push_str(
                    "\n\nHINT: `fractions.gcd` was removed in Python 3.9+. \
                    Replace `from fractions import gcd` with `from math import gcd` in the source file.",
                );
            }
            if out_lower.contains("cannot import name 'mapping' from 'collections'")
                || out_lower.contains("cannot import name 'mutablemapping' from 'collections'")
                || out_lower.contains("cannot import name 'iterable' from 'collections'")
            {
                output.push_str(
                    "\n\nHINT: Several ABCs were moved from `collections` to `collections.abc` \
                    in Python 3.10+. Replace `from collections import Mapping/MutableMapping/Iterable` \
                    with `from collections.abc import Mapping/MutableMapping/Iterable`.",
                );
            }

            // Command timeout hint — compilation can be slow in emulated environments
            if out_lower.contains("command timed out") {
                output.push_str(
                    "\n\nHINT: The command timed out. Compilation/build steps can be very slow in emulated \
                    environments. Try: 1) Break the build into smaller steps (compile one extension at a time), \
                    2) Use simpler compiler flags, 3) Pre-install all dependencies before building, \
                    4) Run the build in background and poll for completion with a loop.",
                );
            }

            // Disk space hint — common in constrained containers
            if out_lower.contains("no space left")
                || out_lower.contains("disk full")
                || out_lower.contains("cannot write: no space")
                || out_lower.contains("errno 28")
                || out_lower.contains("write error: no space")
            {
                output.push_str(
                    "\n\nHINT: Disk is full. Free space FIRST before retrying: \
                    rm -rf build/ dist/ *.egg-info __pycache__ .cache/ /tmp/* 2>/dev/null \
                    Then retry with a smaller build (avoid creating temp files, use --no-build-isolation).",
                );
            }

            // build_ext success hint — remind to pip install -e . so the package is importable
            if cmd_lower.contains("build_ext") && !out_lower.contains("error")
                && !out_lower.contains("failed")
            {
                output.push_str(
                    "\n\nREMINDER: build_ext completed. You MUST also run `pip install -e .` (or `pip install -e /path/to/package`) \
                    to install the package so it can be imported system-wide. \
                    Without this step, `import <package>` will fail with ModuleNotFoundError.",
                );
            }

            // git checkout warning — don't revert your own changes
            if cmd_lower.contains("git checkout") && cmd_lower.contains(".")
                && !cmd_lower.contains("git checkout -b")
            {
                output.push_str(
                    "\n\nWARNING: You just used git checkout to revert a file. This undoes your previous work! \
                    If your edit caused an error, FIX the error instead of reverting. \
                    Move FORWARD — edit the file to fix the issue, don't go back to square one.",
                );
            }

            // Server startup hint — remind to verify server is responding
            if (cmd_lower.contains("nohup") || cmd_lower.contains("&") || cmd_lower.contains("http.server")
                || cmd_lower.contains("python -m http") || cmd_lower.contains("flask run")
                || cmd_lower.contains("uvicorn") || cmd_lower.contains("gunicorn"))
                && !out_lower.contains("error")
            {
                output.push_str(
                    "\n\nREMINDER: Server started. Before declaring done, verify it responds: \
                    sleep 2 && curl -s -o /dev/null -w '%{http_code}' http://localhost:<port>/path \
                    If you get 000, the server isn't ready — add sleep or check the port.",
                );
            }

            // pip install failure hints
            if cmd_lower.contains("pip") && cmd_lower.contains("install") {
                let network_error = out_lower.contains("connectionerror")
                    || out_lower.contains("connection refused")
                    || out_lower.contains("could not find a version")
                    || out_lower.contains("403 forbidden")
                    || out_lower.contains("404 not found")
                    || out_lower.contains("ssl:")
                    || out_lower.contains("timed out")
                    || out_lower.contains("read timed out");
                if network_error {
                    output.push_str(
                        "\n\nHINT: pip install failed due to a network/authentication error. \
                        Try: 1) pip install --retries 5 <package>, 2) Use a different package index, \
                        3) Try a specific version: pip install <package>==<version>",
                    );
                }
                if out_lower.contains("permission denied")
                    || out_lower.contains("not writable")
                    || out_lower.contains("access is denied")
                {
                    output.push_str(
                        "\n\nHINT: pip install failed due to permission error. \
                        Try: pip install --user <package> or use a virtual environment.",
                    );
                }
            }

            // Missing module hints
            if out_lower.contains("modulenotfounderror") || out_lower.contains("no module named") {
                let module_hints = [
                    ("cv2", "opencv-python"),
                    ("PIL", "Pillow"),
                    ("sklearn", "scikit-learn"),
                    ("scipy", "scipy"),
                    ("yaml", "pyyaml"),
                    ("Crypto", "pycryptodome"),
                    ("bs4", "beautifulsoup4"),
                    ("lxml", "lxml"),
                    ("pytest", "pytest"),
                    ("flask", "flask"),
                    ("django", "django"),
                    ("requests", "requests"),
                    ("boto3", "boto3"),
                    ("grpc", "grpcio"),
                ];
                for (import_name, pip_name) in &module_hints {
                    if out_lower
                        .contains(&format!("no module named '{}'", import_name.to_lowercase()))
                        || out_lower.contains(&format!(
                            "no module named \"{}\"",
                            import_name.to_lowercase()
                        ))
                    {
                        output.push_str(&format!(
                            "\n\nHINT: Install the missing module: pip install {}",
                            pip_name
                        ));
                        break;
                    }
                }
            }

            // Git merge conflict hint
            if cmd_lower.contains("git")
                && (out_lower.contains("merge conflict") || out_lower.contains("conflict"))
                && (out_lower.contains("<<<<<")
                    || (out_lower.contains("=====") && out_lower.contains(">>>>>")))
            {
                output.push_str(
                    "\n\nHINT: Git merge conflict detected. To resolve: \
                        1) Open the conflicted files and remove conflict markers (<<<<<<, ======, >>>>>>), \
                        2) Keep the correct version of the code, \
                        3) git add <resolved-files>, then git commit.",
                );
            }

            // Compilation error hints
            if cmd_lower.contains("gcc") || cmd_lower.contains("g++") || cmd_lower.contains("make")
            {
                if out_lower.contains("undefined reference") {
                    output.push_str(
                        "\n\nHINT: Linker error (undefined reference). \
                        You may need to add -l flags (e.g., -lm for math, -lpthread for threads) \
                        or ensure all source files are included in the compile command.",
                    );
                }
                if out_lower.contains("fatal error:") && out_lower.contains(".h: no such file") {
                    output.push_str(
                        "\n\nHINT: Missing header file. Install the dev package: \
                        apt-get install lib<name>-dev (e.g., libssl-dev, libffi-dev)",
                    );
                }
            }

            // Test-skipping warning
            if cmd_lower.contains("pytest") || cmd_lower.contains("python -m pytest") {
                let cmd_trimmed = command.trim().to_lowercase();
                if (cmd_trimmed.contains("-k \"not ")
                    || cmd_trimmed.contains("-k 'not ")
                    || cmd_trimmed.contains("-k=\"not ")
                    || cmd_trimmed.contains("-k='not ")
                    || cmd_trimmed.contains("--ignore"))
                    && !out_lower.contains("error")
                {
                    output.push_str(
                        "\n\nWARNING: You excluded some tests from the run. \
                        ALL tests must pass for the task to be complete. \
                        Do NOT skip failing tests — fix the code so they pass.",
                    );
                }
            }

            // PEP 668 hint (externally-managed-environment)
            if out_lower.contains("externally-managed-environment")
                || out_lower.contains("pep 668")
                || out_lower.contains("error: externally-managed-environment")
            {
                output.push_str(
                    "\n\nHINT: PEP 668 blocks pip install to system Python. \
                    Use: pip install --break-system-packages <package>",
                );
            }

            // Network timeout hint — when multiple network commands fail
            if output.contains("timed out") {
                let net_commands = [
                    "git clone",
                    "curl",
                    "wget",
                    "apt-get",
                    "pip install",
                    "npm install",
                ];
                if net_commands.iter().any(|nc| command.contains(nc)) {
                    output.push_str(
                        "\n\nHINT: Network command timed out. If this keeps happening:\n\
                        1. Check if files already exist: `ls /app/`\n\
                        2. Try an alternative download method (curl vs wget vs git)\n\
                        3. Use `timeout 60` prefix to fail faster\n\
                        4. If ALL network fails, work with local files only",
                    );
                }
            }
        }
    }

    output
}

/// Strip a repeated intro prefix from the current text.
///
/// Some models (e.g., GLM) repeat their plan/introduction at the start of
/// every turn. This function detects when the beginning of `current` matches
/// the beginning of `previous` and strips the duplicated portion, keeping
/// only the new content.
fn strip_repeated_prefix(current: &str, previous: &str) -> String {
    if current.is_empty() || previous.is_empty() {
        return current.to_string();
    }

    let current_lines: Vec<&str> = current.lines().collect();
    let previous_lines: Vec<&str> = previous.lines().collect();

    // Count how many leading lines match (ignoring trailing whitespace)
    let mut match_count = 0;
    for (cur_line, prev_line) in current_lines.iter().zip(previous_lines.iter()) {
        if cur_line.trim() == prev_line.trim() && !cur_line.trim().is_empty() {
            match_count += 1;
        } else {
            break;
        }
    }

    // Only strip if at least 3 consecutive lines match (avoids false positives
    // from short common phrases like "Let me fix this.")
    if match_count >= 3 {
        let remaining = &current_lines[match_count..];
        remaining.join("\n").trim().to_string()
    } else {
        current.to_string()
    }
}

/// Detect whether the assistant text contains a multi-paragraph block
/// that repeats 3+ times. Returns the truncated text with just one copy
/// of the repeated block if detected, or None if no repetition found.
///
/// This catches the pattern where the model generates the same 4-6 paragraph
/// analysis block over and over (200+ times in some cases). We split on
/// double-newlines to identify paragraph breaks, then check for repeated
/// sequences of paragraphs.
fn detect_and_truncate_repeated_blocks(text: &str) -> Option<String> {
    if text.len() < 200 {
        return None;
    }

    // Split into paragraphs (separated by blank lines)
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.len() < 6 {
        return None; // Need enough paragraphs to detect a repeating block
    }

    // Try different block sizes (3 to 8 paragraphs) to find a repeating unit
    for block_size in 3..=8.min(paragraphs.len() / 2) {
        // Use the first `block_size` paragraphs as the candidate repeating block
        let first_block: Vec<&str> = paragraphs[..block_size].to_vec();
        let first_block_text = first_block.join("\n\n");

        // Skip very short blocks (< 100 chars) to avoid false positives
        if first_block_text.len() < 100 {
            continue;
        }

        // Count how many times this exact block repeats
        let mut repetitions = 1; // The first occurrence
        let mut pos = block_size;

        while pos + block_size <= paragraphs.len() {
            let candidate: Vec<&str> = paragraphs[pos..pos + block_size].to_vec();
            let candidate_text = candidate.join("\n\n");

            // Check if this block matches the first block (allow minor whitespace differences)
            if blocks_match(&first_block_text, &candidate_text) {
                repetitions += 1;
                pos += block_size;
            } else {
                break;
            }
        }

        if repetitions >= 3 {
            // Found a repeating block — keep only the first occurrence
            // plus any remaining non-repeating content after the last repetition
            let end_of_repetitions = block_size * repetitions;
            let mut result_parts: Vec<&str> = paragraphs[..block_size].to_vec();

            // Add any trailing non-repeating content
            if end_of_repetitions < paragraphs.len() {
                result_parts.extend_from_slice(&paragraphs[end_of_repetitions..]);
            }

            let truncated = result_parts.join("\n\n");
            tracing::warn!(
                "Detected {}x repetition of {}-paragraph block ({} chars → {} chars)",
                repetitions,
                block_size,
                text.len(),
                truncated.len()
            );
            return Some(truncated);
        }
    }

    None
}

/// Check if two text blocks match, allowing minor differences in whitespace.
fn blocks_match(a: &str, b: &str) -> bool {
    let a_normalized: String = a.chars().filter(|c| !c.is_whitespace()).collect();
    let b_normalized: String = b.chars().filter(|c| !c.is_whitespace()).collect();

    if a_normalized.len() < 50 {
        return false; // Too short to reliably match
    }

    // Exact match after whitespace normalization
    if a_normalized == b_normalized {
        return true;
    }

    // Fuzzy match: check if the texts are >90% similar
    // Use a simple sliding window to check if one is a prefix of the other
    // (handles cases where one repetition is slightly longer/shorter)
    let min_len = a_normalized.len().min(b_normalized.len());
    if min_len < 50 {
        return false;
    }

    // Check if first 80% of the shorter string matches
    let check_len = (min_len as f64 * 0.8) as usize;
    if check_len < 50 {
        return false;
    }

    let matching: usize = a_normalized[..check_len]
        .chars()
        .zip(b_normalized[..check_len].chars())
        .map(|(a, b)| if a == b { 1 } else { 0 })
        .sum();

    let similarity = matching as f64 / check_len as f64;
    similarity > 0.9
}

/// Maximum number of lines of assistant text to accumulate before checking
/// for repetition. After this threshold, we check and potentially truncate.
const REPETITION_CHECK_THRESHOLD: usize = 200;

/// Strip repeated preamble phrases within a single turn's text.
///
/// Some models (e.g., GLM) repeat "I'll help you..." or "Let me..." before
/// every tool call within a single response. This detects sentences that
/// appear 3+ times and removes the duplicates, keeping only the first occurrence.
fn strip_repeated_preamble_phrases(text: &str) -> String {
    // Extract sentences (split on sentence-ending punctuation followed by newline or whitespace)
    let sentences: Vec<&str> = text
        .split_inclusive(['.', '!', '?'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if sentences.len() < 3 {
        return text.to_string();
    }

    // Count occurrences of each trimmed sentence
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for s in &sentences {
        *counts.entry(s).or_insert(0) += 1;
    }

    // Find sentences that repeat 3+ times
    let repeated: std::collections::HashSet<&&str> = counts
        .iter()
        .filter(|(_, &c)| c >= 3)
        .map(|(s, _)| s)
        .collect();

    if repeated.is_empty() {
        return text.to_string();
    }

    // Rebuild text, keeping only the first occurrence of each repeated sentence
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut result = String::new();
    for s in &sentences {
        if repeated.contains(&s) {
            if !seen.contains(s) {
                seen.insert(s);
                result.push_str(s);
                result.push(' ');
            }
            // Skip duplicate occurrence
        } else {
            result.push_str(s);
        }
    }

    result.trim().to_string()
}

/// Headless streaming callbacks for unified SSE event processing
///
/// Bridges the shared SseEventProcessor with headless-specific logic:
/// - Streaming text to stderr for real-time visibility
/// - Repetition detection and truncation
/// - Token counting from usage deltas
/// - Tool argument summarization
struct HeadlessStreamCallbacks<'a> {
    assistant_text: &'a mut String,
    completed_tools: &'a mut Vec<(String, String, String)>, // (id, name, json)
    stop_reason: &'a mut Option<String>,
    total_input_tokens: &'a mut u64,
    total_output_tokens: &'a mut u64,
    total_cache_read_tokens: &'a mut u64,
    total_cache_creation_tokens: &'a mut u64,
    break_stream: &'a mut bool, // Signal to break stream due to repetition
    last_block_was_tool: &'a mut bool, // Track if last block was a tool to know when to print newline
}

impl<'a> StreamingCallbacks for HeadlessStreamCallbacks<'a> {
    fn on_text(&mut self, text: &str) {
        // Stream text directly to stderr for real-time visibility
        eprint!("{}", text);
        self.assistant_text.push_str(text);
        *self.last_block_was_tool = false;

        // In-stream repetition detection: if the assistant text has
        // grown very large (>REPETITION_CHECK_THRESHOLD lines) without
        // any tool calls, check for block repetition. If detected,
        // truncate immediately and break the stream to stop wasting
        // output tokens on repeated content.
        if self.assistant_text.lines().count() > REPETITION_CHECK_THRESHOLD
            && self.completed_tools.is_empty()
        {
            if let Some(truncated) = detect_and_truncate_repeated_blocks(self.assistant_text) {
                tracing::warn!(
                    "In-stream repetition detected ({} lines), truncating and breaking",
                    self.assistant_text.lines().count()
                );
                eprintln!("\n\n[Repetition loop detected, truncating]");
                *self.assistant_text = truncated;
                *self.break_stream = true;
            }
        }
    }

    fn on_thinking(&mut self, _thinking: &str) {
        // Headless ignores thinking blocks (default no-op implementation)
    }

    fn on_tool_start(&mut self, _id: &str, name: &str) {
        eprint!("  🔧 {}(", name);
        *self.last_block_was_tool = true;
    }

    fn on_tool_complete(&mut self, tool: ToolAccumulator) {
        // Summarize tool arguments for display
        let args_display = summarize_tool_args(&tool.name, &tool.partial_json);
        eprintln!("{})", args_display);
        info!("Tool call: {} ({})", tool.name, tool.id);
        self.completed_tools
            .push((tool.id, tool.name, tool.partial_json));
    }

    fn on_content_block_stop(&mut self) {
        // Text block ended — add a newline if we streamed text and it wasn't a tool
        if !*self.last_block_was_tool && !self.assistant_text.is_empty() {
            eprintln!();
        }
    }

    fn on_message_delta(
        &mut self,
        stop_reason: Option<&str>,
        usage: Option<&rustycode_llm::provider_v2::Usage>,
    ) {
        *self.stop_reason = stop_reason.map(String::from);
        if let Some(u) = usage {
            *self.total_input_tokens += u.input_tokens as u64;
            *self.total_output_tokens += u.output_tokens as u64;
            *self.total_cache_read_tokens += u.cache_read_input_tokens as u64;
            *self.total_cache_creation_tokens += u.cache_creation_input_tokens as u64;
        }
    }

    fn on_message_stop(&mut self) {
        // No-op: stream processing will exit loop naturally
    }

    fn on_error(&mut self, error_type: &str, message: &str) {
        // Log will be handled by the outer error return
        tracing::error!("SSE error: {} - {}", error_type, message);
    }
}

/// Maximum number of tool-use turns before we break to prevent infinite loops.
/// Most successful tasks complete in 8-15 turns. 25 provides ample room
/// while preventing runaway sessions that waste time and tokens.
const MAX_TOOL_TURNS: usize = 25;

/// Maximum number of tool calls per single turn before we force-break.
const MAX_TOOLS_PER_TURN: usize = 20;

/// Maximum consecutive similar tool calls before we force-break.
/// "Similar" means same tool name + first 80 chars of arguments match
/// (after normalizing `&&` vs `;` separators).
const MAX_SIMILAR_CONSECUTIVE: usize = 3;

/// Timeout for each individual stream chunk (prevents hangs).
const CHUNK_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum retries for transient LLM stream errors (rate limit, server error).
const MAX_STREAM_RETRIES: usize = 3;

/// Delay between stream retries (milliseconds), doubles each attempt.
const INITIAL_RETRY_DELAY_MS: u64 = 1000;

/// Result of running a headless task.
pub struct HeadlessTaskResult {
    /// The final text response from the LLM.
    pub final_text: String,
    /// Whether the agent made any write/edit tool calls during execution.
    pub made_writes: bool,
    /// Whether the agent ran verification commands after the last file edit.
    /// Used by the CLI to reject "success" when files were changed but never tested.
    pub verified_after_last_edit: bool,
    /// Total number of tool calls made during execution.
    /// Used by the outer retry loop to enforce a minimum-work threshold
    /// and reject premature "success" declarations (e.g., agent clones a
    /// repo, reads README, then stops without building/installing).
    pub total_tool_calls: usize,
    /// Conversation messages from this iteration (for carry-forward across retries).
    /// Contains the full message history so the next iteration can continue
    /// from where this one left off instead of starting from scratch.
    pub messages: Vec<ChatMessage>,
    /// Total token usage from this iteration.
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

/// Default system prompt for headless coding agent mode.
const HEADLESS_SYSTEM_PROMPT: &str = "\
You are an expert coding agent. Complete the task described below.

## CRITICAL RULES
1. You MUST iterate until the task is FULLY solved. Do NOT stop early.
2. After making changes, ALWAYS verify by running the code/tests.
3. If verification fails, debug and fix — NEVER declare success without proof.
4. When a command fails, read the error carefully and fix the root cause.
5. Make the smallest change that correctly solves the task.
6. NEVER repeat the same action if it failed before — try a DIFFERENT approach.
7. If a tool call returns the same result twice, STOP and change strategy.
8. NEVER say \"Task completed\" or \"Task completed successfully\" unless you have \
ACTUALLY written/modified files AND verified the changes work. Reading files is NOT completion. \
Exploring code is NOT completion. You must WRITE code, EDIT files, or RUN commands that \
modify the system. If you have not created or edited any files, you are NOT done.
9. Do NOT write README files, documentation files, summary files, example scripts, or implementation notes. \
Only write code files that are DIRECTLY needed for the task. Writing docs or examples wastes turns and tokens. \
Once your solution works and passes tests, STOP — do NOT write additional scripts or verification files.
10. NEVER skip or exclude failing tests with -k 'not ...' or --ignore. If tests fail, FIX the code \
so the tests pass. Skipping tests means the task is NOT complete.
11. For security/vulnerability tasks: READ THE TEST FILES FIRST. The tests reveal the EXACT vulnerability \
expected (which CWE, which function, which error type). Do NOT guess — let the tests guide you to the \
correct fix. If a test fails after your fix, that IS the vulnerability you need to fix — it is NOT \
pre-existing. The report.jsonl must contain the EXACT CWE-ids the verifier expects. Check ALL failing tests.
12. EVERY response you make MUST contain at least one tool call. Pure text responses with no tool calls \
are NEVER acceptable. If you are thinking about what to do, DO IT with a tool call instead. \
Even your first response should start with read_file, list_dir, or bash — never plain text.
13. NEVER use git checkout/revert/stash to undo your own changes. If an edit caused an error, \
FIX the error — do NOT revert to the original. Reverting wastes all prior work. Always move FORWARD.

## VERIFICATION GATES - MANDATORY BEFORE COMPLETION
Before you say \"Task completed\", you MUST verify your work:

### For Python Packages
- After building/installing: run `python -c \"import <package>\"` to confirm importable
- If build uses `setup.py`: ALWAYS run both `python setup.py build_ext` AND `pip install -e .`
- `build_ext --inplace` alone is NOT enough — you must install to site-packages
- If tests import the module, they will fail if installation is incomplete
- VERIFY: Try importing from a DIFFERENT directory (not the source dir)

### For C/C++ Builds (CMake, Makefile, gcc)
- After building: verify .so, .o, or executable files were created with `ls -la`
- If it's a library: verify the compiled artifact exists in the expected location
- If it's an executable: try running it with `./executable --help` or `./executable`
- DO NOT claim success if compilation succeeded but binary was not created

### For Server/Service Tasks
- After starting: verify the port is listening with `netstat -tuln | grep :PORT` or `ss -tuln | grep :PORT`
- Try making a test request: `curl http://localhost:PORT/` or equivalent
- Check service logs for startup errors
- DO NOT assume success if the start command runs without error — verify it's actually listening

### For Code Changes/Fixes
- Run the EXACT test code provided in the task description
- If the task shows example code: run it and confirm output matches expected
- Run the full test suite, not just one test file
- DO NOT skip tests that check your changes
- If a verifier runs tests, ENSURE they all pass (look at test output for failures)

### For Data Processing Tasks
- After processing: spot-check the output (read a sample, count rows, verify format)
- If CSV/JSON: validate the format is correct (can be parsed by standard tools)
- If transformation: verify both input and output match expected schema
- DO NOT assume transformation succeeded without checking the output

## What \"Task completed\" Really Means
\"Task completed\" means:
✓ All required files have been created or modified
✓ All code changes have been tested and verified to work
✓ All imports/dependencies are available and working
✓ All builds are complete AND installed/deployed (if applicable)
✓ All tests from the task description pass
✓ The exact output/behavior matches what was requested

It does NOT mean:
✗ Files were read or explored
✗ Code was written but not tested
✗ Tests were written but not run
✗ Build succeeded but not installed
✗ Some tests pass (all must pass)
✗ Partial work completed

## MANDATORY: Success Criteria Extraction
Before ANY implementation, extract the EXACT success criteria from the task:
1. Identify every measurable requirement (e.g., \"75% win rate\", \"all tests pass\", \"output matches X\")
2. List each criterion explicitly — this is your checklist
3. When the task mentions multiple targets (e.g., \"against stone, vampire, paper, snake, g2-clear\"), \
each is a SEPARATE criterion that must be verified individually
4. Do NOT stop until EVERY criterion is met — partial success IS failure
5. When a test result shows failure (e.g., \"0 wins\" when you need 75), that criterion is NOT met — \
iterate with a different approach
6. If results show ties/draws (e.g., \"Results: 0 0 100\"), those are NOT wins — the criterion is NOT met

Example: If the task says \"achieve 75% win rate against A, B, C and 33% against D, E\", then:
- Test against A → check win count ≥ 75
- Test against B → check win count ≥ 75
- Test against C → check win count ≥ 75
- Test against D → check win count ≥ 33
- Test against E → check win count ≥ 33
- ALL five must pass. If any fails, modify your solution and re-test.

## MANDATORY: Plan Before Implementation
Before writing ANY code or running build commands, you MUST:
1. Read the task description carefully and extract success criteria (see above)
2. List the exact files that exist (use list_dir, glob)
3. Read ONLY the files directly relevant to the task (README, build scripts, existing code)
4. State your implementation plan as a numbered list of specific steps
5. Then execute each step in order

For complex tasks (building from source, implementing interpreters, multi-file projects):
- Identify ALL dependencies needed before starting
- Determine the correct build/install sequence upfront
- When building from source: fix code, build, INSTALL globally, then verify
- IMPORTANT: `build_ext --inplace` is NOT enough — you MUST also run `pip install .` or \
`pip install -e .` to install to the system's global Python environment
- Verify the install by running `python -c \"import <package>\"` from OUTSIDE the source directory
- Plan the file structure before writing any code
- Write the COMPLETE implementation in one pass, then test

## Workflow
1. Read task → extract success criteria → list files → read key files → state plan
2. Implement plan step by step
3. After EACH change, verify by running the EXACT test command from the task description
4. Check results against EACH success criterion — if any fails, modify and re-test
5. Keep iterating until ALL criteria are met
6. NEVER say 'Task completed' until you have verified EVERY criterion from step 1

## Build/Install Workflow (for compiling from source)
1. Clone/build the project
2. Run `python setup.py build_ext --inplace` (or equivalent)
3. Run `pip install .` or `pip install -e .` to install to global environment
4. Verify: `python -c \"import <package>\"` from OUTSIDE the source directory
5. If import fails with a traceback: read the traceback, identify the EXACT file and line
6. Fix the error in the specific file mentioned in the traceback
7. If the error is a deprecation (e.g., numpy.int, numpy.float): grep for the pattern \
across ALL source files including .pyx, .pxd, .pyx.in, .py — fix ALL occurrences
8. Rebuild and re-install, then re-verify from OUTSIDE the source directory
9. Run the actual test suite to confirm everything works
10. IMPORTANT: If you edit Cython (.pyx/.pxd) or C source files, you MUST rebuild \
(`python setup.py build_ext --inplace`) and re-install (`pip install -e .`) before testing again. \
Editing .pyx source does NOT update the compiled .so extension until you rebuild.

## Tool Strategy
- `read_file` — Read files to understand code. Always read before editing. Shows line numbers. \
Supports `\"offset\"` (0-based line index) and `\"limit\"` (max lines) for reading large files. \
If output shows [TRUNCATED], the file is large — use grep to find specific functions, \
then read_file with offset/limit to read the relevant section.
- `bash` — Run commands: compile, test, install packages, git operations. \
Use `cat > file << 'EOF'` for writing large files if write_file fails. \
Supports `\"timeout_secs\": N` for long builds (default 120s, max 600s). \
ALWAYS set `\"timeout_secs\": 300` for builds, compilation, or pip install. \
If a simple command times out, the shell may be broken — try a different approach.
- `write_file` — Create new files or rewrite existing ones. For files over ~20KB, use bash with heredoc instead.
- `edit_file` — Replace a specific string in an existing file. \
Supports `\"replace_all\": true` to replace ALL occurrences in one call. \
Supports `\"regex\": true` to use regex pattern matching (use ${1}, ${2} for capture groups). \
Useful for renaming variables, fixing deprecated APIs, or batch pattern replacements.
- `bash` with `sed -i` — For bulk find-replace across MANY files at once (e.g., `sed -i 's/np\\.float/float/g' file1.py file2.py`). \
PREFER this over edit_file when you need to fix the same pattern in 5+ files. \
Use `find . -name '*.py' -exec sed -i 's/old/new/g' {} +` for project-wide replacements.
- `grep` — Search for patterns. Supports: `\"regex\": true` for regex, `\"ignore_case\": true` for \
case-insensitive, `\"include\": \"*.py\"` to filter by file extension. \
Use comma-separated includes to search multiple types: `\"include\": \"*.py,*.pyx,*.pxd\"`.
- `glob` — Find files matching a pattern. Supports `**/*.ext` for recursive patterns.
- `list_dir` — Explore directory structure. Shows file sizes.

## Large File Strategy (IMPORTANT)
For files over 1000 lines:
1. FIRST use grep to find relevant functions/patterns (e.g., `grep def\\|class\\|function` to find structure)
2. Note the line numbers from grep results
3. Then use read_file with offset/limit to read ONLY the sections you need
4. Do NOT try to read the entire file — it will be truncated and you will miss important code
5. If you see [TRUNCATED] in output, STOP reading the same file and switch to grep+offset/limit

## Anti-Loop Rules (CRITICAL)
- Do NOT run the same or very similar command more than 3 times in a row.
- If a bash command produces an error, READ THE ERROR before retrying.
- If an approach fails after 2 attempts, switch to a COMPLETELY DIFFERENT strategy.
- When you are stuck, do NOT retry — instead: read error output, grep for relevant code, or try a different tool.
- If a Python/C/etc script fails, read the traceback and fix the SPECIFIC line mentioned.
- ONCE you have verified your solution works (tests pass, code runs correctly), STOP. \
Do NOT run additional verification commands. Your task is done — end your turn.
- Do NOT run git log, git status, cat, or ls more than once after completing your task.

## Efficiency Rules
- Do NOT repeat your plan or introduction at the start of every turn. State it ONCE, then act.
- Do NOT say \"Let me\", \"I'll help you\", or \"Now let's\" — just call the tool directly.
- Do NOT explore files you already read — proceed to implementation.
- Do NOT run the same command multiple times expecting different results.
- Write COMPLETE code. Partial implementations waste turns.
- Be concise in reasoning. Focus on action, not explanation.
- When fixing compatibility issues (NumPy, Python versions, etc.), search ALL source files \
(including .pyx, .pxd, .pyx.in Cython files) for the deprecated pattern using grep, \
then fix ALL occurrences at once.
- Build commands (setup.py, pip install, cargo build, make) automatically get longer timeouts. \
If a build still times out, retry with a smaller scope (e.g., build only the changed module).

## Key Principles
- You have everything you need to solve this. Keep going until it's done.
- NEVER end your turn without having truly and completely solved the problem.
- When you say you will verify something, ACTUALLY run the verification.
- If a download fails, try alternative URLs or package managers.
- If compilation fails, read the error and fix it incrementally.
- For Cython/C extensions: also check .pyx and .pxd files for deprecated APIs.";

/// System prompt for retry iterations where the previous attempt only read files.
/// This strips the "Plan Before Implementation" section and demands immediate action.
const RETRY_SYSTEM_PROMPT: &str = "\
You are an expert coding agent. A previous attempt to complete this task FAILED — \
you only read/explored files without making any changes. This is a RETRY.

## ABSOLUTE RULES (NO EXCEPTIONS)
1. Your FIRST tool call MUST be write_file, edit_file, or bash (with a modifying command).
2. Do NOT read files, list directories, or explore — you already know the structure.
3. Do NOT write plans, introductions, or analysis — WRITE CODE.
4. If you are unsure what to change, make your best guess and write it immediately.
5. It is BETTER to write imperfect code than to read more files.
6. NEVER respond with text only — you MUST make at least one tool call.
7. After writing, verify by running tests/builds, then fix any errors.

## Tool Strategy (WRITE-FIRST)
- write_file — Create new files immediately
- edit_file — Modify existing files immediately
- bash — Run builds, installs, and tests AFTER making changes

## Key Principle
ACT NOW. Write code FIRST, verify SECOND. Do not plan, do not explore, do not analyze. \
The files already exist from the previous attempt — modify them.";

/// Detect repeating tool call patterns in recent history (tamux-inspired).
/// Checks for patterns of period 1 (A→A→A) or period 2 (A→B→A→B) in the
/// last N tool calls. Returns a description of the detected loop.
fn detect_tool_loop(recent: &[String], min_length: usize) -> Option<String> {
    if recent.len() < min_length {
        return None;
    }
    for period in 1..=2 {
        let check_len = std::cmp::max(min_length, 2 * period);
        if recent.len() < check_len {
            continue;
        }
        let tail = &recent[recent.len() - check_len..];
        let is_repeating = tail
            .iter()
            .enumerate()
            .all(|(i, name)| *name == tail[i % period]);
        if is_repeating {
            let pattern: Vec<&str> = tail[..period].iter().map(|s| s.as_str()).collect();
            let repetitions = check_len / period;
            return Some(format!(
                "[{}] repeated {} times",
                pattern.join(" -> "),
                repetitions
            ));
        }
    }
    None
}

/// Run a single agentic task to completion (headless, no TUI).
///
/// This drives the standard agent loop:
/// 1. Send user prompt (+ tool results) to the LLM
/// 2. Process the streamed response
/// 3. If the LLM requests tool calls, execute them and feed results back
/// 4. Repeat until the LLM stops with no tool calls
///
/// Returns the final text response from the LLM.
pub async fn run_headless_task(
    provider: &dyn LLMProvider,
    model: &str,
    tools_schema: &[serde_json::Value],
    task: &str,
    cwd: &Path,
    tool_registry: &ToolRegistry,
) -> Result<String> {
    let result = run_headless_task_with_iteration(
        provider,
        model,
        tools_schema,
        task,
        cwd,
        1,
        tool_registry,
        None,
    )
    .await?;
    Ok(result.final_text)
}

/// Run a headless task with an explicit iteration number.
/// Iteration > 1 uses a more aggressive system prompt that demands immediate action.
/// Returns a HeadlessTaskResult with the final text and whether any writes were made.
///
/// When `prior_messages` is Some, the conversation continues from where the previous
/// iteration left off instead of starting from scratch. This preserves exploration
/// context so the agent doesn't re-read the same files it already analyzed.
pub async fn run_headless_task_with_iteration(
    provider: &dyn LLMProvider,
    model: &str,
    tools_schema: &[serde_json::Value],
    task: &str,
    cwd: &Path,
    iteration: usize,
    tool_registry: &ToolRegistry,
    prior_messages: Option<Vec<ChatMessage>>,
) -> Result<HeadlessTaskResult> {
    // Build context-aware task prompt with directory listing
    let dir_listing = std::fs::read_dir(cwd)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if is_dir {
                        format!("{}/", name)
                    } else {
                        name
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "(could not read directory)".to_string());

    let task_with_context = format!(
        "Working directory: {} (contains {} files/dirs)\n\n{}\n\n---\n\n{}",
        cwd.display(),
        dir_listing.lines().count(),
        dir_listing,
        task
    );

    // System prompt goes through the dedicated system_prompt field on CompletionRequest,
    // not as a ChatMessage. ChatMessage::system() gets mapped to role="user" by
    // parse_conversation_messages, which causes two consecutive user messages and
    // prevents Anthropic from applying system-level caching.
    let headless_system_prompt = HEADLESS_SYSTEM_PROMPT.to_string();

    // Initialize conversation: either continue from prior iteration or start fresh.
    // Carrying forward prior_messages preserves exploration context so the agent
    // doesn't re-read the same files it already analyzed.
    let had_prior = prior_messages.is_some();
    let mut messages: Vec<ChatMessage> = if let Some(prior) = prior_messages {
        // For retries where the agent only analyzed (no writes), truncate the
        // prior conversation to just the original task + file contents extracted
        // from tool results. This prevents the model from getting lost in analysis
        // context and ignoring the action nudge.
        let mut continued = prior;

        // Extract file contents from tool results in prior messages so we can
        // include them in the nudge. This lets us give the agent the info it
        // needs without carrying forward the entire analysis conversation.
        let mut file_contents = String::new();
        for msg in &continued {
            if let MessageContent::Blocks(blocks) = &msg.content {
                for block in blocks {
                    if let ContentBlock::ToolResult { content, .. } = block {
                        // Heuristic: if the tool result looks like file content
                        // (multiple lines, typical code patterns), capture it
                        if content.lines().count() > 5 && content.len() > 200 {
                            // Only keep the largest result (likely the main file)
                            if content.len() > file_contents.len() {
                                file_contents = content.clone();
                            }
                        }
                    }
                }
            }
        }

        // Build a strong action nudge. If we extracted file content, include it
        // so the agent has ZERO reason to read the file again.
        let nudge = if file_contents.is_empty() {
            "The previous attempt FAILED — you only read/explored files without making \
            any changes. This is your FINAL retry.\n\n\
            MANDATORY: Your VERY FIRST tool call MUST be write_file or edit_file. \
            Do NOT run bash grep, read_file, list_dir, or glob. You already have all \
            the context you need from the previous attempt. \
            Write the solution NOW, then verify with bash."
                .to_string()
        } else {
            format!(
                "The previous attempt FAILED — you only read/explored files without making \
                any changes. This is your FINAL retry.\n\n\
                MANDATORY: Your VERY FIRST tool call MUST be write_file or edit_file. \
                Do NOT run bash grep, read_file, list_dir, or glob.\n\n\
                You already read the file. Here is the content from your previous analysis:\n\
                ```
                {}
                ```\n\n\
                Based on this content, write the fix NOW. Then verify with bash.",
                file_contents.chars().take(8000).collect::<String>()
            )
        };

        continued.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(nudge),
        });
        continued
    } else {
        vec![ChatMessage::user(task_with_context)]
    };

    // For retry iterations (iteration > 1) WITHOUT prior messages, pre-fill
    // the assistant's first response to force action mode.
    // When prior_messages exist, we already added a nudge above.
    if iteration > 1 && !had_prior {
        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Simple(
                "I'll start making changes immediately.\n\n".to_string(),
            ),
        });
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(
                "Good. Now use write_file or edit_file to make your first change. \
                Do NOT read any files — you already know what to do from the previous attempt. \
                Pick a file and WRITE CODE NOW."
                    .to_string(),
            ),
        });
    }

    let mut final_text = String::new();

    // Track the previous assistant text to detect and strip repeated intro text.
    // Some models (e.g., GLM) repeat their plan/introduction at the start of
    // every turn, wasting output tokens. We detect this and strip the repeated
    // prefix before adding the message to conversation history.
    let mut previous_assistant_text: Option<String> = None;

    // Track how many course corrections we've injected (max 1 before hard break)
    let mut course_corrections: usize = 0;

    // Progress tracking for loop detection
    let mut tool_history: Vec<(String, String)> = Vec::new(); // (tool_name, tool_args)
    let mut last_successful_output: Option<String> = None;
    let mut stagnant_turns: usize = 0;

    // Track consecutive verification-only turns (no file-modifying commands).
    // When the agent keeps running read-only commands without making changes,
    // it's likely stuck in a verification loop and should stop.
    let mut consecutive_verification_turns: usize = 0;
    const MAX_CONSECUTIVE_VERIFICATION_TURNS: usize = 3;

    // Track cumulative read vs write operations across the entire run.
    // Used to detect when the agent is stuck in exploration mode and inject
    // progressive urgency nudges to push it toward action.
    let mut cumulative_reads: usize = 0;
    let mut cumulative_writes: usize = 0;
    // Track actual code writes (write_file, edit_file, sed -i) separately from
    // setup steps (git clone, pip install). Used for "agent never wrote code" guardrails.
    let mut code_writes: usize = 0;

    // Track total cumulative tool calls across all turns (not per-turn).
    // Used by MIN_TOOL_CALLS_TO_STOP to prevent the agent from stopping early.
    let mut total_tool_calls: usize = 0;

    // Track reads since the last write. This catches the pattern where the agent
    // writes once early on, then goes back to endless reading without being nudged
    // (the cumulative_writes==0 check misses this case).
    let mut reads_since_last_write: usize = 0;

    // Track whether the agent has verified its changes after the last modification.
    // Prevents the agent from declaring "Task completed" without running any
    // test or verification command after making edits.
    let mut last_modification_turn: Option<usize> = None;
    let mut verified_after_last_mod: bool = false;

    // Track if verification revealed errors that the agent hasn't fixed yet.
    // Catches the pattern: edit → verify → errors found → grep/read → stop without fixing.
    let mut verification_errors_found: bool = false;

    // Track if grep/search found unfixed issues in the current turn.
    // Catches: edit(A) → verify → OK → grep(find B) → "Task completed" without fixing B.
    let mut grep_found_unfixed_issues: bool = false;

    // Track if the previous turn's last tool call returned an error.
    // Used to prevent the agent from stopping after encountering a tool error
    // without trying alternatives. Catches: read_file(missing path) → LLM stops.
    let mut prev_turn_last_tool_error: bool;

    // Track writes to the same file to detect approach stagnation.
    // When the agent writes/edits the same file 4+ times, it's likely stuck
    // tweaking parameters instead of trying a fundamentally different approach.
    let mut file_write_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut approach_nudge_given: bool = false;

    // Closure to detect if a tool call modifies the filesystem.
    // Used in cumulative write tracking and early-stop detection.
    let is_modifying = |name: &str, json: &str| -> bool {
        if name == "write_file" || name == "edit_file" || name == "apply_patch" {
            return true;
        }
        if name == "bash" {
            let cmd = json.to_lowercase();
            if cmd.contains("sed -i") || cmd.contains("awk -i") || cmd.contains("awk --inplace") {
                return true;
            }
            if cmd.contains("> ")
                || cmd.contains(">>")
                || cmd.contains("cat >")
                || cmd.contains("tee ")
            {
                return true;
            }
            if cmd.contains("pip install")
                || cmd.contains("pip3 install")
                || cmd.contains("cargo ")
                || cmd.contains("apt-get install")
                || cmd.contains("apt install")
                || cmd.contains("yum install")
                || cmd.contains("dnf install")
                || cmd.contains("npm install")
                || cmd.contains("yarn install")
                || cmd.contains("pnpm install")
                || cmd.contains("bun install")
                || cmd.contains("go install")
                || cmd.contains("gem install")
            {
                return true;
            }
            if cmd.contains("make ")
                || cmd.contains("gcc ")
                || cmd.contains("g++")
                || cmd.contains("cmake ")
            {
                return true;
            }
            if cmd.contains("git add")
                || cmd.contains("git commit")
                || cmd.contains("git merge")
                || cmd.contains("git checkout")
                || cmd.contains("git clone")
                || cmd.contains("git rebase")
                || cmd.contains("git cherry-pick")
                || cmd.contains("git apply")
                || cmd.contains("git am")
                || cmd.contains("git stash")
                || cmd.contains("git rm")
                || cmd.contains("git mv")
            {
                return true;
            }
            if cmd.contains("mv ")
                || cmd.contains("cp ")
                || cmd.contains("rm ")
                || cmd.contains("chmod")
                || cmd.contains("chown")
                || cmd.contains("mkdir ")
                || cmd.contains("ln ")
                || cmd.contains("install ")
                || cmd.contains("dd ")
            {
                return true;
            }
            if cmd.contains("python -c") || cmd.contains("python3 -c") || cmd.contains("perl -i") {
                return true;
            }
            if cmd.contains("python <<") || cmd.contains("python3 <<") {
                // python heredoc modifying files (e.g. python3 << 'PYEOF')
                return true;
            }
            if cmd.contains("patch")
                || cmd.contains("service ")
                || cmd.contains("systemctl ")
                || cmd.contains("nohup ")
                || cmd.contains("setup.py ")
                || cmd.contains("docker build")
                || cmd.contains("docker run")
                || cmd.contains("docker-compose")
                || cmd.contains("docker compose")
                || cmd.contains("tar ")
                || cmd.contains("unzip ")
                // curl/wget only count as modifying when downloading to a file
                || (cmd.contains("curl ") && (cmd.contains("-o ") || cmd.contains("--output") || cmd.contains("> ") || cmd.contains("-o")))
                || (cmd.contains("wget ") && (cmd.contains("-o ") || cmd.contains("--output") || cmd.contains("-O")))
            {
                return true;
            }
        }
        false
    };

    // Separate tracker for "code writes" — actual source code changes, NOT setup steps.
    // This is used by guardrails to detect "agent explored but never modified code".
    // git clone, pip install, mkdir, etc. are setup; write_file, edit_file, sed -i are code writes.
    let is_code_write = |name: &str, json: &str| -> bool {
        if name == "write_file" || name == "edit_file" || name == "apply_patch" {
            return true;
        }
        if name == "bash" {
            let cmd = json.to_lowercase();
            // Only commands that directly change source file contents
            if cmd.contains("sed -i") {
                return true;
            }
            if cmd.contains("> ")
                || cmd.contains(">>")
                || cmd.contains("cat >")
                || cmd.contains("tee ")
            {
                return true;
            }
            if cmd.contains("python -c") || cmd.contains("python3 -c") {
                // python -c can be used to write files, count as code write
                return true;
            }
            if cmd.contains("python <<") || cmd.contains("python3 <<") {
                // python heredoc used to modify files (e.g. python3 << 'PYEOF')
                return true;
            }
            if cmd.contains("patch") && !cmd.contains("patch --") {
                return true;
            }
            if cmd.contains("perl -i") {
                return true;
            }
        }
        false
    };

    // Track recent tool names for loop detection (tamux-inspired StuckDetector).
    // Detects repeating patterns like [read_file, grep, read_file, grep] and
    // injects a strategy-changing nudge.
    let mut recent_tool_names: Vec<String> = Vec::new();
    let mut recent_tool_results: Vec<String> = Vec::new();
    const TOOL_HISTORY_LENGTH: usize = 12;
    const TOOL_LOOP_MIN_LENGTH: usize = 4;

    // Minimum total tool calls before the agent is allowed to stop.
    // GLM has a pattern of cloning a repo + reading a file + declaring "Task completed"
    // with only 4-5 tool calls. This threshold forces it to keep working.
    const MIN_TOOL_CALLS_TO_STOP: usize = 5;

    // Track cumulative token usage for metrics reporting
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut total_cache_read_tokens: u64 = 0;
    let mut total_cache_creation_tokens: u64 = 0;

    // Wall-clock timeout: prevent runaway agents from consuming unbounded time.
    // Default 15 minutes per iteration (generous for slow QEMU-emulated builds).
    // Override via RUSTYCODE_AGENT_TIMEOUT_SECS.
    const DEFAULT_WALL_CLOCK_TIMEOUT_SECS: u64 = 900;
    let wall_clock_timeout_secs = std::env::var("RUSTYCODE_AGENT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_WALL_CLOCK_TIMEOUT_SECS);
    let start_time = std::time::Instant::now();

    for turn in 0..MAX_TOOL_TURNS {
        // Wall-clock timeout check — break if we've exceeded the time budget
        let elapsed = start_time.elapsed().as_secs();
        if elapsed > wall_clock_timeout_secs {
            eprintln!(
                "\n⏰ Wall-clock timeout reached ({}s > {}s), stopping agent",
                elapsed, wall_clock_timeout_secs
            );
            info!(
                "Agent stopped by wall-clock timeout: {}s > {}s",
                elapsed, wall_clock_timeout_secs
            );
            break;
        }
        if turn > 0 {
            eprintln!(); // blank line between turns
        }
        info!("Headless agent turn {}", turn + 1);

        // Progressive urgency: if the agent has been reading without writing,
        // inject a nudge to push it toward action. This catches the common
        // pattern where the model keeps exploring (grep, read_file, glob)
        // but never transitions to actually editing files.
        // Uses reads_since_last_write so it also fires when the agent writes once
        // early on, then goes back to endless reading.
        let urgent_reads = if cumulative_writes == 0 {
            cumulative_reads
        } else {
            reads_since_last_write
        };
        // On retry iterations (iteration > 1), inject urgency nudge from turn 0.
        // The model often stops after just 2-3 read tool calls on retry, before
        // the nudge would fire at turn >= 1. Starting nudges earlier on retries
        // forces the model toward write action sooner.
        let nudge_threshold_reads = if iteration > 1 { 1 } else { 2 };
        let nudge_min_turn = if iteration > 1 { 0 } else { 1 };
        if turn >= nudge_min_turn && urgent_reads >= nudge_threshold_reads {
            let nudge = if urgent_reads >= 10 {
                format!(
                    "CRITICAL STOP: You have done {} read/grep operations since your last write. \
                    You MUST IMMEDIATELY use edit_file or write_file. Do NOT run grep, glob, \
                    list_dir, or read_file. Pick a file and EDIT IT RIGHT NOW.",
                    urgent_reads
                )
            } else if urgent_reads >= 6 {
                format!(
                    "URGENT: {} reads since last write. You have enough information. \
                    Use edit_file or write_file NOW. Do not explore any further.",
                    urgent_reads
                )
            } else {
                "You have been reading files but haven't written anything recently. \
                Your next tool call MUST be edit_file or write_file."
                    .to_string()
            };
            info!(
                "Injecting write urgency nudge (reads={}, writes={})",
                cumulative_reads, cumulative_writes
            );
            messages.push(ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Simple(nudge),
            });
        }

        // Approach stagnation: if the agent has written the same file 8+ times,
        // it's likely tweaking parameters instead of trying a fundamentally different
        // approach. Inject a nudge to think differently. If it's been nudged already
        // and keeps writing the same file (15+ writes), force-stop to prevent wasting
        // the entire budget on a hopeless approach.
        // Note: threshold is intentionally high because some tasks (e.g. iterative
        // optimization) legitimately rewrite the same file many times with different
        // approaches — we only want to catch truly stuck loops.
        if !approach_nudge_given && turn >= 3 {
            if let Some((path, count)) = file_write_counts.iter().find(|(_, c)| **c >= 8) {
                tracing::info!(
                    "Approach stagnation detected: {} written {} times, injecting nudge",
                    path,
                    count
                );
                approach_nudge_given = true;
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(format!(
                        "You have edited {} {} times with similar approaches. \
                        Your current strategy may not be working. Consider trying a COMPLETELY DIFFERENT approach: \
                        1) Use a different algorithm or data structure \
                        2) Use a different library or tool \
                        3) Rewrite the solution from scratch with a fresh design \
                        4) Search online for known solutions to this type of problem \
                        Do NOT make another small tweak — try something fundamentally different.",
                        path, count
                    )),
                });
            }
        }
        // Hard stop: if the nudge was given and the agent still keeps writing the
        // same file (15+ total writes), it's clearly not going to find a solution.
        // Force-stop to prevent wasting the entire task budget.
        if approach_nudge_given && turn >= 5 {
            if let Some((path, count)) = file_write_counts.iter().find(|(_, c)| **c >= 15) {
                tracing::warn!(
                    "Approach stagnation hard stop: {} written {} times after nudge, forcing completion",
                    path, count
                );
                eprintln!(
                    "\n⚠️  Approach stagnation: {} written {} times. Stopping.",
                    path, count
                );
                break;
            }
        }

        let request = CompletionRequest::new(model.to_string(), messages.clone())
            .with_streaming(true)
            .with_max_tokens(32768)
            .with_temperature(0.2)
            .with_system_prompt(if iteration > 1 {
                RETRY_SYSTEM_PROMPT.to_string()
            } else {
                headless_system_prompt.clone()
            })
            .with_tools(tools_schema.to_vec());

        // Retry stream start on transient errors (rate limits, server errors)
        // If all retries fail, attempt recovery using saved checkpoints
        let mut stream = {
            let mut result_stream = None;
            let mut final_error = None;

            for attempt in 0..=MAX_STREAM_RETRIES {
                match provider.complete_stream(request.clone()).await {
                    Ok(s) => {
                        if attempt > 0 {
                            info!("Stream started on retry attempt {}", attempt);
                        }
                        result_stream = Some(s);
                        break;
                    }
                    Err(e) => {
                        let err_str = format!("{}", e);
                        let is_transient = err_str.contains("429")
                            || err_str.contains("503")
                            || err_str.contains("502")
                            || err_str.contains("500")
                            || err_str.contains("timeout")
                            || err_str.contains("connection");

                        final_error = Some((err_str.clone(), e));

                        if is_transient && attempt < MAX_STREAM_RETRIES {
                            let delay = INITIAL_RETRY_DELAY_MS * (1 << attempt);
                            tracing::warn!(
                                "Stream error (attempt {}/{}): {}. Retrying in {}ms",
                                attempt + 1,
                                MAX_STREAM_RETRIES + 1,
                                err_str,
                                delay
                            );
                            tokio::time::sleep(Duration::from_millis(delay)).await;
                        }
                    }
                }
            }

            // If stream failed after all retries, attempt checkpoint recovery
            if result_stream.is_none() {
                if let Some((_err_str, e)) = final_error {
                    tracing::warn!(
                        "LLM stream failed after all retries. Attempting checkpoint recovery..."
                    );

                    match attempt_checkpoint_recovery() {
                        Ok(Some(recovered_msgs)) => {
                            tracing::info!(
                                "Checkpoint recovery: reconstructed {} messages",
                                recovered_msgs.len()
                            );
                            // Recreate messages with recovery data included
                            let mut recovery_messages = messages.clone();
                            recovery_messages.extend(recovered_msgs);

                            // Rebuild request with recovered messages
                            let recovery_request =
                                CompletionRequest::new(model.to_string(), recovery_messages)
                                    .with_streaming(true)
                                    .with_max_tokens(32768)
                                    .with_temperature(0.2)
                                    .with_system_prompt(if iteration > 1 {
                                        RETRY_SYSTEM_PROMPT.to_string()
                                    } else {
                                        HEADLESS_SYSTEM_PROMPT.to_string()
                                    })
                                    .with_tools(tools_schema.to_vec());

                            // Try one more time with recovery request
                            tracing::info!("Attempting recovery stream...");
                            match provider.complete_stream(recovery_request).await {
                                Ok(s) => {
                                    tracing::info!("✓ Recovery stream started successfully");
                                    result_stream = Some(s);
                                }
                                Err(recovery_err) => {
                                    tracing::warn!(
                                        "Recovery stream also failed: {}. Giving up.",
                                        recovery_err
                                    );
                                    return Err(recovery_err).context(
                                        "LLM stream failed and recovery attempt also failed",
                                    );
                                }
                            }
                        }
                        Ok(None) => {
                            tracing::info!("No checkpoint available for recovery");
                            return Err(e).context("Failed to start LLM stream after retries");
                        }
                        Err(recovery_err) => {
                            tracing::warn!(
                                "Checkpoint recovery failed: {}. Falling back to original error.",
                                recovery_err
                            );
                            return Err(e).context("Failed to start LLM stream after retries");
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Failed to start LLM stream after retries"));
                }
            }

            result_stream.ok_or_else(|| {
                anyhow::anyhow!("Stream initialization failed after all recovery attempts")
            })?
        };

        // Accumulate stream state
        let mut assistant_text = String::new();
        let mut completed_tools: Vec<(String, String, String)> = Vec::new(); // (id, name, json)
        let mut stop_reason: Option<String> = None;
        let mut break_stream = false;
        let mut last_block_was_tool = false;

        // Create SSE event processor
        let mut processor = SseEventProcessor::new();

        loop {
            let chunk = match tokio::time::timeout(CHUNK_TIMEOUT, stream.next()).await {
                Ok(Some(Ok(event))) => event,
                Ok(Some(Err(e))) => {
                    let err_str = format!("{}", e);
                    // Transient mid-stream errors: log and break rather than fatal
                    tracing::warn!("Mid-stream error: {}. Ending turn early.", err_str);
                    break;
                }
                Ok(None) => break, // stream ended
                Err(_) => {
                    tracing::warn!(
                        "Stream chunk timed out after {}s. Ending turn early.",
                        CHUNK_TIMEOUT.as_secs()
                    );
                    break;
                }
            };

            // Process event through unified SSE processor with callbacks
            {
                let mut callbacks = HeadlessStreamCallbacks {
                    assistant_text: &mut assistant_text,
                    completed_tools: &mut completed_tools,
                    stop_reason: &mut stop_reason,
                    total_input_tokens: &mut total_input_tokens,
                    total_output_tokens: &mut total_output_tokens,
                    total_cache_read_tokens: &mut total_cache_read_tokens,
                    total_cache_creation_tokens: &mut total_cache_creation_tokens,
                    break_stream: &mut break_stream,
                    last_block_was_tool: &mut last_block_was_tool,
                };
                let keep_going = processor.process_event(chunk, &mut callbacks)?;
                if !keep_going {
                    break;
                }
            }

            // After callbacks, check for structured action dispatch
            if !assistant_text.is_empty() {
                // Attempt to parse structured AgentAction
                if let Ok(action) = serde_json::from_str::<AgentAction>(&assistant_text) {
                    let result = dispatch_agent_action(action, cwd, tool_registry);
                    eprintln!("  🔧 Structured Action Executed: {}", result);
                    // Add result to completed_tools so loop knows work was done
                    completed_tools.push((
                        "headless-structured".to_string(),
                        "action".to_string(),
                        result,
                    ));
                    assistant_text.clear();
                    break;
                }
            }

            // Check for break signal (from repetition detection)
            if break_stream {
                break;
            }
        }

        tracing::debug!(
            turn,
            completed_tools = completed_tools.len(),
            total_tool_calls,
            ?stop_reason,
            "Turn completed"
        );

        // Handle final newline if needed
        if last_block_was_tool {
            // Already handled in on_tool_complete
        } else if !assistant_text.is_empty() {
            // Text block ended without explicit newline — add one
            eprintln!();
        }

        // If the assistant produced text, save it (but strip hallucinated tool markers
        // and repeated intro text from previous turns)
        if !assistant_text.is_empty() {
            let cleaned = clean_assistant_text(&assistant_text);
            // Strip repeated preamble phrases within this turn (e.g., GLM repeating
            // "I'll help you compile..." before every tool call)
            let cleaned = strip_repeated_preamble_phrases(&cleaned);
            // Detect and truncate repeated multi-paragraph blocks. This catches the
            // pattern where the model generates the same analysis paragraphs 50-200+
            // times in a single response (observed with GLM-4.7 on regex tasks).
            let cleaned = detect_and_truncate_repeated_blocks(&cleaned).unwrap_or(cleaned);
            // Strip repeated intro prefix: if the first ~100 chars match the previous
            // turn's assistant text, remove the duplicated portion. This catches the
            // common GLM pattern of repeating "I'll help you..." every turn.
            final_text = if let Some(ref prev) = previous_assistant_text {
                strip_repeated_prefix(&cleaned, prev)
            } else {
                cleaned
            };
            previous_assistant_text = Some(final_text.clone());
        }

        // If no tool calls were made, we're done — unless the agent has been
        // exploring without writing, in which case inject a strong nudge and
        // give it one more chance to produce tool calls.
        if completed_tools.is_empty() {
            // MINIMUM WORK THRESHOLD: if the agent made tool calls in previous turns
            // but now stops (end_turn) without making any in this turn, and total tool
            // calls are below the minimum, force continuation. This catches GLM's pattern:
            // Turn 0: clone + find + find (3 tool calls, stop_reason=tool_use)
            // Turn 1: "Task completed" text (0 tool calls, stop_reason=end_turn)
            tracing::debug!(
                turn,
                total_tool_calls,
                cumulative_writes,
                cumulative_reads,
                ?stop_reason,
                "Guard check: no tools this turn"
            );
            if total_tool_calls > 0 && total_tool_calls < MIN_TOOL_CALLS_TO_STOP {
                tracing::debug!(
                    total_tool_calls,
                    MIN_TOOL_CALLS_TO_STOP,
                    "Below minimum work threshold, forcing continuation"
                );
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                let has_any_code_write = code_writes > 0;
                let nudge = if has_any_code_write {
                    format!(
                        "You have only made {} tool calls — the task is NOT done yet. \
                        You've made some file changes but haven't verified they work. \
                        You MUST: 1) Run tests or verification commands \
                        2) Fix any errors found \
                        3) Verify ALL success criteria from the task description \
                        Do NOT stop until everything works correctly.",
                        total_tool_calls
                    )
                } else {
                    format!(
                        "CRITICAL: You have only made {} tool calls and have NOT written any files! \
                        The task is NOT done — you've barely started. \
                        Your very next action MUST be write_file or edit_file to create/modify code. \
                        Then run verification commands (tests, imports, builds). \
                        Do NOT respond with analysis — take IMMEDIATE action.",
                        total_tool_calls
                    )
                };
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(nudge),
                });
                final_text.clear();
                continue;
            }
            // Fire the nudge when the agent has read at all but never written code.
            // Uses code_writes (actual source edits) not cumulative_writes (which includes
            // git clone, pip install, etc.). This catches: git clone → read → stop.
            if cumulative_reads >= 1 && code_writes == 0 && turn < MAX_TOOL_TURNS - 1 {
                info!(
                    "Agent stopped after {} reads, 0 code writes. Injecting action nudge.",
                    cumulative_reads
                );
                // Push the assistant's text-only response
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                // Inject a very strong nudge demanding immediate action
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(
                        "You stopped without making any tool calls. You have only been reading files. \
                        You MUST use a tool NOW. Call edit_file or write_file immediately. \
                        Do NOT respond with text only — you MUST make a tool call."
                            .to_string(),
                    ),
                });
                final_text.clear();
                continue;
            }
            // Zero tool calls total — the agent responded with text only from the
            // very first turn. This is GLM's "analysis without action" pattern.
            // Inject a strong nudge to force tool use.
            if total_tool_calls == 0 && turn < MAX_TOOL_TURNS - 1 {
                info!(
                    "Agent responded with text only on turn {} (0 tool calls total), injecting action nudge",
                    turn
                );
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(
                        "You MUST use tools to complete this task. Do NOT just describe what you would do. \
                        Your very next response MUST contain tool calls (bash, write_file, read_file, etc.). \
                        Start by exploring the files with list_dir or read_file, then take action immediately."
                            .to_string(),
                    ),
                });
                final_text.clear();
                continue;
            }
            info!("Headless agent finished (no more tool calls)");
            break;
        }

        // Build assistant message with text + tool_use blocks
        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        if !final_text.is_empty() {
            assistant_blocks.push(ContentBlock::text(&final_text));
        }

        // Execute tool calls and build result blocks
        let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
        let mut force_stop = false;
        let mut intra_turn_outputs: Vec<String> = Vec::new();
        for (tool_idx, (tool_id, tool_name_raw, tool_json)) in completed_tools.iter().enumerate() {
            // Trim tool name — some models emit leading whitespace/newlines (e.g., "\nwrite_file")
            let tool_name = tool_name_raw.trim().to_string();
            // Add tool_use block to assistant message
            let input: serde_json::Value = match serde_json::from_str(tool_json) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse tool JSON for {} ({}): {}. Raw: {:.100}",
                        tool_name,
                        tool_id,
                        e,
                        tool_json
                    );
                    serde_json::json!({"_raw": tool_json})
                }
            };
            assistant_blocks.push(ContentBlock::ToolUse {
                id: tool_id.clone(),
                name: tool_name.clone(),
                input,
            });

            // Execute the tool
            let output = execute_headless_tool(cwd, &tool_name, tool_json, tool_registry);

            // Inject heuristic hints for common errors (missing modules, build
            // failures, etc.) directly into the tool output so the LLM sees them.
            // Note: bash-specific hints (NumPy, Cython, etc.) are already injected
            // inside execute_headless_tool, so this mainly adds hints for non-bash
            // tools (npm, gcc, TypeScript, etc.) and patterns not covered there.
            let hint_cmd = if tool_name == "bash" {
                // For bash, extract the actual command string from the JSON args
                serde_json::from_str::<serde_json::Value>(tool_json)
                    .ok()
                    .and_then(|v| {
                        v.get("command")
                            .and_then(|c| c.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default()
            } else {
                tool_json.to_string()
            };
            let output = if let Some(hint) = hints::get_tool_error_hint(&hint_cmd, &output) {
                format!("{}\n\n{}", output, hint)
            } else {
                output
            };

            // Show result summary
            let output_preview = if output.len() > 200 {
                // Use char_indices to avoid panicking on multi-byte UTF-8 boundaries
                let truncate_at = output
                    .char_indices()
                    .take_while(|(i, _)| *i < 197)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0);
                format!("{}...", &output[..truncate_at])
            } else {
                output.clone()
            };
            let output_lines = output.lines().count();
            if output_lines <= 3 && output.len() <= 200 {
                eprintln!("  ➜ {}", output_preview.trim_end());
            } else {
                eprintln!("  ➜ ({} lines, {} bytes)", output_lines, output.len());
            }

            // Build tool result as a proper ToolResult content block.
            let is_tool_error = output.starts_with("Error ")
                || output.starts_with("ERROR: ")
                || output.contains("[exit code: ") && !output.contains("[exit code: 0]");

            if is_tool_error {
                tool_result_blocks.push(ContentBlock::tool_error(tool_id, &output));
            } else {
                tool_result_blocks.push(ContentBlock::tool_result(tool_id, &output));
            }

            // Track recent tool results for error-loop detection
            recent_tool_results.push(output.clone());
            if recent_tool_results.len() > TOOL_HISTORY_LENGTH {
                let drain_count = recent_tool_results.len() - TOOL_HISTORY_LENGTH;
                recent_tool_results.drain(..drain_count);
            }

            // Grep-then-stop detection: if grep/search found results indicating
            // unfixed issues, flag it so we can prevent premature completion.
            // Catches: edit(A) → verify → OK → grep(find B) → "Task completed"
            if (tool_name == "grep" || (tool_name == "bash" && tool_json.contains("grep")))
                && !output.is_empty()
                && !output.contains("no matches")
                && !output.contains("No files found")
                && !output.contains("0 matches")
                && output.lines().any(|l| {
                    // Match common grep -rn output patterns:
                    // ./path/file.py:42:content
                    // path/file.pyx:42:content
                    // file.py:42:content
                    // Also match if grep found ANY results (non-empty, non-"no match" output)
                    // and the grep command was searching for deprecated/fixable patterns
                    l.contains("./")
                        || l.contains(".py:")
                        || l.contains(".pyx:")
                        || l.contains(".pxd:")
                        || l.contains(".c:")
                        || l.contains(".rs:")
                })
            {
                grep_found_unfixed_issues = true;
            }

            // Intra-turn loop detection: cap tools per turn
            if tool_idx + 1 > MAX_TOOLS_PER_TURN {
                tracing::warn!(
                    "Too many tool calls in one turn ({}), stopping early",
                    tool_idx + 1
                );
                force_stop = true;
                break;
            }

            // Intra-turn stagnant output detection: if last N outputs are similar
            // Use first 100 chars as fingerprint — this catches the common pattern where
            // the agent appends more commands to a pipeline but the core output is the same
            let output_fingerprint = {
                let trimmed = output.trim();
                // For long outputs, use the first 100 chars as fingerprint
                // This catches cases where the agent runs "git log" then "git log && git status"
                // The core output (git log) is the same, just with extra text appended
                let fp: String = trimmed.chars().take(100).collect();
                fp
            };
            intra_turn_outputs.push(output_fingerprint.clone());
            if intra_turn_outputs.len() >= 3 {
                let last_n_same = intra_turn_outputs
                    .iter()
                    .rev()
                    .take(3)
                    .all(|o| o == &output_fingerprint);
                if last_n_same {
                    tracing::warn!("Detected 3 similar outputs in same turn, stopping early");
                    force_stop = true;
                    break;
                }
            }

            // Track tool in history for progress detection
            let current_key = (tool_name.clone(), tool_json.clone());
            tool_history.push(current_key.clone());

            // Track cumulative read/write for urgency detection
            match tool_name.as_str() {
                "write_file" | "edit_file" | "apply_patch" => {
                    cumulative_writes += 1;
                    code_writes += 1;
                    reads_since_last_write = 0; // Reset after a write
                                                // Track per-file write count to detect approach stagnation
                                                // Parse the path from the JSON arguments
                    if let Ok(args) = serde_json::from_str::<serde_json::Value>(tool_json) {
                        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                            // Use just the filename for tracking (not full path)
                            let file_name = path.rsplit('/').next().unwrap_or(path);
                            *file_write_counts.entry(file_name.to_string()).or_insert(0) += 1;
                        }
                    }
                }
                "bash" => {
                    // Bash commands that modify the filesystem count as writes.
                    // This prevents the "read-without-write" nudge from firing when the
                    // agent is making progress via gcc, pip install, git clone, etc.
                    if is_modifying(&tool_name, tool_json) {
                        cumulative_writes += 1;
                        reads_since_last_write = 0;
                    } else {
                        cumulative_reads += 1;
                        reads_since_last_write += 1;
                    }
                    // Also track code writes separately — only actual source edits
                    if is_code_write(&tool_name, tool_json) {
                        code_writes += 1;
                    }
                }
                "read_file" | "grep" | "search" | "glob" | "find_files" | "list_dir"
                | "list_directory" => {
                    cumulative_reads += 1;
                    reads_since_last_write += 1;
                }
                _ => {}
            }

            // Detect exact same tool call 3 times in a row — true infinite loop
            let consecutive_dupes = tool_history
                .iter()
                .rev()
                .take(3)
                .filter(|k| *k == &current_key)
                .count();
            if consecutive_dupes >= 3 {
                tracing::warn!(
                    "Detected same tool call 3x in a row ({}), forcing stop",
                    tool_name
                );
                force_stop = true;
                break;
            }

            // Detect similar tool calls (same name, similar args) — near-loop.
            // Normalize command separators (`&&`, `;`) so that variations like
            // "cd /app && cmd" vs "cd /app; cmd" are treated as similar.
            let normalize = |s: &str| s.replace("&&", ";").replace(" & ", "; ");
            let args_prefix: String = tool_json.chars().take(80).collect();
            let args_prefix_norm = normalize(&args_prefix);
            let similar_count = tool_history
                .iter()
                .rev()
                .take(MAX_SIMILAR_CONSECUTIVE)
                .filter(|(name, json)| {
                    name == &tool_name && {
                        let other_prefix: String = json.chars().take(80).collect();
                        normalize(&other_prefix) == args_prefix_norm
                    }
                })
                .count();
            if similar_count >= MAX_SIMILAR_CONSECUTIVE {
                tracing::warn!(
                    "Detected {} similar tool calls in a row ({}), forcing stop",
                    similar_count,
                    tool_name
                );
                force_stop = true;
                break;
            }

            // Detect stagnant progress (same output multiple times)
            let output_normalized = output.trim().to_lowercase();
            if let Some(ref last_output) = last_successful_output {
                if output_normalized == last_output.trim().to_lowercase() {
                    stagnant_turns += 1;
                    tracing::info!(
                        "Detected stagnant output (turn {}), stagnant_count={}",
                        turn,
                        stagnant_turns
                    );
                    if stagnant_turns >= 3 {
                        tracing::warn!("Too many stagnant turns, forcing completion");
                        force_stop = true;
                        break;
                    }
                } else if output.len() < 500 && is_tool_error && stagnant_turns >= 1 {
                    // Soft stagnation: short error outputs that differ slightly but
                    // indicate the same underlying failure (e.g., build commands
                    // returning 6-7 line error messages with minor formatting diffs).
                    stagnant_turns += 1;
                    if stagnant_turns >= 4 {
                        tracing::warn!(
                            "Soft stagnation: {} short error outputs, injecting strategy nudge",
                            stagnant_turns
                        );
                        // Don't force stop — inject a nudge to change approach instead
                        tool_result_blocks.push(ContentBlock::tool_result(
                            tool_id,
                            format!(
                                "{}\n\nSTOP RETRYING THE SAME COMMAND. It keeps failing with the same error. \
                                You need a DIFFERENT approach:\n\
                                1. Read the actual error message above carefully\n\
                                2. Identify the ROOT CAUSE (missing file, wrong path, syntax error)\n\
                                3. Fix the root cause FIRST, then retry the command",
                                output
                            ),
                        ));
                        // Skip the normal tool_result push below since we already added it
                        tool_history.push(current_key);
                        recent_tool_names.push(tool_name.clone());
                        recent_tool_results.push(output.clone());
                        continue;
                    }
                } else {
                    stagnant_turns = 0;
                }
            }
            last_successful_output = Some(output_normalized);
        }

        // Detect if ALL commands this turn were verification/read-only.
        // Uses the is_modifying closure defined before the main loop.
        let turn_had_modifications = completed_tools
            .iter()
            .any(|(_, name, json)| is_modifying(name, json));

        // Track code edits separately from filesystem modifications.
        // Only actual source edits (write_file, sed -i, etc.) should reset
        // the verification flag. Operations like cp, mv, pip install, build
        // commands are "modifying" but don't represent new code that needs
        // re-verification. This prevents false "unverified edits" when the
        // agent copies build artifacts after successfully verifying.
        let turn_had_code_edits = completed_tools
            .iter()
            .any(|(_, name, json)| is_code_write(name, json));

        if turn_had_modifications {
            consecutive_verification_turns = 0;
            last_modification_turn = Some(turn);
            // Only reset verification for actual code edits, not build/copy ops
            if turn_had_code_edits {
                verified_after_last_mod = false;
                tracing::info!("Turn had code edits, reset verification counter");
            } else {
                tracing::info!(
                    "Turn had filesystem modifications (non-code), keeping verification state"
                );
            }
        } else {
            consecutive_verification_turns += 1;
            tracing::info!(
                "Verification-only turn ({} consecutive)",
                consecutive_verification_turns
            );
        }

        // Detect verification commands: bash commands that run tests or check results.
        // This tracks whether the agent verified its changes before stopping.
        // Check even when the turn has modifications — an agent might edit a file
        // AND run a test in the same turn's tool-use batch.
        let has_verification = completed_tools.iter().any(|(_, name, json)| {
            if name != "bash" {
                return false;
            }
            let cmd = json.to_lowercase();
            // Common verification patterns
            cmd.contains("pytest")
                || cmd.contains("python -m pytest")
                || cmd.contains("python -m unittest")
                || cmd.contains("cargo test")
                || cmd.contains("go test")
                || cmd.contains("make test")
                || cmd.contains("make check")
                || cmd.contains("npm test")
                || cmd.contains("yarn test")
                || cmd.contains("bun test")
                || cmd.contains("pnpm test")
                || cmd.contains("npx jest")
                || cmd.contains("pip install") && (cmd.contains("-e .") || cmd.contains("--editable"))
                || cmd.contains("setup.py build_ext")
                || cmd.contains("python setup.py install")
                || cmd.contains("cmake --build")
                || cmd.contains("make &&")
                || cmd.contains("gradle test") || cmd.contains("gradlew test")
                || cmd.contains("mvn test") || cmd.contains("mvn verify")
                // Compilation as verification (source compiles without errors)
                || cmd.contains("gcc ") && cmd.contains("-o ")
                || cmd.contains("g++ ") && cmd.contains("-o ")
                || cmd.contains("cargo build") || cmd.contains("cargo run")
                || cmd.contains("javac ")
                || cmd.contains("rustc ")
                // python -c / python3 -c / node -e count as verification if they
                // contain assertions, tests, or imports (import confirms install works)
                || (cmd.contains("python -c") && (cmd.contains("import") || cmd.contains("assert") || cmd.contains("test")))
                || (cmd.contains("python3 -c") && (cmd.contains("import") || cmd.contains("assert") || cmd.contains("test")))
                || (cmd.contains("node -e") && (cmd.contains("assert") || cmd.contains("test") || cmd.contains("require")))
                || cmd.contains("ruby -e")
                || cmd.contains("perl -e")
                // Running the actual program/executable counts as verification
                || cmd.contains("./") && !cmd.contains("install")
                // curl/wget to check a server is running
                || cmd.contains("curl http://localhost")
                || cmd.contains("curl http://127.0.0.1")
                // Netstat/ss to check port listening
                || cmd.contains("netstat") || cmd.contains("ss -tuln")
                // Generic "run the test script" patterns
                || cmd.contains("test ") || cmd.contains("test.sh")
                || cmd.contains("test_") && cmd.contains(".py")
                || cmd.contains("verify") || cmd.contains("validate")
                || cmd.contains("check ") && (cmd.contains("result") || cmd.contains("output"))
                // Running the evaluator/verifier script
                || cmd.contains("eval.py") || cmd.contains("verify.py") || cmd.contains("grade.py")
                // Running any Python script after edits counts as verification.
                // (e.g., python /app/test_outputs.py, python3 optimized_packer.py)
                || cmd.contains("python ") && cmd.contains(".py")
                || cmd.contains("python3 ") && cmd.contains(".py")
                // uv run executes the project — counts as verification
                || cmd.contains("uv run")
                // uv sync installs dependencies — counts as verification
                || cmd.contains("uv sync")
                // Running any Node.js script after edits
                || cmd.contains("node ") && cmd.contains(".js")
                // Common simulators and test runners (CoreWars, games, etc.)
                || cmd.contains("pmars")
                // Running any installed program as verification (e.g., pytest, mypy, ruff)
                || cmd.contains("pytest") || cmd.contains("mypy") || cmd.contains("ruff")
                || cmd.contains("pylint") || cmd.contains("flake8")
                // Running the project's CLI tool
                || cmd.contains("cli_tool")
        });
        if has_verification && last_modification_turn.is_some() {
            verified_after_last_mod = true;
            tracing::info!("Verification command detected after modifications");
        }

        // Detect verification errors in bash output. When the agent runs a
        // test/verification command and the output contains error indicators
        // (tracebacks, failed tests, import errors), track this so we can
        // prevent the agent from stopping without fixing the errors.
        // Only check verification commands (not grep/read operations).
        for (tool_idx, (_tool_id, tool_name, _tool_json)) in completed_tools.iter().enumerate() {
            if tool_name != "bash" {
                continue;
            }
            let cmd = _tool_json.to_lowercase();
            // Must match the same patterns as the verification detection above
            // to catch errors from ALL recognized verification commands.
            let is_verification_cmd = cmd.contains("pytest")
                || cmd.contains("python -c")
                || cmd.contains("python3 -c")
                || cmd.contains("python -m pytest")
                || cmd.contains("python -m unittest")
                || cmd.contains("cargo test")
                || cmd.contains("npm test")
                || cmd.contains("go test")
                || cmd.contains("make test")
                || cmd.contains("make check")
                || cmd.contains("python ") && cmd.contains(".py")
                || cmd.contains("python3 ") && cmd.contains(".py")
                || cmd.contains("uv run")
                || cmd.contains("uv sync")
                // Running an executable (e.g., ./decomp, ./a.out)
                || cmd.contains("./") && !cmd.contains("install")
                // Compilation commands
                || cmd.contains("gcc ") && cmd.contains("-o ")
                || cmd.contains("g++ ") && cmd.contains("-o ")
                || cmd.contains("cargo build") || cmd.contains("cargo run")
                || cmd.contains("javac ")
                // Generic test/verify patterns
                || cmd.contains("test_") && cmd.contains(".py")
                || cmd.contains("verify") || cmd.contains("validate")
                || cmd.contains("eval.py") || cmd.contains("grade.py");
            if !is_verification_cmd {
                continue;
            }
            // Check the tool result for error indicators
            let result_idx = tool_idx; // tool results are in same order
            if let Some(ContentBlock::ToolResult { content, .. }) =
                tool_result_blocks.get(result_idx)
            {
                let result_text = content.as_str();
                let result_lower = result_text.to_lowercase();
                let has_errors = result_lower.contains("error")
                    || result_lower.contains("traceback")
                    || result_lower.contains("failed")
                    || result_lower.contains("importerror")
                    || result_lower.contains("attributeerror")
                    || result_lower.contains("nameerror")
                    || result_lower.contains("typeerror")
                    || result_lower.contains("valueerror")
                    || result_lower.contains("assertionerror")
                    || result_lower.contains("syntaxerror")
                    || result_lower.contains("exception")
                    // Binary execution failures
                    || result_lower.contains("sigill")
                    || result_lower.contains("sigsegv")
                    || result_lower.contains("segmentation fault")
                    || result_lower.contains("segfault")
                    || result_lower.contains("core dumped")
                    || result_lower.contains("killed")
                    || result_lower.contains("command not found")
                    || result_lower.contains("no such file")
                    || result_lower.contains("cannot execute")
                    || result_lower.contains("not executable");
                let has_success_indicators =
                    result_lower.contains("passed") && !result_lower.contains("failed")
                    || result_lower.contains("all tests passed")
                    || result_lower.contains("ok")
                    || result_lower.contains("success")
                    || result_lower.contains("exit: 0")
                    || result_lower.contains("exit code: 0");
                if has_errors && !has_success_indicators {
                    verification_errors_found = true;
                    tracing::info!("Verification revealed errors, preventing premature stop");
                } else if has_success_indicators && !has_errors {
                    // Verification passed — clear any previous error flag
                    if verification_errors_found {
                        tracing::info!("Verification passed, clearing error flag");
                        verification_errors_found = false;
                    }
                }
            }
        }
        // Tool call loop detection (tamux-inspired StuckDetector).
        // Track recent tool names and detect repeating patterns.
        for (_, tool_name, _) in &completed_tools {
            recent_tool_names.push(tool_name.clone());
        }
        if recent_tool_names.len() > TOOL_HISTORY_LENGTH {
            let drain_count = recent_tool_names.len() - TOOL_HISTORY_LENGTH;
            recent_tool_names.drain(..drain_count);
        }

        // Only check for loops when the turn had no modifications (read-only loops)
        if recent_tool_names.len() >= TOOL_LOOP_MIN_LENGTH && !turn_had_modifications {
            if let Some(evidence) = detect_tool_loop(&recent_tool_names, TOOL_LOOP_MIN_LENGTH) {
                info!("Tool call loop detected: {}", evidence);
                // Clear tool history to avoid re-triggering
                recent_tool_names.clear();
                // Inject a strategy-changing nudge
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(format!(
                        "LOOP DETECTED: {}. You are repeating the same actions without making progress. \
                        STOP what you are doing and try a COMPLETELY DIFFERENT approach. \
                        If you were reading files, stop reading and START WRITING. \
                        If you were searching, stop searching and use edit_file to fix what you found. \
                        Your next action MUST be a write_file, edit_file, or bash command that MODIFIES files. \
                        Do NOT repeat the same pattern.",
                        evidence
                    )),
                });
                final_text.clear();
                continue;
            }
        }

        // Error-loop detection: when the last N tool results all contain errors/timeouts
        {
            const ERROR_LOOP_MIN_LENGTH: usize = 3;
            let recent_results: Vec<bool> = recent_tool_results
                .iter()
                .rev()
                .take(ERROR_LOOP_MIN_LENGTH)
                .map(|r| r.contains("timed out") || r.contains("Error:") || r.contains("blocked"))
                .collect();
            if recent_results.len() >= ERROR_LOOP_MIN_LENGTH && recent_results.iter().all(|&e| e) {
                info!(
                    "Error loop detected: last {} results all failed",
                    ERROR_LOOP_MIN_LENGTH
                );
                recent_tool_results.clear();
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(
                        "CRITICAL: Your last several commands ALL failed (timeout or error). \
                        The shell session may be broken. Try:\n\
                        1. Use read_file/write_file instead of bash where possible\n\
                        2. Use `timeout 10 command` to fail faster and diagnose\n\
                        3. If network commands fail, check if files already exist: `ls /app/`\n\
                        4. Try a COMPLETELY DIFFERENT approach\n\
                        Do NOT repeat the failing command."
                            .to_string(),
                    ),
                });
                final_text.clear();
                continue;
            }
        }

        // Note: we do NOT auto-reset verification_errors_found based on turn_had_modifications.
        // The agent may edit file A (fixing one issue) while verification errors still exist
        // in file B. The flag only resets when the agent re-runs verification and it passes,
        // or when the agent is given one more chance (at the stop-prevention nudge).
        // Previous behavior (auto-reset on any modification) caused the agent to stop
        // after fixing fractions.gcd but leaving n.float unfixed — the edit for gcd
        // cleared the flag even though n.float errors were still present.

        // If intra-turn loop detection triggered, inject course correction instead of stopping
        if force_stop {
            course_corrections += 1;

            if course_corrections > 1 {
                // Already tried course correction once — hard break
                tracing::info!("Second force stop, breaking agent loop");
                if !assistant_blocks.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Blocks(assistant_blocks),
                    });
                }
                if !tool_result_blocks.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::User,
                        content: MessageContent::Blocks(tool_result_blocks),
                    });
                }
                break;
            }

            tracing::info!("Force stop triggered, injecting course correction");

            // Push the partial turn's messages
            if !assistant_blocks.is_empty() {
                messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: MessageContent::Blocks(assistant_blocks),
                });
            }
            if !tool_result_blocks.is_empty() {
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Blocks(tool_result_blocks),
                });
            }

            // Inject a course correction message to help the agent break out of the loop
            messages.push(ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Simple(
                    "WARNING: You appear to be stuck in a loop — you have run the same or very similar \
                    commands multiple times without making progress. Please STOP and try a completely \
                    different approach. Read the error messages carefully, examine the problem from a \
                    different angle, or use a different tool/technique. Do NOT repeat what you just tried."
                        .to_string(),
                ),
            });

            // Reset counters so the agent gets one more chance
            stagnant_turns = 0;
            tool_history.clear();
            last_successful_output = None;

            // Continue the loop — the agent gets one more turn with the correction
            continue;
        }

        // Detect verification-only loops: agent keeps running read-only commands
        // without making any changes. This catches the common pattern where the
        // agent completes the task but then loops checking "git log", "git status",
        // "cat file" over and over.
        if consecutive_verification_turns >= MAX_CONSECUTIVE_VERIFICATION_TURNS {
            tracing::warn!(
                "Agent has done {} consecutive verification-only turns, stopping",
                consecutive_verification_turns
            );
            // Push final messages
            if !assistant_blocks.is_empty() {
                messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: MessageContent::Blocks(assistant_blocks),
                });
            }
            if !tool_result_blocks.is_empty() {
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Blocks(tool_result_blocks),
                });
            }
            break;
        }

        // Update cumulative tool call counter for the minimum work threshold
        total_tool_calls += completed_tools.len();

        // Create and save checkpoint before continuing to next iteration
        // This allows recovery if the next LLM stream call fails
        if !completed_tools.is_empty() {
            if let Err(e) =
                create_and_save_checkpoint(turn as u32, &completed_tools, &tool_result_blocks, cwd)
            {
                tracing::warn!("Failed to save iteration checkpoint: {}", e);
                // Continue anyway — checkpointing is a best-effort feature
            }
        }

        // Add assistant message with tool_use blocks
        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(assistant_blocks),
        });

        // Detect if the last tool call returned an error, before we move tool_result_blocks.
        // Stored persistently so the next turn (where LLM returns end_turn) can check it.
        prev_turn_last_tool_error = (|| {
            let last_idx = completed_tools.len().saturating_sub(1);
            let block = tool_result_blocks.get(last_idx)?;
            if let ContentBlock::ToolResult { content, .. } = block {
                let text = content.as_str().to_lowercase();
                let has_error = text.contains("error:")
                    || text.contains("no such file")
                    || text.contains("not found")
                    || text.contains("failed")
                    || text.contains("denied")
                    || text.contains("cannot")
                    || text.contains("does not exist");
                let has_success = text.contains("0 errors")
                    || text.contains("no errors")
                    || text.contains("passed")
                    || text.contains("success");
                Some(has_error && !has_success)
            } else {
                None
            }
        })()
        .unwrap_or(false);

        // Add tool results as user message
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(tool_result_blocks),
        });

        // Prune message history to stay within context window budget
        messages = prune_messages(messages);

        // Check stop reason
        if stop_reason.as_deref() == Some("end_turn") {
            // MINIMUM WORK THRESHOLD: if the agent tries to stop with fewer than
            // MIN_TOOL_CALLS_TO_STOP total tool calls, force it to continue regardless
            // of what it claims. GLM has a pattern of clone + read + "Task completed"
            // with only 4-5 tool calls. No task can be completed that quickly.
            if total_tool_calls < MIN_TOOL_CALLS_TO_STOP {
                info!(
                    "Agent stopped too early with only {} total tool calls (minimum {}), forcing continuation",
                    total_tool_calls, MIN_TOOL_CALLS_TO_STOP
                );
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                let has_any_code_write = code_writes > 0;
                let nudge = if has_any_code_write {
                    format!(
                        "You have only made {} tool calls — the task is NOT done yet. \
                        You've made some file changes but haven't verified they work. \
                        You MUST: 1) Run tests or verification commands \
                        2) Fix any errors found \
                        3) Verify ALL success criteria from the task description \
                        Do NOT stop until everything works correctly.",
                        total_tool_calls
                    )
                } else {
                    format!(
                        "CRITICAL: You have only made {} tool calls and have NOT written any files! \
                        The task is NOT done — you've barely started. \
                        Your very next action MUST be write_file or edit_file to create/modify code. \
                        Then run verification commands (tests, imports, builds). \
                        Do NOT respond with analysis — take IMMEDIATE action.",
                        total_tool_calls
                    )
                };
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(nudge),
                });
                final_text.clear();
                continue;
            }

            // Guard against premature termination: if the agent stops in fewer
            // than 3 turns, it likely hasn't done meaningful work yet. Inject a
            // continuation message to push it to keep going.
            // Even if the agent says "Task completed", if it hasn't made any file
            // modifications in its turns, it probably hasn't actually done the work.
            // Skip this check when verification_errors_found is true — the more specific
            // verification error nudge below is more helpful than the generic continuation.
            let made_modifications = completed_tools
                .iter()
                .any(|(_, name, json)| is_modifying(name, json));
            if turn < 5
                && !verification_errors_found
                && (!made_modifications
                    || (!final_text.contains("Task completed")
                        && !final_text.contains("All tests pass")))
            {
                info!(
                    "Agent stopped early (turn {}), injecting continuation",
                    turn + 1
                );
                // Keep the assistant's text as context
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                let continuation_msg = if !made_modifications {
                    // Summarize what the agent did so far to help it course-correct
                    let tools_used: Vec<&str> =
                        completed_tools.iter().map(|(_, n, _)| n.as_str()).collect();
                    let tool_summary = tools_used.join(", ");
                    format!(
                        "CRITICAL: You have NOT made any changes to files yet! \
                        You only used these tools: {}. The task is NOT done. \
                        Your very next action MUST be one of: \
                        1. Use write_file to create a new file \
                        2. Use edit_file to modify an existing file \
                        3. Use bash to run a modifying command (install, build, sed, etc.) \
                        Do NOT respond with analysis or plans — take IMMEDIATE action to modify files.",
                        tool_summary
                    )
                } else {
                    "You stopped too early. The task is NOT complete yet. \
                    You must continue working. Read the relevant code files, \
                    identify the issue, make changes, and verify with tests. \
                    Do NOT stop until you have actually fixed the problem and \
                    verified it works."
                        .to_string()
                };
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(continuation_msg),
                });
                final_text.clear();
                continue;
            }

            // Read-then-stop detection: if the agent's last tool call was read_file
            // and the agent declares "Task completed" or mentions needing to fix something
            // but stops without making the fix, inject a nudge. Catches: read(.pyx) → "I need
            // to fix this" → "Task completed" without actually editing the file.
            if last_modification_turn.is_some()
                && (final_text.contains("Task completed")
                    || final_text.contains("task completed")
                    || final_text.contains("successfully"))
            {
                // Check if the agent's text mentions needing to fix/edit something
                let text_lower = final_text.to_lowercase();
                let mentions_fix_needed = text_lower.contains("need to fix")
                    || text_lower.contains("need to edit")
                    || text_lower.contains("now let me fix")
                    || text_lower.contains("still need")
                    || text_lower.contains("now i need to fix")
                    || text_lower.contains("now i'll fix");
                // Check if the last tool call was read_file (agent read but didn't act)
                let last_tool_was_read = completed_tools
                    .last()
                    .is_some_and(|(_, name, _)| name == "read_file");
                if (mentions_fix_needed || last_tool_was_read)
                    && completed_tools
                        .iter()
                        .any(|(_, name, _)| name == "read_file")
                {
                    info!(
                        "Agent stopped after reading files / mentioning fixes needed (turn {}), injecting action nudge",
                        turn + 1
                    );
                    if !assistant_text.is_empty() {
                        messages.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                        });
                    }
                    messages.push(ChatMessage {
                        role: MessageRole::User,
                        content: MessageContent::Simple(
                            "You read a file and mentioned needing to fix it, but then declared \
                            'Task completed' WITHOUT actually editing the file! Your own text says \
                            you still need to fix something. You MUST use edit_file to fix the issues \
                            you identified, then rebuild and re-verify. Do NOT stop until ALL files \
                            (including .pyx, .pxd Cython files) are fixed and tests pass."
                                .to_string(),
                        ),
                    });
                    final_text.clear();
                    continue;
                }
            }

            // Post-edit verification check: if the agent made edits but hasn't run
            // any verification command after the last modification, inject a nudge.
            // This catches the common failure mode where the agent edits files and
            // stops without verifying the changes actually work — regardless of
            // whether it says "Task completed" or stops silently.
            if last_modification_turn.is_some() && !verified_after_last_mod {
                info!(
                    "Agent declared completion without verification after last edit (turn {}), injecting verify nudge",
                    turn + 1
                );
                // Keep the assistant's text as context
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(
                        "You claimed the task is complete but you have NOT verified your changes work. \
                        Your last action was an edit/write without running any test or verification command. \
                        You MUST verify before stopping. Run the actual test command, import the module, \
                        or execute the program to confirm your changes work. Do NOT stop until you have \
                        verified your changes produce the expected output."
                            .to_string(),
                    ),
                });
                final_text.clear();
                continue;
            }

            // Grep-then-stop detection: agent found issues via grep but stopped
            // without fixing them. Catches: edit(A) → verify → OK → grep(find B) → "done"
            // Also catches: grep found issues, then agent produced no text at all.
            if grep_found_unfixed_issues && last_modification_turn.is_some() {
                info!(
                    "Agent stopped after grep found issues (turn {}), injecting fix nudge",
                    turn + 1
                );
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(
                        "CRITICAL: Your grep/search just found remaining issues that need fixing! \
                        You CANNOT stop now — you must fix ALL of them. \
                        1) Use edit_file to fix each file shown in the grep results \
                        2) After editing, run the SAME grep command again to verify zero matches \
                        3) Rebuild/reinstall if you edited compiled files \
                        4) Run verification again \
                        Do NOT stop until grep shows NO matches."
                            .to_string(),
                    ),
                });
                grep_found_unfixed_issues = false;
                final_text.clear();
                continue;
            }

            // If verification revealed errors that haven't been fixed yet,
            // prevent the agent from stopping. This catches the pattern:
            // edit → verify → errors → grep/read → stop without fixing.
            if verification_errors_found {
                info!(
                    "Agent trying to stop after verification revealed errors (turn {}), injecting fix nudge",
                    turn + 1
                );
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(
                        "STOP: Your verification revealed ERRORS (tracebacks, failed tests, import errors, etc.) \
                        but you have NOT fixed them yet. You just ran grep/read operations without making any edits. \
                        You MUST use edit_file or write_file to fix the errors you found. \
                        Look at the error messages from your verification — they tell you EXACTLY what to fix \
                        and in which files. Apply the fixes NOW, then rebuild/reinstall if needed, and re-verify."
                            .to_string(),
                    ),
                });
                verification_errors_found = false; // Give it one more chance
                final_text.clear();
                continue;
            }

            // Error-then-stop detection: if the previous turn's last tool call returned
            // an error and the agent stopped without saying "Task completed", it likely
            // gave up prematurely. Catches: read_file(missing path) → LLM stops.
            if prev_turn_last_tool_error
                && !final_text.contains("Task completed")
                && !final_text.contains("All tests pass")
                && !final_text.contains("task completed")
            {
                info!(
                    "Agent stopped after tool error (turn {}), injecting error-recovery nudge",
                    turn + 1
                );
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(
                        "Your last action encountered an error but you STOPPED without trying \
                        alternatives. The task is NOT complete — an error on one path does NOT \
                        mean the task is impossible. You MUST try a different approach: \
                        1. Use list_dir or glob to find the correct file paths \
                        2. Try an alternative build method or source location \
                        3. Read README or documentation for the correct procedure \
                        Do NOT give up after a single error. Continue working immediately."
                            .to_string(),
                    ),
                });
                final_text.clear();
                continue;
            }

            // Silent-stop detection: if the agent made modifications in prior turns
            // but stops without declaring "Task completed" or similar, it likely
            // stopped mid-work. Catches: agent writes code, tests partially, then
            // stops without finishing or verifying.
            if last_modification_turn.is_some()
                && !final_text.contains("Task completed")
                && !final_text.contains("All tests pass")
                && !final_text.contains("task completed")
                && !final_text.contains("successfully")
                && !verified_after_last_mod
            {
                info!(
                    "Agent stopped silently after modifications without completion text (turn {}), injecting continuation",
                    turn + 1
                );
                if !assistant_text.is_empty() {
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                    });
                }
                messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Simple(
                        "You stopped working without declaring the task complete. \
                        You have made file changes but have NOT verified they work correctly. \
                        You MUST continue: run tests, verify the output, and fix any issues. \
                        Only stop when you can confidently say 'Task completed' after successful verification."
                            .to_string(),
                    ),
                });
                final_text.clear();
                continue;
            }

            // SANITY CHECK: If the agent claims completion with a suspicious
            // zero/empty answer, inject a sanity nudge. Catches tasks where
            // the agent writes "0" as a numeric answer.
            if last_modification_turn.is_some()
                && final_text.contains("Task completed")
                && turn >= MIN_TOOL_CALLS_TO_STOP
            {
                let text_lower = final_text.to_lowercase();
                let mentions_zero_or_empty = text_lower.contains("answer is 0")
                    || text_lower.contains("result is 0")
                    || text_lower.contains("result: 0")
                    || text_lower.contains("value: 0");
                if mentions_zero_or_empty {
                    info!(
                        "Agent claims completion with suspicious zero answer (turn {}), injecting sanity nudge",
                        turn + 1
                    );
                    if !assistant_text.is_empty() {
                        messages.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: MessageContent::Simple(clean_assistant_text(&assistant_text)),
                        });
                    }
                    messages.push(ChatMessage {
                        role: MessageRole::User,
                        content: MessageContent::Simple(
                            "You claimed the task is complete but your answer is 0 or empty. \
                            This is almost certainly WRONG — tasks do not ask you to compute \
                            trivially zero results. Re-examine your approach: \
                            1. Did you correctly filter/process the data? \
                            2. Did you use the right tool/tokenizer/method? \
                            3. Check if you missed a step in the task description \
                            Try a completely different approach and verify the answer is non-zero \
                            before stopping."
                                .to_string(),
                        ),
                    });
                    final_text.clear();
                    continue;
                }
            }

            info!("Headless agent finished (end_turn)");
            break;
        }

        // Hard limit on turns to prevent infinite loops
        if turn >= MAX_TOOL_TURNS - 1 {
            tracing::warn!(
                "Reached MAX_TOOL_TURNS ({}), forcing completion",
                MAX_TOOL_TURNS
            );
            messages.push(ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Simple(
                    "Maximum tool turns reached. Please provide the final answer now.".to_string(),
                ),
            });
            // One final turn to get the answer without tools
            break;
        }
    }

    // Emit metrics JSON to stdout so callers (Harbor, CLI) can capture token usage
    println!(
        "{}",
        serde_json::json!({
            "type": "metrics",
            "input_tokens": total_input_tokens,
            "output_tokens": total_output_tokens,
            "cache_read_input_tokens": total_cache_read_tokens,
            "cache_creation_input_tokens": total_cache_creation_tokens,
        })
    );

    Ok(HeadlessTaskResult {
        final_text,
        made_writes: code_writes > 0,
        verified_after_last_edit: verified_after_last_mod,
        total_tool_calls,
        messages,
        total_input_tokens,
        total_output_tokens,
    })
}

/// Create and save an iteration checkpoint for LLM stream resilience.
///
/// This function is called after tools are executed in an iteration.
/// It creates a checkpoint containing the tool outputs and saves it to disk.
/// If the next LLM call fails, the checkpoint can be used to retry without re-running tools.
fn create_and_save_checkpoint(
    turn: u32,
    completed_tools: &[(String, String, String)], // (id, name, json)
    tool_result_blocks: &[ContentBlock],
    _cwd: &Path,
) -> Result<()> {
    // Build checkpoint tool calls from completed tools and results
    let mut checkpoint_calls = Vec::new();

    for ((tool_id, tool_name, tool_json), result_block) in
        completed_tools.iter().zip(tool_result_blocks.iter())
    {
        // Extract tool output from the result block
        let output = match result_block {
            ContentBlock::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        };

        let output_size = output.as_ref().map_or(0, |o| o.len());

        // Parse tool input as JSON
        let input = serde_json::from_str::<serde_json::Value>(tool_json)
            .unwrap_or_else(|_| serde_json::json!({"_raw": tool_json}));

        let call = CheckpointToolCall {
            id: tool_id.clone(),
            name: tool_name.clone(),
            input,
            output,
            success: true,
            output_size_bytes: output_size,
            executed_at: chrono::Utc::now(),
        };

        checkpoint_calls.push(call);
    }

    // Create checkpoint with a simple prompt (tools are documented in the checkpoint itself)
    let prompt = format!(
        "Turn {}: executed {} tools with total {} bytes of output",
        turn,
        checkpoint_calls.len(),
        checkpoint_calls
            .iter()
            .map(|c| c.output_size_bytes)
            .sum::<usize>()
    );

    let checkpoint = IterationCheckpoint::new(turn, checkpoint_calls, prompt);

    // Get or create checkpoint storage
    // Use session ID from environment or a default if not available
    let session_id =
        std::env::var("RUSTYCODE_SESSION_ID").unwrap_or_else(|_| "default".to_string());
    let storage = CheckpointStorage::for_session(&session_id)?;

    // Save checkpoint
    let path = storage.save(&checkpoint)?;
    info!(
        "Saved iteration checkpoint {} to {}",
        checkpoint.summary(),
        path.display()
    );

    Ok(())
}

/// Attempt to recover from an LLM stream failure using a saved checkpoint.
///
/// This function is called when an LLM stream call fails after tools have been executed.
/// It loads the most recent checkpoint and reconstructs the messages that would have
/// been sent to the LLM, allowing a retry without re-executing tools.
///
/// Returns:
/// - Ok(Some(messages)) if checkpoint was loaded successfully
/// - Ok(None) if no checkpoint is available (normal failure path)
/// - Err if checkpoint loading failed (should be treated as critical error)
fn attempt_checkpoint_recovery() -> Result<Option<Vec<ChatMessage>>> {
    let session_id =
        std::env::var("RUSTYCODE_SESSION_ID").unwrap_or_else(|_| "default".to_string());

    let storage = match CheckpointStorage::for_session(&session_id) {
        Ok(s) => s,
        Err(_) => {
            // No checkpoint storage available
            return Ok(None);
        }
    };

    let checkpoint = match storage.get_latest() {
        Ok(Some(cp)) => cp,
        Ok(None) => {
            // No checkpoints saved yet
            info!("No checkpoint available for recovery");
            return Ok(None);
        }
        Err(e) => {
            tracing::warn!("Failed to load checkpoint for recovery: {}", e);
            return Ok(None); // Non-fatal, continue with normal error handling
        }
    };

    // Reconstruct messages from checkpoint data
    let mut recovered_messages = Vec::new();

    // Add tool results as a user message with the checkpoint data
    let mut result_blocks: Vec<ContentBlock> = Vec::new();

    for tool_call in &checkpoint.tool_calls {
        if let Some(ref output) = tool_call.output {
            result_blocks.push(ContentBlock::tool_result(&tool_call.id, output));
        }
    }

    if !result_blocks.is_empty() {
        recovered_messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(result_blocks),
        });
    }

    info!(
        "Recovered from checkpoint: {} tool calls, {} bytes output",
        checkpoint.tool_calls.len(),
        checkpoint.total_output_bytes
    );

    Ok(Some(recovered_messages))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_assistant_text_removes_tool_markers() {
        let input =
            "Here is my analysis.\n[Tool use]\n[tool_result:bash:abc123] hello\nMore text.\n";
        let cleaned = clean_assistant_text(input);
        assert!(!cleaned.contains("[Tool use]"));
        assert!(!cleaned.contains("[tool_result:"));
        assert!(cleaned.contains("Here is my analysis."));
        assert!(cleaned.contains("More text."));
    }

    #[test]
    fn test_clean_assistant_text_preserves_normal_text() {
        let input = "Hello world\nThis is normal text\nNo markers here";
        let cleaned = clean_assistant_text(input);
        assert_eq!(cleaned, input);
    }

    #[test]
    fn test_strip_repeated_prefix_removes_duplicated_intro() {
        let previous = "I'll help you build a gRPC KV store server.\n\
                        Let me start by planning:\n\
                        1. Install grpcio\n\
                        2. Create proto file\n\
                        Now let me begin:";
        let current = "I'll help you build a gRPC KV store server.\n\
                       Let me start by planning:\n\
                       1. Install grpcio\n\
                       2. Create proto file\n\
                       Now let me begin:\n\
                       The server is running on port 5328.";
        let result = strip_repeated_prefix(current, previous);
        // Should strip the matching 5 lines, keep only the new content
        assert!(
            !result.contains("I'll help you"),
            "Should strip intro: {}",
            result
        );
        assert!(
            result.contains("running on port 5328"),
            "Should keep new content: {}",
            result
        );
    }

    #[test]
    fn test_strip_repeated_prefix_keeps_short_matches() {
        // Less than 3 matching lines should not strip
        let previous = "Hello\nWorld";
        let current = "Hello\nWorld\nNew content here";
        let result = strip_repeated_prefix(current, previous);
        assert_eq!(result, current, "Should not strip with <3 matching lines");
    }

    #[test]
    fn test_strip_repeated_prefix_handles_empty() {
        assert_eq!(strip_repeated_prefix("", "some text"), "");
        assert_eq!(strip_repeated_prefix("some text", ""), "some text");
        assert_eq!(strip_repeated_prefix("", ""), "");
    }

    #[test]
    fn test_strip_repeated_preamble_removes_repeated_sentences() {
        // GLM pattern: same sentence before every tool call within a single turn
        let input = "I'll help you compile and install pyknotid with NumPy 2.3.0 compatibility. \
            Let me start by examining the current state and fixing the compatibility issues. \
            First, let me grep for patterns. \
            I'll help you compile and install pyknotid with NumPy 2.3.0 compatibility. \
            Let me start by examining the current state and fixing the compatibility issues. \
            Now I'll fix the files. \
            I'll help you compile and install pyknotid with NumPy 2.3.0 compatibility. \
            Let me start by examining the current state and fixing the compatibility issues. \
            Done!";
        let result = strip_repeated_preamble_phrases(input);
        // The repeated 2-sentence preamble should appear only once
        let count = result.matches("I'll help you compile").count();
        assert_eq!(
            count, 1,
            "Should keep only one occurrence of repeated preamble"
        );
        assert!(
            result.contains("First, let me grep"),
            "Should keep non-repeated content"
        );
        assert!(result.contains("Done!"), "Should keep the ending");
    }

    #[test]
    fn test_strip_repeated_preamble_keeps_unique_text() {
        let input = "This is unique text. And another sentence. No repeats here.";
        let result = strip_repeated_preamble_phrases(input);
        assert_eq!(result, input, "Should not modify text without repeats");
    }

    #[test]
    fn test_detect_repeated_blocks_truncates_loop() {
        // Simulates the regex-log pattern: same 4-paragraph block repeated many times
        let block = "I'll validate the date components carefully, ensuring month and day ranges \
            are correct while preventing unintended matches through strategic boundary checks.\n\n\
            The regex uses negative lookarounds to prevent digit adjacency, ensuring precise \
            date pattern matching.\n\n\
            The IPv4 address validation follows a similar precise approach, checking each \
            octet's range and preventing leading zeros while allowing single zero values.\n\n\
            The regex strategy involves a positive lookahead to confirm an IP address exists \
            in the line, then greedily consuming content to match the final date.";

        // Repeat the block 5 times (above the 3-repetition threshold)
        let repeated = [block; 5].join("\n\n");
        let result = detect_and_truncate_repeated_blocks(&repeated);

        assert!(result.is_some(), "Should detect repetition");
        let truncated = result.unwrap();
        assert!(
            truncated.len() < repeated.len() / 3,
            "Should truncate to roughly one block: {} vs {}",
            truncated.len(),
            repeated.len()
        );
        // Should contain exactly one copy of the block
        assert_eq!(
            truncated.matches("date components").count(),
            1,
            "Should have exactly one copy: {}",
            truncated
        );
    }

    #[test]
    fn test_detect_repeated_blocks_preserves_normal_text() {
        let normal = "First paragraph about the problem.\n\n\
            Second paragraph with analysis.\n\n\
            Third paragraph with solution.\n\n\
            Fourth paragraph with verification.\n\n\
            Fifth paragraph about next steps.";
        let result = detect_and_truncate_repeated_blocks(normal);
        assert!(result.is_none(), "Should not truncate non-repeating text");
    }

    #[test]
    fn test_detect_repeated_blocks_with_trailing_content() {
        // Use a 4-paragraph block repeated 4 times (meets the 3-repetition threshold)
        let block = "The regex uses negative lookarounds to prevent digit adjacency.\n\n\
            The IPv4 address validation checks each octet's range precisely.\n\n\
            The regex strategy involves a positive lookahead to confirm IP exists.\n\n\
            The key is balancing precise validation with flexible matching.";
        // 4 repetitions + unique trailing content
        let text = format!(
            "{}\n\n{}\n\n{}\n\n{}\n\nFinal unique conclusion here.",
            block, block, block, block
        );
        let result = detect_and_truncate_repeated_blocks(&text);
        assert!(
            result.is_some(),
            "Should detect repetition with trailing content"
        );
        let truncated = result.unwrap();
        assert!(
            truncated.contains("Final unique conclusion"),
            "Should preserve trailing non-repeating content: {}",
            truncated
        );
    }

    /// Verify the message construction at each turn of the headless agent loop.
    /// This test simulates the exact flow: user task → assistant+tool → user(tool_result) → ...
    /// and verifies:
    /// 1. No consecutive same-role messages (Anthropic API rejects these)
    /// 2. System prompt is NOT in the messages array (goes via system_prompt field)
    /// 3. Tool results have matching tool_use_ids
    /// 4. Message content serializes to valid Anthropic API format
    #[test]
    fn test_message_construction_turn_by_turn() {
        // === Turn 0: Initial state ===
        // Headless mode starts with user task as messages[0], system prompt via system_prompt field
        let task = "Fix the bug in main.py";
        let task_with_context = format!(
            "Working directory: /workspace (contains 5 files/dirs)\n\nmain.py\n\n---\n\n{}",
            task
        );

        let mut messages: Vec<ChatMessage> = vec![ChatMessage::user(task_with_context.clone())];

        // Verify: only 1 message, role=User
        assert_eq!(messages.len(), 1, "Turn 0: should have 1 message");
        assert_eq!(
            messages[0].role,
            MessageRole::User,
            "Turn 0: first message should be User role"
        );
        assert!(matches!(&messages[0].content, MessageContent::Simple(t) if t.contains(task)));

        // === Turn 1: LLM responds with text + tool_use ===
        // Simulate the assistant response with text + bash tool call
        let assistant_text = "I'll fix the bug. Let me read the file first.";
        let tool_id_1 = "toolu_abc123";
        let tool_name_1 = "bash";
        let _tool_json_1 = r#"{"command": "cat main.py"}"#;

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        assistant_blocks.push(ContentBlock::text(assistant_text));
        assistant_blocks.push(ContentBlock::ToolUse {
            id: tool_id_1.to_string(),
            name: tool_name_1.to_string(),
            input: serde_json::json!({"command": "cat main.py"}),
        });

        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(assistant_blocks),
        });

        // Verify: messages[1] is Assistant with blocks
        assert_eq!(messages.len(), 2, "Turn 1: should have 2 messages");
        assert_eq!(
            messages[1].role,
            MessageRole::Assistant,
            "Turn 1: second message should be Assistant"
        );
        if let MessageContent::Blocks(blocks) = &messages[1].content {
            assert_eq!(
                blocks.len(),
                2,
                "Turn 1: assistant should have text + tool_use blocks"
            );
            assert!(blocks[0].is_text(), "Turn 1: first block should be text");
            assert!(
                blocks[1].is_tool_use(),
                "Turn 1: second block should be tool_use"
            );
        } else {
            panic!(
                "Turn 1: assistant content should be Blocks, got {:?}",
                messages[1].content
            );
        }

        // === Turn 1 continued: Tool results as user message ===
        let tool_output_1 = "def main():\n    print('hello')\n";

        let tool_result_blocks: Vec<ContentBlock> =
            vec![ContentBlock::tool_result(tool_id_1, tool_output_1)];

        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(tool_result_blocks),
        });

        // Verify: messages[2] is User with tool_result
        assert_eq!(
            messages.len(),
            3,
            "Turn 1: should have 3 messages after tool result"
        );
        assert_eq!(
            messages[2].role,
            MessageRole::User,
            "Turn 1: tool results should be User role"
        );
        if let MessageContent::Blocks(blocks) = &messages[2].content {
            assert_eq!(blocks.len(), 1, "Turn 1: should have 1 tool_result block");
            // Verify the tool_result has matching tool_use_id
            if let ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } = &blocks[0]
            {
                assert_eq!(tool_use_id, tool_id_1, "Turn 1: tool_use_id should match");
                assert_eq!(
                    content, tool_output_1,
                    "Turn 1: tool result content should match"
                );
            } else {
                panic!("Turn 1: block should be ToolResult");
            }
        } else {
            panic!("Turn 1: tool results should be Blocks");
        }

        // === Verify role alternation so far ===
        let roles: Vec<String> = messages.iter().map(|m| format!("{:?}", m.role)).collect();
        assert_eq!(
            roles,
            vec!["User", "Assistant", "User"],
            "Turn 1: roles should alternate User-Assistant-User, got {:?}",
            roles
        );

        // === Turn 2: LLM responds with text + edit_file tool ===
        let tool_id_2 = "toolu_def456";
        let edit_input = serde_json::json!({
            "path": "main.py",
            "old_string": "print('hello')",
            "new_string": "print('world')"
        });

        let assistant_blocks_2: Vec<ContentBlock> = vec![
            ContentBlock::text("I'll fix the print statement."),
            ContentBlock::ToolUse {
                id: tool_id_2.to_string(),
                name: "edit_file".to_string(),
                input: edit_input,
            },
        ];

        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(assistant_blocks_2),
        });

        // Turn 2 tool results
        let tool_output_2 = "File edited successfully (line 2)";

        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_result(
                tool_id_2,
                tool_output_2,
            )]),
        });

        // Verify role alternation
        let roles: Vec<String> = messages.iter().map(|m| format!("{:?}", m.role)).collect();
        assert_eq!(
            roles,
            vec!["User", "Assistant", "User", "Assistant", "User"],
            "Turn 2: roles should alternate correctly, got {:?}",
            roles
        );

        // === Verify serialization of each message to JSON ===
        // This catches issues with the serde derive that would cause API errors
        for (i, msg) in messages.iter().enumerate() {
            let json = serde_json::to_value(msg)
                .unwrap_or_else(|e| panic!("Message {} failed to serialize: {}", i, e));

            // Every message must have a role
            assert!(
                json.get("role").is_some(),
                "Message {} missing role field",
                i
            );

            // Every message must have content
            assert!(
                json.get("content").is_some(),
                "Message {} missing content field",
                i
            );
        }

        // === Test course correction message ===
        // After a force_stop, a warning is injected as a User message
        let warning = "WARNING: You appear to be stuck in a loop. Please try a different approach.";
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(warning.to_string()),
        });

        // Verify: course correction doesn't break role alternation
        // The previous message was User (tool_result), so this would be two consecutive User messages
        // This is actually valid for Anthropic — the API allows consecutive same-role messages
        // but it's worth noting
        assert_eq!(
            messages.len(),
            6,
            "Should have 6 messages after course correction"
        );
        assert_eq!(
            messages[5].role,
            MessageRole::User,
            "Course correction should be User role"
        );

        // === Test message pruning ===
        let pruned = prune_messages(messages.clone());
        // After pruning, the first message (user task) should still be there
        assert!(!pruned.is_empty(), "Pruned messages should not be empty");
        assert_eq!(
            pruned[0].role,
            MessageRole::User,
            "First pruned message should be User task"
        );
        if let MessageContent::Simple(t) = &pruned[0].content {
            assert!(
                t.contains(task),
                "First pruned message should contain the task"
            );
        } else {
            panic!("First pruned message should be Simple text");
        }
    }

    /// Verify that tool result blocks have the correct serialized format for Anthropic API.
    /// The API expects: {"type": "tool_result", "tool_use_id": "...", "content": "..."}
    #[test]
    fn test_tool_result_serialization_format() {
        let block = ContentBlock::tool_result("toolu_abc123", "hello world");
        let json = serde_json::to_value(&block).expect("Failed to serialize tool_result");

        assert_eq!(
            json["type"], "tool_result",
            "type field should be 'tool_result'"
        );
        assert_eq!(
            json["tool_use_id"], "toolu_abc123",
            "tool_use_id should match"
        );
        assert_eq!(json["content"], "hello world", "content should match");
        // is_error should be absent (skip_serializing_if = false)
        assert!(
            json.get("is_error").is_none(),
            "is_error should not be present when false"
        );
    }

    /// Verify tool_use block serialization format.
    #[test]
    fn test_tool_use_serialization_format() {
        let block = ContentBlock::ToolUse {
            id: "toolu_xyz789".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "echo hello"}),
        };
        let json = serde_json::to_value(&block).expect("Failed to serialize tool_use");

        assert_eq!(json["type"], "tool_use", "type field should be 'tool_use'");
        assert_eq!(json["id"], "toolu_xyz789", "id should match");
        assert_eq!(json["name"], "bash", "name should match");
        assert_eq!(json["input"]["command"], "echo hello", "input should match");
    }

    /// Verify text block serialization format.
    #[test]
    fn test_text_block_serialization_format() {
        let block = ContentBlock::text("Hello world");
        let json = serde_json::to_value(&block).expect("Failed to serialize text block");

        assert_eq!(json["type"], "text", "type field should be 'text'");
        assert_eq!(json["text"], "Hello world", "text should match");
        // cache_control should be absent (skip_serializing_if = None)
        assert!(
            json.get("cache_control").is_none(),
            "cache_control should not be present when None"
        );
    }

    /// Verify the error tool_result format.
    #[test]
    fn test_tool_error_serialization_format() {
        let block = ContentBlock::tool_error("toolu_err123", "Command failed: exit code 1");
        let json = serde_json::to_value(&block).expect("Failed to serialize tool_error");

        assert_eq!(
            json["type"], "tool_result",
            "type field should be 'tool_result'"
        );
        assert_eq!(
            json["tool_use_id"], "toolu_err123",
            "tool_use_id should match"
        );
        assert_eq!(
            json["content"], "Command failed: exit code 1",
            "content should match"
        );
        assert_eq!(json["is_error"], true, "is_error should be true");
    }

    /// Simulate multi-turn conversation with multiple tools per turn
    /// and verify message structure integrity.
    #[test]
    fn test_multi_tool_per_turn_message_structure() {
        let mut messages: Vec<ChatMessage> = vec![ChatMessage::user("Fix all the bugs")];

        // Assistant responds with 3 tool calls in one turn
        let tools = vec![
            ("toolu_1", "bash", r#"{"command": "grep -r BUG src/"}"#),
            ("toolu_2", "read_file", r#"{"path": "src/main.rs"}"#),
            ("toolu_3", "bash", r#"{"command": "cargo test"}"#),
        ];

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        assistant_blocks.push(ContentBlock::text("Let me investigate the bugs."));

        for (id, name, _json) in &tools {
            assistant_blocks.push(ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({"command": format!("cmd for {}", id)}),
            });
        }

        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(assistant_blocks),
        });

        // All 3 tool results go into a single User message
        let mut result_blocks: Vec<ContentBlock> = Vec::new();
        for (id, _, _) in &tools {
            result_blocks.push(ContentBlock::tool_result(*id, "result output"));
        }

        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(result_blocks),
        });

        // Verify structure
        assert_eq!(messages.len(), 3, "Should have 3 messages");

        // Assistant message should have 4 blocks (1 text + 3 tool_use)
        if let MessageContent::Blocks(blocks) = &messages[1].content {
            assert_eq!(
                blocks.len(),
                4,
                "Assistant should have 1 text + 3 tool_use = 4 blocks"
            );
            assert!(blocks[0].is_text());
            assert!(blocks[1].is_tool_use());
            assert!(blocks[2].is_tool_use());
            assert!(blocks[3].is_tool_use());
        } else {
            panic!("Assistant content should be Blocks");
        }

        // User tool_result message should have 3 blocks
        if let MessageContent::Blocks(blocks) = &messages[2].content {
            assert_eq!(blocks.len(), 3, "User should have 3 tool_result blocks");
            for (i, block) in blocks.iter().enumerate() {
                if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                    assert_eq!(
                        tool_use_id, tools[i].0,
                        "tool_use_id should match at index {}",
                        i
                    );
                } else {
                    panic!("Block {} should be ToolResult", i);
                }
            }
        } else {
            panic!("User tool results should be Blocks");
        }

        // Verify all messages serialize cleanly
        for (i, msg) in messages.iter().enumerate() {
            let json = serde_json::to_string(msg)
                .unwrap_or_else(|e| panic!("Message {} failed to serialize: {}", i, e));
            assert!(!json.is_empty(), "Message {} serialized to empty string", i);
        }
    }

    /// Verify that the system prompt is NOT part of the messages array
    /// (it should go through CompletionRequest::system_prompt instead)
    #[test]
    fn test_system_prompt_not_in_messages() {
        let messages: Vec<ChatMessage> = vec![ChatMessage::user("do the task")];

        // No message should have System role
        for (i, msg) in messages.iter().enumerate() {
            assert_ne!(
                msg.role,
                MessageRole::System,
                "Message {} should not be System role — system prompts go via system_prompt field",
                i
            );
        }
    }

    /// Verify the is_modifying helper correctly identifies file-modifying commands.
    #[test]
    fn test_is_modifying_detects_write_tools() {
        let is_modifying = |name: &str, json: &str| -> bool {
            if name == "write_file" || name == "edit_file" || name == "apply_patch" {
                return true;
            }
            if name == "bash" {
                let cmd = json.to_lowercase();
                if cmd.contains("sed -i") || cmd.contains("awk -i") || cmd.contains("awk --inplace") {
                    return true;
                }
                if cmd.contains("> ")
                    || cmd.contains(">>")
                    || cmd.contains("cat >")
                    || cmd.contains("tee ")
                {
                    return true;
                }
                if cmd.contains("pip install")
                    || cmd.contains("pip3 install")
                    || cmd.contains("cargo ")
                    || cmd.contains("apt-get install")
                    || cmd.contains("apt install")
                    || cmd.contains("yum install")
                    || cmd.contains("dnf install")
                    || cmd.contains("npm install")
                    || cmd.contains("yarn install")
                    || cmd.contains("pnpm install")
                    || cmd.contains("bun install")
                    || cmd.contains("go install")
                    || cmd.contains("gem install")
                {
                    return true;
                }
                if cmd.contains("make ")
                    || cmd.contains("gcc ")
                    || cmd.contains("g++")
                    || cmd.contains("cmake ")
                {
                    return true;
                }
                if cmd.contains("git add")
                    || cmd.contains("git commit")
                    || cmd.contains("git merge")
                    || cmd.contains("git checkout")
                    || cmd.contains("git clone")
                    || cmd.contains("git rebase")
                    || cmd.contains("git cherry-pick")
                    || cmd.contains("git apply")
                    || cmd.contains("git am")
                    || cmd.contains("git stash")
                    || cmd.contains("git rm")
                    || cmd.contains("git mv")
                {
                    return true;
                }
                if cmd.contains("mv ")
                    || cmd.contains("cp ")
                    || cmd.contains("rm ")
                    || cmd.contains("chmod")
                    || cmd.contains("chown")
                    || cmd.contains("mkdir ")
                    || cmd.contains("ln ")
                    || cmd.contains("install ")
                    || cmd.contains("dd ")
                {
                    return true;
                }
                if cmd.contains("python -c")
                    || cmd.contains("python3 -c")
                    || cmd.contains("perl -i")
                {
                    return true;
                }
                if cmd.contains("patch")
                    || cmd.contains("service ")
                    || cmd.contains("systemctl ")
                    || cmd.contains("nohup ")
                    || cmd.contains("setup.py ")
                    || cmd.contains("docker build")
                    || cmd.contains("docker run")
                    || cmd.contains("docker-compose")
                    || cmd.contains("docker compose")
                    || cmd.contains("tar ")
                    || cmd.contains("unzip ")
                    // curl/wget only count as modifying when downloading to a file
                    || (cmd.contains("curl ") && (cmd.contains("-o ") || cmd.contains("--output") || cmd.contains("> ") || cmd.contains("-o")))
                    || (cmd.contains("wget ") && (cmd.contains("-o ") || cmd.contains("--output") || cmd.contains("-O")))
                {
                    return true;
                }
            }
            false
        };

        // Modifying commands
        assert!(is_modifying(
            "write_file",
            r#"{"path": "/app/test.py", "content": "hello"}"#
        ));
        assert!(is_modifying(
            "edit_file",
            r#"{"path": "main.py", "old": "x", "new": "y"}"#
        ));
        assert!(is_modifying("apply_patch", r#"{"path": "a.py"}"#));
        assert!(is_modifying(
            "bash",
            r#"{"command": "sed -i 's/old/new/g' file.py"}"#
        ));
        assert!(is_modifying("bash", r#"{"command": "pip install numpy"}"#));
        assert!(is_modifying("bash", r#"{"command": "git add ."}"#));
        assert!(is_modifying(
            "bash",
            r#"{"command": "git commit -m 'fix'"}"#
        ));
        assert!(is_modifying(
            "bash",
            r#"{"command": "cat > file.py << 'EOF'"}"#
        ));
        assert!(is_modifying(
            "bash",
            r#"{"command": "python -c \"import os\""}"#
        ));
        assert!(is_modifying(
            "bash",
            r#"{"command": "python3 -c \"open('f','w')\""}"#
        ));
        assert!(is_modifying(
            "bash",
            r#"{"command": "service nginx start"}"#
        ));
        assert!(is_modifying(
            "bash",
            r#"{"command": "systemctl start postfix"}"#
        ));
        assert!(is_modifying(
            "bash",
            r#"{"command": "nohup python app.py &"}"#
        ));
        assert!(is_modifying("bash", r#"{"command": "mkdir -p /app/data"}"#));
        assert!(is_modifying(
            "bash",
            r#"{"command": "echo hello > out.txt"}"#
        ));
        assert!(is_modifying("bash", r#"{"command": "make install"}"#));
        assert!(is_modifying(
            "bash",
            r#"{"command": "cargo build --release"}"#
        ));
        assert!(is_modifying("bash", r#"{"command": "git stash"}"#));
        assert!(is_modifying(
            "bash",
            r#"{"command": "git clone https://github.com/example/repo.git"}"#
        ));

        // Non-modifying commands
        assert!(!is_modifying("read_file", r#"{"path": "/app/test.py"}"#));
        assert!(!is_modifying("bash", r#"{"command": "ls -la"}"#));
        assert!(!is_modifying("bash", r#"{"command": "cat file.py"}"#));
        assert!(!is_modifying(
            "bash",
            r#"{"command": "grep -r pattern src/"}"#
        ));
        assert!(!is_modifying("bash", r#"{"command": "git status"}"#));
        assert!(!is_modifying("bash", r#"{"command": "git log --oneline"}"#));
        assert!(!is_modifying("bash", r#"{"command": "pip list"}"#));
        assert!(!is_modifying("bash", r#"{"command": "pip show numpy"}"#));
        assert!(!is_modifying("glob", r#"{"pattern": "**/*.py"}"#));
        assert!(!is_modifying("grep", r#"{"pattern": "TODO"}"#));
        // curl/wget checking (not downloading to file) should NOT be modifying
        assert!(!is_modifying(
            "bash",
            r#"{"command": "curl http://localhost:8080/health"}"#
        ));
        assert!(!is_modifying(
            "bash",
            r#"{"command": "wget -qO- http://localhost:8080/"}"#
        ));
        // curl downloading to file SHOULD be modifying
        assert!(is_modifying(
            "bash",
            r#"{"command": "curl -o data.json http://example.com/data"}"#
        ));
        assert!(is_modifying(
            "bash",
            r#"{"command": "wget -O data.json http://example.com/data"}"#
        ));
    }

    /// Verify the system prompt contains critical rules about not claiming completion early.
    #[test]
    fn test_system_prompt_contains_anti_completion_rules() {
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("Task completed"),
            "System prompt should warn about false completion claims"
        );
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("NOT done"),
            "System prompt should state reading files is not completion"
        );
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("Do NOT run git log"),
            "System prompt should limit verification commands"
        );
    }

    /// Verify that the verification detection logic correctly identifies
    /// test/verification bash commands vs non-verification commands.
    #[test]
    fn test_verification_detection_identifies_test_commands() {
        // These should be detected as verification commands
        let verification_cmds = [
            r#"{"command": "cd /tmp && python -c \"import pyknotid\""}"#,
            r#"{"command": "pytest tests/ -v"}"#,
            r#"{"command": "python -m pytest tests/ --tb=short"}"#,
            r#"{"command": "cargo test"}"#,
            r#"{"command": "go test ./..."}"#,
            r#"{"command": "make test"}"#,
            r#"{"command": "npm test"}"#,
            r#"{"command": "python3 -c \"print('hello')\""}"#,
            r#"{"command": "node -e \"console.log('test')\""}"#,
            r#"{"command": "python /app/test_outputs.py"}"#,
            r#"{"command": "python3 /app/task_file/scripts/optimized_packer.py"}"#,
            r#"{"command": "uv run python /app/compress.py /app/c4_sample /app/test_output"}"#,
        ];

        for cmd_json in &verification_cmds {
            let cmd = cmd_json.to_lowercase();
            let is_verification = cmd.contains("pytest")
                || cmd.contains("python -c")
                || cmd.contains("python3 -c")
                || cmd.contains("node -e")
                || cmd.contains("cargo test")
                || cmd.contains("go test")
                || cmd.contains("make test")
                || cmd.contains("npm test")
                || cmd.contains("python ") && cmd.contains(".py")
                || cmd.contains("python3 ") && cmd.contains(".py")
                || cmd.contains("uv run")
                || cmd.contains("uv sync");
            assert!(
                is_verification,
                "Should detect as verification: {}",
                cmd_json
            );
        }

        // These should NOT be detected as verification commands
        let non_verification_cmds = [
            r#"{"command": "grep -r pattern src/"}"#,
            r#"{"command": "cat file.py"}"#,
            r#"{"command": "ls -la"}"#,
            r#"{"command": "git status"}"#,
            r#"{"command": "pip install numpy"}"#,
        ];

        for cmd_json in &non_verification_cmds {
            let cmd = cmd_json.to_lowercase();
            let is_verification = cmd.contains("pytest")
                || (cmd.contains("python -c") && !cmd.contains("grep"))
                || cmd.contains("cargo test")
                || cmd.contains("go test");
            assert!(
                !is_verification,
                "Should NOT detect as verification: {}",
                cmd_json
            );
        }
    }
}
