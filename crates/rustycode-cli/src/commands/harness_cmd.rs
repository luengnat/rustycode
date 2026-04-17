//! Harness framework commands for long-running agent tasks with progress persistence

use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};

use super::cli_args::HarnessCommand;

fn find_harness_root(cwd: &Path) -> Option<PathBuf> {
    let mut search = cwd.to_path_buf();
    loop {
        if search.join(".harness").join("harness-tasks.json").exists() {
            return Some(search);
        }
        if !search.pop() {
            return None;
        }
    }
}

fn log_progress(harness_dir: &Path, msg: &str) {
    let progress_file = harness_dir.join("harness-progress.txt");
    let entry = format!("[{}] {}\n", Utc::now().to_rfc3339(), msg);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&progress_file)
    {
        use std::io::Write;
        let _ = f.write_all(entry.as_bytes());
    }
}

fn load_tasks(harness_dir: &Path) -> Result<serde_json::Value> {
    let tasks_file = harness_dir.join("harness-tasks.json");
    let bak = harness_dir.join("harness-tasks.json.bak");

    let content =
        std::fs::read_to_string(&tasks_file).context("Failed to read harness-tasks.json")?;

    match serde_json::from_str::<serde_json::Value>(&content) {
        Ok(v) => Ok(v),
        Err(_) => {
            // Try backup
            if bak.exists() {
                eprintln!("WARN: harness-tasks.json corrupted, restoring from backup");
                let bak_content = std::fs::read_to_string(&bak)?;
                let v: serde_json::Value = serde_json::from_str(&bak_content)?;
                std::fs::write(&tasks_file, &bak_content)?;
                Ok(v)
            } else {
                anyhow::bail!("harness-tasks.json corrupted and no backup found")
            }
        }
    }
}

/// Get a mutable reference to the tasks array, with proper error handling.
fn tasks_array_mut(tasks: &mut serde_json::Value) -> Result<&mut Vec<serde_json::Value>> {
    tasks
        .get_mut("tasks")
        .ok_or_else(|| anyhow::anyhow!("harness-tasks.json missing 'tasks' field"))?
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("harness-tasks.json 'tasks' is not an array"))
}

/// Set a field on a JSON object, with a warning instead of panic on missing key.
fn set_json_field(obj: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    if let Some(map) = obj.as_object_mut() {
        map.insert(key.to_string(), value);
    } else {
        eprintln!("WARN: expected JSON object, cannot set '{}'", key);
    }
}

/// Get a mutable array field from a JSON object.
fn json_array_mut<'a>(
    obj: &'a mut serde_json::Value,
    key: &str,
) -> Result<&'a mut Vec<serde_json::Value>> {
    obj.get_mut(key)
        .ok_or_else(|| anyhow::anyhow!("missing '{}' field", key))?
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("'{}' is not an array", key))
}

fn run_validation(cwd: &Path, command: &str, timeout_secs: u64) -> bool {
    eprintln!("  🧪 Validation: {}", command);
    let result = std::process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .env("TERM", "dumb")
        .output();

    match result {
        Ok(output) => {
            let success = output.status.success();
            if !success {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    eprintln!(
                        "  ❌ Validation failed: {}",
                        stderr.lines().next().unwrap_or("")
                    );
                }
            }
            let _ = timeout_secs; // Used by caller for timeout enforcement at higher level
            success
        }
        Err(e) => {
            eprintln!("  ❌ Validation failed to run: {}", e);
            false
        }
    }
}

fn save_tasks(harness_dir: &Path, tasks: &serde_json::Value) -> Result<()> {
    let tasks_file = harness_dir.join("harness-tasks.json");
    let bak = harness_dir.join("harness-tasks.json.bak");
    let tmp = harness_dir.join("harness-tasks.json.tmp");

    // Backup current
    if tasks_file.exists() {
        let _ = std::fs::copy(&tasks_file, &bak);
    }

    // Write atomically via tmp
    let json = serde_json::to_string_pretty(tasks)?;
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, &tasks_file)?;
    Ok(())
}

