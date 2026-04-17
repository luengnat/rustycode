//! Task and Todo management for the TUI.
//!
//! This module provides data structures and persistence for:
//! - Tasks (with status tracking: Pending, InProgress, Completed, Blocked)
//! - Todos (simple checklist items)
//! - Active agents (background agent processes)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

// ── Data Structures ───────────────────────────────────────────────────────

/// A task with status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: SystemTime,
    pub dependencies: Vec<String>,
}

/// Status of a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

/// A simple todo item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub text: String,
    pub done: bool,
    pub created_at: SystemTime,
}

/// An active agent process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveAgent {
    pub id: String,
    pub task: String,
    pub status: AgentStatus,
    pub created_at: SystemTime,
}

/// Status of an agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum AgentStatus {
    Starting,
    Running,
    Completed,
    Failed,
    Killed,
}

/// Container for all workspace tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTasks {
    pub tasks: Vec<Task>,
    pub todos: Vec<Todo>,
    pub active_agents: Vec<ActiveAgent>,
}

// ── File Management ───────────────────────────────────────────────────────

// Thread-local override for tasks path (used by tests)
#[cfg(test)]
thread_local! {
    static TEST_TASKS_PATH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Set a thread-local override for the tasks file path (tests only)
#[cfg(test)]
pub fn set_test_tasks_path(path: Option<PathBuf>) {
    TEST_TASKS_PATH.with(|p| *p.borrow_mut() = path);
}

/// Get the path to the tasks file
pub fn tasks_path() -> PathBuf {
    #[cfg(test)]
    {
        let override_path = TEST_TASKS_PATH.with(|p| p.borrow().clone());
        if let Some(path) = override_path {
            return path;
        }
    }
    PathBuf::from(".rustycode/tasks.json")
}

/// Load tasks from disk
pub fn load_tasks() -> WorkspaceTasks {
    let path = tasks_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            match serde_json::from_str::<WorkspaceTasks>(&content) {
                Ok(tasks) => {
                    tracing::debug!(
                        "Loaded {} tasks, {} todos, {} agents",
                        tasks.tasks.len(),
                        tasks.todos.len(),
                        tasks.active_agents.len()
                    );
                    return tasks;
                }
                Err(e) => {
                    tracing::warn!("Failed to deserialize tasks.json: {}", e);
                    tracing::warn!("Creating new empty tasks state");
                }
            }
        }
    }
    WorkspaceTasks {
        tasks: Vec::new(),
        todos: Vec::new(),
        active_agents: Vec::new(),
    }
}

/// Save tasks to disk atomically (temp file + rename).
///
/// Writes to a temporary file first, then renames it into place.
/// This prevents corruption if the app crashes mid-write.
pub fn save_tasks(tasks: &WorkspaceTasks) -> std::io::Result<()> {
    let path = tasks_path();
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(tasks)?;

    // Atomic write: temp file in same directory, then rename
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, &content)?;
    std::fs::rename(&temp_path, &path)?;

    Ok(())
}

// ── Task Operations ───────────────────────────────────────────────────────

/// Create a new task
pub fn create_task(description: String) -> Task {
    Task {
        id: ulid::Ulid::new().to_string(),
        description,
        status: TaskStatus::Pending,
        created_at: SystemTime::now(),
        dependencies: Vec::new(),
    }
}

/// Update task status
pub fn update_task_status(
    tasks: &mut WorkspaceTasks,
    id: &str,
    status: TaskStatus,
) -> Result<(), String> {
    if let Some(task) = tasks.tasks.iter_mut().find(|t| t.id == id) {
        task.status = status;
        Ok(())
    } else {
        Err(format!("Task {} not found", id))
    }
}

/// Get status icon for a task
pub fn task_status_icon(status: &TaskStatus) -> &str {
    match status {
        TaskStatus::Pending => "⏳",
        TaskStatus::InProgress => "🔄",
        TaskStatus::Completed => "✅",
        TaskStatus::Blocked => "🚫",
    }
}

// ── Todo Operations ───────────────────────────────────────────────────────

/// Create a new todo
pub fn create_todo(text: String) -> Todo {
    Todo {
        id: ulid::Ulid::new().to_string(),
        text,
        done: false,
        created_at: SystemTime::now(),
    }
}

/// Toggle todo completion
pub fn toggle_todo(tasks: &mut WorkspaceTasks, id: &str) -> Result<bool, String> {
    if let Some(todo) = tasks.todos.iter_mut().find(|t| t.id == id) {
        todo.done = !todo.done;
        Ok(todo.done)
    } else {
        Err(format!("Todo {} not found", id))
    }
}

/// Get checkbox icon for a todo
pub fn todo_checkbox(done: bool) -> &'static str {
    if done {
        "☑"
    } else {
        "☐"
    }
}

