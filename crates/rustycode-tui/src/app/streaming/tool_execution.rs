//! Tool execution for LLM streaming
//!
//! This module handles the execution of tools detected during LLM streaming,
//! including timeout handling, caching, and error tracking.

use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use crate::{ErrorTracker, FileReadCache};
use rustycode_core::integration::{HookContext, HookRegistry};
use rustycode_guard::codec::{HookInput, HookResult};
use rustycode_guard::pre_tool;
use rustycode_protocol::ToolCall;
use rustycode_tools::ToolExecutor;

use super::parse_tool_parameters;

/// Execute a tool with the given name and parameters (JSON string)
///
/// This is the main tool execution function used during streaming. It parses
/// the JSON parameters, creates a ToolCall, and executes it with a 30-second timeout.
///
/// # Arguments
/// * `cwd` - Current working directory for tool execution context
/// * `tool_name` - Name of the tool to execute
/// * `parameters_json` - JSON string containing tool parameters
/// * `file_read_cache` - Optional file read deduplication cache
/// * `error_tracker` - Optional error tracker for repeated failures
/// * `todo_state` - Optional todo state for task tracking
/// * `tool_registry` - Optional custom tool registry (e.g., with skill tools)
///
/// # Returns
/// * Tool output on success
/// * Error message on failure (parsing error, execution error, or timeout)
///
/// # Timeout
///
/// Tools are executed with a 30-second timeout to prevent hung operations.
pub fn execute_tool(
    cwd: &Path,
    tool_name: &str,
    parameters_json: &str,
    file_read_cache: Option<&Arc<StdMutex<FileReadCache>>>,
    error_tracker: Option<&Arc<StdMutex<ErrorTracker>>>,
    todo_state: Option<&rustycode_tools::TodoState>,
    tool_registry: Option<&Arc<rustycode_tools::ToolRegistry>>,
) -> String {
    tracing::info!(
        "Executing tool: {} with params: {}",
        tool_name,
        parameters_json
    );

    // Parse the parameters JSON with repair fallback
    let arguments: serde_json::Value = parse_tool_parameters(parameters_json);

    // Guardrail pre-check
    if let Some(result) = check_tool_guard(tool_name, &arguments, cwd) {
        if result.permission_decision.as_deref() == Some("deny") {
            return format!(
                "BLOCKED: {}",
                result.permission_decision_reason.unwrap_or_default()
            );
        }
    }

    // Extract path value for cache operations
    let path_str = arguments
        .get("path")
        .and_then(|p| p.as_str())
        .map(|s| s.to_string());

    // Check file read cache for read_file tool
    if tool_name == "read_file" {
        if let Some(ref path_value) = path_str {
            let file_path = cwd.join(path_value);
            if let Some(cache) = file_read_cache {
                if let Ok(mut cache_guard) = cache.lock() {
                    if let Some(entry) = cache_guard.check(&file_path) {
                        if entry.read_count >= 3 {
                            return format!(
                                "[DUPLICATE READ] You have already read '{}' {} times in this conversation. \
                                 The content has not changed since your last read. \
                                 Please use the information you already have and proceed with your task.",
                                path_value, entry.read_count
                            );
                        }
                    }
                }
            }
        }
    }

    // Invalidate cache on write operations
    if matches!(tool_name, "write_file" | "apply_patch" | "edit_file") {
        if let Some(ref path_value) = path_str {
            let file_path = cwd.join(path_value);
            if let Some(cache) = file_read_cache {
                if let Ok(mut cache_guard) = cache.lock() {
                    cache_guard.invalidate(&file_path);
                }
            }
        }
    }

    // Create a ToolCall with generated ID
    let tool_call = ToolCall::with_generated_id(tool_name, arguments);

    // Create tool executor with custom registry if provided
    let executor = if let Some(registry) = tool_registry {
        ToolExecutor::with_registry(cwd.to_path_buf(), Arc::clone(registry))
    } else if let Some(state) = todo_state {
        ToolExecutor::with_todo_state(cwd.to_path_buf(), state.clone())
    } else {
        ToolExecutor::new(cwd.to_path_buf())
    };

    // Execute with timeout (120s for complex operations like compilation, test suites)
    let tool_result: rustycode_protocol::ToolResult = rustycode_shared_runtime::block_on_shared(
        async move {
            match tokio::time::timeout(
                Duration::from_secs(120),
                executor.execute_cached_with_session(&tool_call, None),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    tracing::error!("Tool execution timeout after 120s");
                    rustycode_protocol::ToolResult {
                        call_id: tool_call.call_id.clone(),
                        output: String::new(),
                        error: Some("Tool execution timed out after 120s. Try simplifying the operation or breaking it into smaller steps.".to_string()),
                        exit_code: None,
                        success: false,
                        data: None,
                    }
                }
            }
        },
    );

    // Return the appropriate content based on result
    if tool_result.success {
        tracing::info!("Tool executed successfully");

        // Record successful file reads in cache
        if tool_name == "read_file" {
            if let Some(ref path_value) = path_str {
                let file_path = cwd.join(path_value);
                if let Some(cache) = file_read_cache {
                    if let Ok(mut cache_guard) = cache.lock() {
                        let mtime = std::fs::metadata(&file_path)
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);

                        let has_images = tool_result.output.contains("image")
                            || tool_result.output.contains("![")
                            || tool_result.output.contains("<img");

                        cache_guard.record_read(&file_path, mtime, has_images);
                    }
                }
            }
        }

        // Clear error tracking on success
        if let Some(tracker) = error_tracker {
            if let Ok(mut tracker_guard) = tracker.lock() {
                tracker_guard.clear_errors(tool_name);
            }
        }

        tool_result.output
    } else {
        let error_msg = tool_result
            .error
            .unwrap_or_else(|| "Tool returned no output or error details".to_string());
        tracing::error!("Tool execution failed: {}", error_msg);

        // Track error for potential alternative suggestions
        if let Some(tracker) = error_tracker {
            if let Ok(mut tracker_guard) = tracker.lock() {
                tracker_guard.record_error(tool_name, &error_msg);

                if tracker_guard.should_suggest_alternative(tool_name) {
                    if let Some(alt) = tracker_guard.suggest_alternative_tool(tool_name) {
                        tracing::warn!("Tool error recovery suggestion: {}", alt);
                    }
                }
            }
        }

        format!("Error: {}", error_msg)
    }
}