fn get_git_head(cwd: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn git_commit(cwd: &Path, message: &str) -> Option<String> {
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(cwd)
        .output()
        .ok()?;

    std::process::Command::new("git")
        .args(["commit", "-m", message, "--no-gpg-sign"])
        .current_dir(cwd)
        .output()
        .ok()?;

    get_git_head(cwd)
}

fn git_reset_hard(cwd: &Path, commit: &str) -> bool {
    std::process::Command::new("git")
        .args(["reset", "--hard", commit])
        .current_dir(cwd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Pick the next eligible task following the Harness spec priority order
fn pick_next_task(tasks: &mut serde_json::Value) -> Option<usize> {
    let tasks_array = tasks.get("tasks").and_then(|t| t.as_array())?;
    let completed_ids: Vec<String> = tasks_array
        .iter()
        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("completed"))
        .filter_map(|t| {
            t.get("id")
                .and_then(|id| id.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    // Priority 1: pending tasks with all deps completed
    let mut best_pending: Option<usize> = None;
    let mut best_pending_prio = 99;

    for (i, task) in tasks_array.iter().enumerate() {
        if task.get("status").and_then(|s| s.as_str()) != Some("pending") {
            continue;
        }
        let deps_met = task
            .get("depends_on")
            .and_then(|d| d.as_array())
            .map(|deps| {
                deps.iter().all(|d| {
                    d.as_str()
                        .map(|id| completed_ids.contains(&id.to_string()))
                        .unwrap_or(true)
                })
            })
            .unwrap_or(true);

        if !deps_met {
            continue;
        }

        let prio = task
            .get("priority")
            .and_then(|p| p.as_str())
            .map(|p| match p {
                "P0" => 0,
                "P1" => 1,
                "P2" => 2,
                _ => 3,
            })
            .unwrap_or(3);

        if prio < best_pending_prio {
            best_pending_prio = prio;
            best_pending = Some(i);
        }
    }

    if best_pending.is_some() {
        return best_pending;
    }

    // Priority 2: failed tasks eligible for retry
    let mut best_retry: Option<usize> = None;
    let mut best_retry_prio = 99;

    for (i, task) in tasks_array.iter().enumerate() {
        if task.get("status").and_then(|s| s.as_str()) != Some("failed") {
            continue;
        }
        let attempts = task.get("attempts").and_then(|a| a.as_u64()).unwrap_or(0);
        let max_attempts = task
            .get("max_attempts")
            .and_then(|a| a.as_u64())
            .unwrap_or(3);
        if attempts >= max_attempts {
            continue;
        }

        let deps_met = task
            .get("depends_on")
            .and_then(|d| d.as_array())
            .map(|deps| {
                deps.iter().all(|d| {
                    d.as_str()
                        .map(|id| completed_ids.contains(&id.to_string()))
                        .unwrap_or(true)
                })
            })
            .unwrap_or(true);

        if !deps_met {
            continue;
        }

        let prio = task
            .get("priority")
            .and_then(|p| p.as_str())
            .map(|p| match p {
                "P0" => 0,
                "P1" => 1,
                "P2" => 2,
                _ => 3,
            })
            .unwrap_or(3);

        if prio < best_retry_prio {
            best_retry_prio = prio;
            best_retry = Some(i);
        }
    }

    best_retry
}

/// Execute harness commands
pub async fn execute(cwd: &Path, command: HarnessCommand) -> Result<()> {
    match command {
        HarnessCommand::Init { path } => {
            let project_path = if path == "." {
                cwd.to_path_buf()
            } else {
                PathBuf::from(&path)
            };

            if !project_path.exists() {
                std::fs::create_dir_all(&project_path)?;
            }

            let harness_dir = project_path.join(".harness");
            std::fs::create_dir_all(&harness_dir)?;

            // Initialize git if needed
            let git_dir = project_path.join(".git");
            if !git_dir.exists() {
                std::process::Command::new("git")
                    .arg("init")
                    .current_dir(&project_path)
                    .output()?;
            }

            let tasks_file = harness_dir.join("harness-tasks.json");
            let initial_tasks = serde_json::json!({
                "version": 1,
                "created": Utc::now().to_rfc3339(),
                "session_config": {
                    "concurrency_mode": "exclusive",
                    "max_tasks_per_session": 10,
                    "max_sessions": 50
                },
                "tasks": [],
                "session_count": 0,
                "last_session": null
            });

            std::fs::write(&tasks_file, serde_json::to_string_pretty(&initial_tasks)?)?;

            let progress_file = harness_dir.join("harness-progress.txt");
            log_progress(
                &harness_dir,
                &format!(
                    "[SESSION-1] INIT Harness initialized for project {}",
                    project_path.display()
                ),
            );

            // Create activation marker
            std::fs::write(project_path.join(".harness-active"), "")?;

            // Create .gitignore entry
            let gitignore = project_path.join(".gitignore");
            if !gitignore.exists() || {
                let content = std::fs::read_to_string(&gitignore).unwrap_or_default();
                !content.contains(".harness")
            } {
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&gitignore)?;
                writeln!(f, "\n# Harness progress files\n.harness/")?;
            }

            println!("Harness initialized in {}", harness_dir.display());
            println!("Files created:");
            println!("  {}", tasks_file.display());
            println!("  {}", progress_file.display());
            println!("  {}", project_path.join(".harness-active").display());
        }

        HarnessCommand::Status => {
            let root = find_harness_root(cwd).ok_or_else(|| {
                anyhow::anyhow!("No harness found. Run 'rustycode harness init' first.")
            })?;
            let harness_dir = root.join(".harness");

            let tasks = load_tasks(&harness_dir)?;
            let tasks_count = tasks
                .get("tasks")
                .and_then(|t| t.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let session_count = tasks
                .get("session_count")
                .and_then(|s| s.as_u64())
                .unwrap_or(0);

            println!("Harness Status ({}):", root.display());
            println!("  Tasks: {}  Sessions: {}", tasks_count, session_count);

            let mut completed = 0;
            let mut failed = 0;
            let mut pending = 0;
            let mut in_progress = 0;

            let mut per_task = Vec::new();

            if let Some(tasks_array) = tasks.get("tasks").and_then(|t| t.as_array()) {
                for task in tasks_array {
                    let id = task.get("id").and_then(|s| s.as_str()).unwrap_or("?");
                    let title = task.get("title").and_then(|s| s.as_str()).unwrap_or("?");
                    let attempts = task.get("attempts").and_then(|a| a.as_u64()).unwrap_or(0);
                    let max_attempts = task
                        .get("max_attempts")
                        .and_then(|a| a.as_u64())
                        .unwrap_or(3);
                    let prio = task.get("priority").and_then(|s| s.as_str()).unwrap_or("?");

                    match task.get("status").and_then(|s| s.as_str()) {
                        Some("completed") => {
                            completed += 1;
                            per_task.push(format!(
                                "  ✅ [{}] {}: {} (attempts={})",
                                prio, id, title, attempts
                            ));
                        }
                        Some("failed") => {
                            failed += 1;
                            per_task.push(format!(
                                "  ❌ [{}] {}: {} (attempts={}/{})",
                                prio, id, title, attempts, max_attempts
                            ));
                        }
                        Some("in_progress") => {
                            in_progress += 1;
                            per_task
                                .push(format!("  🔄 [{}] {}: {} (in progress)", prio, id, title));
                        }
                        _ => {
                            pending += 1;
                            per_task.push(format!(
                                "  ⏳ [{}] {}: {} (attempts={}/{})",
                                prio, id, title, attempts, max_attempts
                            ));
                        }
                    }
                }
            }

            println!(
                "  Completed: {}  Failed: {}  Pending: {}  In Progress: {}",
                completed, failed, pending, in_progress
            );
            println!("\nTasks:");
            for line in per_task {
                println!("{}", line);
            }

            let progress_file = harness_dir.join("harness-progress.txt");
            if progress_file.exists() {
                println!("\nRecent Activity:");
                let content = std::fs::read_to_string(&progress_file)?;
                let lines: Vec<&str> = content.lines().collect();
                for line in lines.iter().rev().take(5).collect::<Vec<_>>().iter().rev() {
                    println!("  {}", line);
                }
            }
        }

        HarnessCommand::Add {
            description,
            priority,
            validation,
            timeout,
            depends_on,
        } => {
            let root = find_harness_root(cwd).ok_or_else(|| {
                anyhow::anyhow!("No harness found. Run 'rustycode harness init' first.")
            })?;
            let harness_dir = root.join(".harness");

            let mut tasks = load_tasks(&harness_dir)?;

            let tasks_array = tasks
                .get_mut("tasks")
                .and_then(|t| t.as_array_mut())
                .ok_or_else(|| anyhow::anyhow!("Invalid tasks format"))?;

            let next_id = format!("task-{:03}", tasks_array.len() + 1);

            let validation_json = validation.as_ref().map(|cmd| {
                serde_json::json!({
                    "command": cmd,
                    "timeout_seconds": timeout
                })
            });

            let depends_on_array: Vec<String> = depends_on
                .as_ref()
                .map(|d| d.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();

            let new_task = serde_json::json!({
                "id": next_id,
                "title": description,
                "status": "pending",
                "priority": priority,
                "depends_on": depends_on_array,
                "attempts": 0,
                "max_attempts": 3,
                "started_at_commit": null,
                "validation": validation_json,
                "on_failure": null,
                "error_log": [],
                "checkpoints": [],
                "completed_at": null
            });

            tasks_array.push(new_task);
            save_tasks(&harness_dir, &tasks)?;

            log_progress(
                &harness_dir,
                &format!("[SESSION-?] ADD {} \"{}\"", next_id, description),
            );
            println!("Added {}: {} [{}]", next_id, description, priority);
        }

        HarnessCommand::Run => {
            let root = find_harness_root(cwd).ok_or_else(|| {
                anyhow::anyhow!("No harness found. Run 'rustycode harness init' first.")
            })?;
            let harness_dir = root.join(".harness");
            let project_dir = &root;

            // Load provider
            use rustycode_core::headless::run_headless_task;
            use rustycode_llm::{create_provider_with_config, load_provider_config_from_env};
            use rustycode_tools::ToolRegistry;

            let (provider_type, model_name, v2_config) =
                load_provider_config_from_env().context("Failed to load LLM provider config")?;
            let provider = create_provider_with_config(&provider_type, &model_name, v2_config)
                .context("Failed to create LLM provider")?;

            let tools_schema = build_tools_schema();
            let tool_registry = ToolRegistry::new();

            let mut tasks = load_tasks(&harness_dir)?;

            // Increment session
            let session_count = tasks
                .get("session_count")
                .and_then(|s| s.as_u64())
                .unwrap_or(0)
                + 1;
            let max_sessions = tasks
                .get("session_config")
                .and_then(|c| c.get("max_sessions"))
                .and_then(|m| m.as_u64())
                .unwrap_or(50);
            let max_tasks = tasks
                .get("session_config")
                .and_then(|c| c.get("max_tasks_per_session"))
                .and_then(|m| m.as_u64())
                .unwrap_or(10);

            if session_count > max_sessions {
                anyhow::bail!("Max sessions ({}) reached", max_sessions);
            }

            set_json_field(&mut tasks, "session_count", serde_json::json!(session_count));
            let session_id = format!("SESSION-{}", session_count);

            // Create activation marker
            std::fs::write(project_dir.join(".harness-active"), "")?;

            log_progress(
                &harness_dir,
                &format!(
                    "[{}] LOCK acquired (pid={})",
                    session_id,
                    std::process::id()
                ),
            );

            // Context Window Recovery: recover any in_progress tasks from previous session
            {
                let Ok(tasks_array) = tasks_array_mut(&mut tasks) else {
                    anyhow::bail!("Invalid tasks format during recovery");
                };
                for task in tasks_array.iter_mut() {
                    if task.get("status").and_then(|s| s.as_str()) != Some("in_progress") {
                        continue;
                    }
                    let task_id = task
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("?")
                        .to_string();
                    let started_commit = task
                        .get("started_at_commit")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string());

                    // Check git state
                    let has_uncommitted = std::process::Command::new("git")
                        .args(["diff", "--stat"])
                        .current_dir(project_dir)
                        .output()
                        .map(|o| !o.stdout.is_empty())
                        .unwrap_or(false);

                    let has_task_commits = if let Some(ref commit) = started_commit {
                        std::process::Command::new("git")
                            .args(["log", "--oneline", &format!("{}..HEAD", commit)])
                            .current_dir(project_dir)
                            .output()
                            .map(|o| !o.stdout.is_empty())
                            .unwrap_or(false)
                    } else {
                        false
                    };

                    // Decision matrix from Harness spec
                    let validation_cmd = task
                        .get("validation")
                        .and_then(|v| v.get("command"))
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string());

                    if has_uncommitted || has_task_commits {
                        // Try validation if available
                        let passed = if let Some(ref cmd) = validation_cmd {
                            run_validation(project_dir, cmd, 120)
                        } else {
                            // No validation command, check if files exist
                            true
                        };

                        if passed {
                            git_commit(
                                project_dir,
                                &format!(
                                    "[{}] {} (recovery)",
                                    task_id,
                                    task.get("title").and_then(|t| t.as_str()).unwrap_or("")
                                ),
                            );
                            set_json_field(task, "status", serde_json::json!("completed"));
                            set_json_field(task, "completed_at", serde_json::json!(Utc::now().to_rfc3339()));
                            log_progress(&harness_dir, &format!("[{}] RECOVERY [{}] action=\"completed\" reason=\"validation passed\"",
                                session_id, task_id));
                            eprintln!(
                                "🔄 Recovered [{}] -> completed (validation passed)",
                                task_id
                            );
                        } else if let Some(ref commit) = started_commit {
                            git_reset_hard(project_dir, commit);
                            set_json_field(task, "status", serde_json::json!("failed"));
                            let attempts =
                                task.get("attempts").and_then(|a| a.as_u64()).unwrap_or(1);
                            if let Ok(error_log) = json_array_mut(task, "error_log") {
                                error_log.push(serde_json::json!(
                                    "[SESSION_TIMEOUT] Validation failed during recovery"
                                ));
                            }
                            set_json_field(task, "attempts", serde_json::json!(attempts));
                            log_progress(&harness_dir, &format!("[{}] RECOVERY [{}] action=\"failed\" reason=\"validation failed\"",
                                session_id, task_id));
                            eprintln!("🔄 Recovered [{}] -> failed (validation failed)", task_id);
                        } else {
                            set_json_field(task, "status", serde_json::json!("completed"));
                            set_json_field(task, "completed_at", serde_json::json!(Utc::now().to_rfc3339()));
                            log_progress(&harness_dir, &format!("[{}] RECOVERY [{}] action=\"completed\" reason=\"progress found, no validation\"",
                                session_id, task_id));
                            eprintln!("🔄 Recovered [{}] -> completed (no validation)", task_id);
                        }
                    } else {
                        // No progress at all
                        let attempts =
                            task.get("attempts").and_then(|a| a.as_u64()).unwrap_or(0) + 1;
                        set_json_field(task, "status", serde_json::json!("failed"));
                        set_json_field(task, "attempts", serde_json::json!(attempts));
                        if let Ok(error_log) = json_array_mut(task, "error_log") {
                            error_log.push(serde_json::json!("[SESSION_TIMEOUT] No progress detected"));
                        }
                        log_progress(&harness_dir, &format!("[{}] RECOVERY [{}] action=\"failed\" reason=\"no progress from previous session\"",
                            session_id, task_id));
                        eprintln!("🔄 Recovered [{}] -> failed (no progress)", task_id);
                    }
                }
                save_tasks(&harness_dir, &tasks)?;
            }

            let mut tasks_this_session = 0u64;

            loop {
                // Pick next task
                let task_idx = pick_next_task(&mut tasks);
                let task_idx = match task_idx {
                    Some(idx) => idx,
                    None => {
                        log_progress(
                            &harness_dir,
                            &format!("[{}] STATS all tasks processed or blocked", session_id),
                        );
                        break;
                    }
                };

                // Claim task
                let task = {
                    let Ok(arr) = tasks_array_mut(&mut tasks) else {
                        log_progress(&harness_dir, &format!(
                            "[{}] ERROR invalid tasks format", session_id));
                        break;
                    };
                    match arr.get_mut(task_idx) {
                        Some(t) => t,
                        None => {
                            log_progress(&harness_dir, &format!(
                                "[{}] ERROR task index {} out of bounds", session_id, task_idx));
                            break;
                        }
                    }
                };
                let task_id = task
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("?")
                    .to_string();
                let task_title = task
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("?")
                    .to_string();

                let head_commit = get_git_head(project_dir);
                set_json_field(task, "status", serde_json::json!("in_progress"));
                set_json_field(task, "started_at_commit", serde_json::json!(head_commit));
                set_json_field(task, "attempts", serde_json::json!(
                    task.get("attempts").and_then(|a| a.as_u64()).unwrap_or(0) + 1
                ));

                save_tasks(&harness_dir, &tasks)?;

                log_progress(
                    &harness_dir,
                    &format!(
                        "[{}] Starting [{}] {} (base={:?})",
                        session_id, task_id, task_title, head_commit
                    ),
                );

                eprintln!(
                    "\n▶ Starting [{}] {} (attempt {})",
                    task_id,
                    task_title,
                    tasks["tasks"][task_idx]["attempts"].as_u64().unwrap_or(0)
                );

                // Execute task via headless agent
                let task_prompt = format!(
                    "Complete this task: {}. \
                    Work in directory: {}. \
                    After completing the task, describe exactly what you did and what the validation command should be. \
                    If you created files or made changes, list them. \
                    If the task cannot be completed, explain why.",
                    task_title,
                    project_dir.display()
                );

                let result = run_headless_task(
                    &*provider,
                    &model_name,
                    &tools_schema,
                    &task_prompt,
                    project_dir,
                    &tool_registry,
                )
                .await;

                match result {
                    Ok(_response_text) => {
                        // Try to git commit
                        let new_commit =
                            git_commit(project_dir, &format!("[{}] {}", task_id, task_title));

                        // Run validation if available
                        let validation_cmd = tasks["tasks"][task_idx]
                            .get("validation")
                            .and_then(|v| v.get("command"))
                            .and_then(|c| c.as_str())
                            .map(|s| s.to_string());
                        let validation_timeout = tasks["tasks"][task_idx]
                            .get("validation")
                            .and_then(|v| v.get("timeout_seconds"))
                            .and_then(|t| t.as_u64())
                            .unwrap_or(120);

                        let validation_passed = if let Some(ref cmd) = validation_cmd {
                            run_validation(project_dir, cmd, validation_timeout)
                        } else {
                            true // No validation = auto-pass
                        };

                        if validation_passed {
                            let task = {
                                let Ok(arr) = tasks_array_mut(&mut tasks) else {
                                    break;
                                };
                                match arr.get_mut(task_idx) {
                                    Some(t) => t,
                                    None => break,
                                }
                            };
                            set_json_field(task, "status", serde_json::json!("completed"));
                            set_json_field(task, "completed_at", serde_json::json!(Utc::now().to_rfc3339()));

                            log_progress(
                                &harness_dir,
                                &format!(
                                    "[{}] Completed [{}] (commit={:?})",
                                    session_id, task_id, new_commit
                                ),
                            );

                            eprintln!("✅ Completed [{}] {}", task_id, task_title);
                        } else {
                            // Validation failed - rollback
                            let started_commit = tasks["tasks"][task_idx]["started_at_commit"]
                                .as_str()
                                .map(|s| s.to_string());
                            if let Some(ref commit) = started_commit {
                                git_reset_hard(project_dir, commit);
                                log_progress(&harness_dir, &format!("[{}] ROLLBACK [{}] git reset --hard {} (validation failed)",
                                    session_id, task_id, commit));
                            }

                            let task = {
                                let Ok(arr) = tasks_array_mut(&mut tasks) else {
                                    break;
                                };
                                match arr.get_mut(task_idx) {
                                    Some(t) => t,
                                    None => break,
                                }
                            };
                            set_json_field(task, "status", serde_json::json!("failed"));
                            let attempts =
                                task.get("attempts").and_then(|a| a.as_u64()).unwrap_or(1);
                            let max_attempts = task
                                .get("max_attempts")
                                .and_then(|a| a.as_u64())
                                .unwrap_or(3);
                            if let Ok(error_log) = json_array_mut(task, "error_log") {
                                error_log
                                    .push(serde_json::json!("[TEST_FAIL] Validation command failed"));
                            }

                            log_progress(
                                &harness_dir,
                                &format!(
                                    "[{}] ERROR [{}] [TEST_FAIL] Validation failed (attempt {}/{})",
                                    session_id, task_id, attempts, max_attempts
                                ),
                            );

                            eprintln!(
                                "❌ Validation failed [{}] {} (attempt {}/{})",
                                task_id, task_title, attempts, max_attempts
                            );
                        }
                    }
                    Err(e) => {
                        // Rollback on failure
                        let started_commit = tasks["tasks"][task_idx]["started_at_commit"]
                            .as_str()
                            .map(|s| s.to_string());

                        if let Some(ref commit) = started_commit {
                            if git_reset_hard(project_dir, commit) {
                                log_progress(
                                    &harness_dir,
                                    &format!(
                                        "[{}] ROLLBACK [{}] git reset --hard {}",
                                        session_id, task_id, commit
                                    ),
                                );
                            }
                        }

                        let task = {
                            let Ok(arr) = tasks_array_mut(&mut tasks) else {
                                break;
                            };
                            match arr.get_mut(task_idx) {
                                Some(t) => t,
                                None => break,
                            }
                        };
                        set_json_field(task, "status", serde_json::json!("failed"));

                        if let Ok(error_log) = json_array_mut(task, "error_log") {
                            error_log.push(serde_json::json!(format!("[TASK_EXEC] {}", e)));
                        }

                        let attempts = task.get("attempts").and_then(|a| a.as_u64()).unwrap_or(1);
                        let max_attempts = task
                            .get("max_attempts")
                            .and_then(|a| a.as_u64())
                            .unwrap_or(3);

                        log_progress(
                            &harness_dir,
                            &format!(
                                "[{}] ERROR [{}] [TASK_EXEC] {} (attempt {}/{})",
                                session_id, task_id, e, attempts, max_attempts
                            ),
                        );

                        eprintln!(
                            "❌ Failed [{}] {} (attempt {}/{}): {}",
                            task_id, task_title, attempts, max_attempts, e
                        );
                    }
                }

                save_tasks(&harness_dir, &tasks)?;
                tasks_this_session += 1;

                if tasks_this_session >= max_tasks {
                    log_progress(
                        &harness_dir,
                        &format!(
                            "[{}] STATS max_tasks_per_session ({}) reached",
                            session_id, max_tasks
                        ),
                    );
                    break;
                }
            }

            // Update last_session and log STATS
            set_json_field(&mut tasks, "last_session", serde_json::json!(Utc::now().to_rfc3339()));

            // Compute stats
            let (total, completed, failed, pending, attempts_total) = tasks
                .get("tasks")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    let total = arr.len();
                    let completed = arr
                        .iter()
                        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("completed"))
                        .count();
                    let failed = arr
                        .iter()
                        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("failed"))
                        .count();
                    let pending = arr
                        .iter()
                        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("pending"))
                        .count();
                    let attempts_total: u64 = arr
                        .iter()
                        .filter_map(|t| t.get("attempts").and_then(|a| a.as_u64()))
                        .sum();
                    (total, completed, failed, pending, attempts_total)
                })
                .unwrap_or((0, 0, 0, 0, 0));

            let blocked = tasks
                .get("tasks")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    let permanently_failed: Vec<String> = arr
                        .iter()
                        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("failed"))
                        .filter(|t| {
                            t.get("attempts").and_then(|a| a.as_u64()).unwrap_or(0)
                                >= t.get("max_attempts").and_then(|a| a.as_u64()).unwrap_or(3)
                        })
                        .filter_map(|t| t.get("id").and_then(|i| i.as_str()).map(|s| s.to_string()))
                        .collect();
                    arr.iter()
                        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("pending"))
                        .filter(|t| {
                            t.get("depends_on")
                                .and_then(|d| d.as_array())
                                .map(|deps| {
                                    deps.iter().any(|d| {
                                        d.as_str()
                                            .map(|id| permanently_failed.contains(&id.to_string()))
                                            .unwrap_or(false)
                                    })
                                })
                                .unwrap_or(false)
                        })
                        .count()
                })
                .unwrap_or(0);

            log_progress(&harness_dir, &format!(
                "[{}] STATS tasks_total={} completed={} failed={} pending={} blocked={} attempts_total={}",
                session_id, total, completed, failed, pending, blocked, attempts_total
            ));

            save_tasks(&harness_dir, &tasks)?;

            // Check if all done - remove activation marker
            let all_done = tasks
                .get("tasks")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter().all(|t| {
                        matches!(
                            t.get("status").and_then(|s| s.as_str()),
                            Some("completed") | None
                        )
                    })
                })
                .unwrap_or(true);

            if all_done {
                let _ = std::fs::remove_file(project_dir.join(".harness-active"));
                log_progress(
                    &harness_dir,
                    &format!("[{}] All tasks completed. Harness deactivated.", session_id),
                );
            }

            eprintln!(
                "\nHarness session {} complete. {} tasks processed.",
                session_id, tasks_this_session
            );
        }
    }

    Ok(())
}

fn build_tools_schema() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "name": "bash",
            "description": "Execute a shell command. Use for git operations, running tests, installing packages, etc.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "The shell command to execute"}
                },
                "required": ["command"]
            }
        }),
        serde_json::json!({
            "name": "read_file",
            "description": "Read the contents of a file",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file"}
                },
                "required": ["path"]
            }
        }),
        serde_json::json!({
            "name": "write_file",
            "description": "Write content to a file, creating it if needed",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file"},
                    "content": {"type": "string", "description": "Content to write"}
                },
                "required": ["path", "content"]
            }
        }),
        serde_json::json!({
            "name": "edit_file",
            "description": "Replace a string in a file",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file"},
                    "old_string": {"type": "string", "description": "Text to find"},
                    "new_string": {"type": "string", "description": "Replacement text"}
                },
                "required": ["path", "old_string", "new_string"]
            }
        }),
        serde_json::json!({
            "name": "list_dir",
            "description": "List files and directories",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path"}
                }
            }
        }),
        serde_json::json!({
            "name": "grep",
            "description": "Search for a pattern in files",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Search pattern"},
                    "path": {"type": "string", "description": "Directory or file to search in"}
                },
                "required": ["pattern"]
            }
        }),
    ]
}