// ── Agent Operations ──────────────────────────────────────────────────────

/// Create a new agent
pub fn create_agent(task: String) -> ActiveAgent {
    ActiveAgent {
        id: ulid::Ulid::new().to_string(),
        task,
        status: AgentStatus::Starting,
        created_at: SystemTime::now(),
    }
}

/// Update agent status
pub fn update_agent_status(
    tasks: &mut WorkspaceTasks,
    id: &str,
    status: AgentStatus,
) -> Result<(), String> {
    if let Some(agent) = tasks.active_agents.iter_mut().find(|a| a.id == id) {
        agent.status = status;
        Ok(())
    } else {
        Err(format!("Agent {} not found", id))
    }
}

/// Get status icon for an agent
pub fn agent_status_icon(status: &AgentStatus) -> &str {
    match status {
        AgentStatus::Starting => "⚡",
        AgentStatus::Running => "🤖",
        AgentStatus::Completed => "✨",
        AgentStatus::Failed => "💥",
        AgentStatus::Killed => "🗑️",
    }
}

// ── Formatting Helpers ────────────────────────────────────────────────────

/// Format timestamp for display
pub fn format_time(time: SystemTime) -> String {
    use chrono::{DateTime, Local, Utc};

    let datetime: DateTime<Utc> = time.into();
    let datetime: DateTime<Local> = DateTime::from(datetime);
    datetime.format("%H:%M").to_string()
}

/// Format relative time for display
pub fn format_relative_time(time: SystemTime) -> String {
    let now = SystemTime::now();
    let duration = now.duration_since(time).unwrap_or_default();

    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_task() {
        let task = create_task("Test task".to_string());
        assert_eq!(task.description, "Test task");
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(!task.id.is_empty());
    }

    #[test]
    fn test_create_todo() {
        let todo = create_todo("Test todo".to_string());
        assert_eq!(todo.text, "Test todo");
        assert!(!todo.done);
        assert!(!todo.id.is_empty());
    }

    #[test]
    fn test_create_agent() {
        let agent = create_agent("Test agent task".to_string());
        assert_eq!(agent.task, "Test agent task");
        assert_eq!(agent.status, AgentStatus::Starting);
        assert!(!agent.id.is_empty());
    }

    #[test]
    fn test_update_task_status() {
        let mut tasks = WorkspaceTasks {
            tasks: vec![create_task("Test".to_string())],
            todos: Vec::new(),
            active_agents: Vec::new(),
        };
        let id = tasks.tasks[0].id.clone();

        let result = update_task_status(&mut tasks, &id, TaskStatus::Completed);
        assert!(result.is_ok());
        assert_eq!(tasks.tasks[0].status, TaskStatus::Completed);
    }

    #[test]
    fn test_toggle_todo() {
        let mut tasks = WorkspaceTasks {
            tasks: Vec::new(),
            todos: vec![create_todo("Test".to_string())],
            active_agents: Vec::new(),
        };
        let id = tasks.todos[0].id.clone();

        let result = toggle_todo(&mut tasks, &id).unwrap();
        assert!(result);
        assert!(tasks.todos[0].done);
    }

    #[test]
    fn test_status_icons() {
        assert_eq!(task_status_icon(&TaskStatus::Pending), "⏳");
        assert_eq!(task_status_icon(&TaskStatus::InProgress), "🔄");
        assert_eq!(task_status_icon(&TaskStatus::Completed), "✅");
        assert_eq!(task_status_icon(&TaskStatus::Blocked), "🚫");

        assert_eq!(todo_checkbox(false), "☐");
        assert_eq!(todo_checkbox(true), "☑");

        assert_eq!(agent_status_icon(&AgentStatus::Starting), "⚡");
        assert_eq!(agent_status_icon(&AgentStatus::Running), "🤖");
        assert_eq!(agent_status_icon(&AgentStatus::Completed), "✨");
        assert_eq!(agent_status_icon(&AgentStatus::Failed), "💥");
        assert_eq!(agent_status_icon(&AgentStatus::Killed), "🗑️");
    }

    #[test]
    fn test_workspace_tasks_serialization() {
        // Create test data
        let tasks = WorkspaceTasks {
            tasks: vec![create_task("Test task".to_string())],
            todos: vec![create_todo("Test todo".to_string())],
            active_agents: vec![create_agent("Test agent".to_string())],
        };

        // Serialize and deserialize
        let json = serde_json::to_string(&tasks).unwrap();
        let loaded: WorkspaceTasks = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.todos.len(), 1);
        assert_eq!(loaded.active_agents.len(), 1);
        assert_eq!(loaded.tasks[0].description, "Test task");
        assert_eq!(loaded.todos[0].text, "Test todo");
        assert_eq!(loaded.active_agents[0].task, "Test agent");
    }
}