/// Snapshot file content before a write operation for /undo support.
///
/// Returns `Some(batch)` if files were snapshotted, `None` for non-write tools.
/// The batch contains `(path, old_content)` pairs. If the file doesn't exist,
/// the old_content is an empty string (so /undo will delete the new file).
pub fn snapshot_files_for_undo(
    cwd: &Path,
    tool_name: &str,
    parameters_json: &str,
) -> Option<Vec<(String, String)>> {
    if !matches!(tool_name, "write_file" | "edit_file" | "search_replace") {
        return None;
    }

    let arguments: serde_json::Value = parse_tool_parameters(parameters_json);
    let mut batch = Vec::new();

    // Extract target file path(s)
    let paths: Vec<String> = arguments
        .get("path")
        .and_then(|p| p.as_str())
        .map(|s| vec![s.to_string()])
        .unwrap_or_default();

    for path_str in paths {
        let full_path = cwd.join(&path_str);
        let old_content = std::fs::read_to_string(&full_path).unwrap_or_default();
        batch.push((full_path.to_string_lossy().to_string(), old_content));
    }

    if batch.is_empty() {
        None
    } else {
        Some(batch)
    }
}

/// Execute a tool with hooks - called before and after tool execution
///
/// This function extends `execute_tool` with pre and post execution hooks.
/// Hooks can be used for logging, permission checks, or custom processing.
///
/// # Arguments
/// * `cwd` - Current working directory
/// * `tool_name` - Name of the tool to execute
/// * `parameters_json` - JSON string containing tool parameters
/// * `hook_registry` - Registry of hooks to execute
/// * `context` - Hook context for passing data to hooks
///
/// # Returns
/// * Tool output on success
/// * Error message if execution is denied or fails
#[allow(clippy::await_holding_lock)]
pub fn execute_tool_with_hooks(
    cwd: &Path,
    tool_name: &str,
    parameters_json: &str,
    hook_registry: &Arc<std::sync::RwLock<HookRegistry>>,
    context: &HookContext,
) -> String {
    // Parse the parameters JSON with repair fallback
    let _arguments: serde_json::Value = parse_tool_parameters(parameters_json);

    // PRE-TOOL HOOK: Check if tool execution should be allowed
    let allow_execution = {
        let registry = match hook_registry.read() {
            Ok(r) => r,
            Err(_) => return "Error: Failed to acquire hook registry lock".to_string(),
        };

        // For now, just allow - async hooks would need different handling.
        drop(registry);
        true
    };

    if !allow_execution {
        return "Tool execution denied by hook".to_string();
    }

    let start_time = Instant::now();

    // Execute the tool (without cache/tracker in hook context)
    let result = execute_tool(cwd, tool_name, parameters_json, None, None, None, None);
    let duration = start_time.elapsed();

    // POST-TOOL HOOK: Fire and forget (async hooks in background)
    let tool_name_owned = tool_name.to_string();
    let result_owned = result.clone();
    let context_owned = context.clone();
    let registry_for_post = hook_registry.clone();

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create runtime for tool hooks: {}", e);
                return;
            }
        };
        rt.block_on(async {
            let exec_result = {
                let registry = match registry_for_post.read() {
                    Ok(r) => r,
                    Err(_) => return,
                };
                registry
                    .execute_post_tool_use(&context_owned, &tool_name_owned, &Ok(result_owned))
                    .await
            };
            let _ = exec_result;
        });
    });

    tracing::info!("Tool {} executed in {:?}", tool_name, duration);
    result
}

fn check_tool_guard(
    tool_name: &str,
    tool_input: &serde_json::Value,
    cwd: &Path,
) -> Option<HookResult> {
    let input = HookInput {
        session_id: None,
        tool_name: tool_name.to_string(),
        tool_input: tool_input.clone(),
        cwd: Some(cwd.to_string_lossy().to_string()),
        hook_event_name: Some("PreToolUse".to_string()),
    };
    Some(pre_tool::evaluate(&input))
}
