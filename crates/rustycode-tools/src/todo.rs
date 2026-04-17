//! TodoWrite tool for task planning and tracking
//!
//! This tool allows the LLM to create and manage todo lists, forcing planning
//! and giving users visibility into progress.

use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::Result;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// Todo item status
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// A single todo item
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub title: String,
    pub status: TodoStatus,
}

/// Shared todo state (accessible by TUI for display)
pub type TodoState = Arc<Mutex<Vec<TodoItem>>>;

/// Create a new shared todo state
pub fn new_todo_state() -> TodoState {
    Arc::new(Mutex::new(Vec::new()))
}

/// TodoWrite tool - Create/update todo list
pub struct TodoWriteTool {
    pub state: TodoState,
}

impl TodoWriteTool {
    pub fn new(state: TodoState) -> Self {
        Self { state }
    }
}

impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        r#"Create or update a todo list to plan and track tasks. Use this BEFORE starting work on complex tasks.

Parameters:
- title (string): Title for this todo list (e.g., "Fix build errors")
- todos (array): Array of todo items, each with:
  - id (string): Unique identifier
  - title (string): Task description
  - status (string): One of: pending, in_progress, completed

Example:
{
  "title": "Fix build errors",
  "todos": [
    {"id": "1", "title": "Run build", "status": "pending"},
    {"id": "2", "title": "Fix error 1", "status": "pending"},
    {"id": "3", "title": "Fix error 2", "status": "pending"}
  ]
}"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None // No special permissions needed
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Title for this todo list"
                },
                "todos": {
                    "type": "array",
                    "description": "Array of todo items",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Unique identifier for this todo"
                            },
                            "title": {
                                "type": "string",
                                "description": "Task description"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Current status"
                            }
                        },
                        "required": ["id", "title", "status"]
                    }
                }
            },
            "required": ["title", "todos"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let title = params["title"]
            .as_str()
            .ok_or(anyhow::anyhow!("Missing title"))?;
        let todos_input = params["todos"]
            .as_array()
            .ok_or(anyhow::anyhow!("Missing todos"))?;

        let mut todos = Vec::new();
        for item in todos_input {
            let id = item["id"].as_str().ok_or(anyhow::anyhow!("Missing id"))?;
            let item_title = item["title"]
                .as_str()
                .ok_or(anyhow::anyhow!("Missing title"))?;
            let status_str = item["status"]
                .as_str()
                .ok_or(anyhow::anyhow!("Missing status"))?;

            let status = match status_str {
                "pending" => TodoStatus::Pending,
                "in_progress" => TodoStatus::InProgress,
                "completed" => TodoStatus::Completed,
                _ => return Err(anyhow::anyhow!("Invalid status: {}", status_str)),
            };

            todos.push(TodoItem {
                id: id.to_string(),
                title: item_title.to_string(),
                status,
            });
        }

        // Update shared state
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *state = todos;

        let completed_count = state
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Completed))
            .count();
        let in_progress_count = state
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::InProgress))
            .count();

        let output = format!(
            "Todo list '{}' updated:\n- {} items total\n- {} completed\n- {} in progress",
            title,
            state.len(),
            completed_count,
            in_progress_count
        );

        Ok(ToolOutput::text(output))
    }
}

/// TodoUpdate tool - Update single todo item status
pub struct TodoUpdateTool {
    pub state: TodoState,
}

impl TodoUpdateTool {
    pub fn new(state: TodoState) -> Self {
        Self { state }
    }
}

impl Tool for TodoUpdateTool {
    fn name(&self) -> &str {
        "todo_update"
    }

    fn description(&self) -> &str {
        r#"Update the status of a single todo item.

Parameters:
- id (string): Todo item identifier
- status (string): New status: pending, in_progress, or completed

Example:
{
  "id": "1",
  "status": "completed"
}"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Todo item identifier"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed"],
                    "description": "New status"
                }
            },
            "required": ["id", "status"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let id = params["id"].as_str().ok_or(anyhow::anyhow!("Missing id"))?;
        let status_str = params["status"]
            .as_str()
            .ok_or(anyhow::anyhow!("Missing status"))?;

        let new_status = match status_str {
            "pending" => TodoStatus::Pending,
            "in_progress" => TodoStatus::InProgress,
            "completed" => TodoStatus::Completed,
            _ => return Err(anyhow::anyhow!("Invalid status: {}", status_str)),
        };

        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let item = state
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("Todo item not found: {}", id))?;

        let old_status = std::mem::replace(&mut item.status, new_status);

        Ok(ToolOutput::text(format!(
            "Todo '{}': {} → {}",
            id,
            format_status(old_status),
            format_status(new_status)
        )))
    }
}

fn format_status(status: TodoStatus) -> String {
    match status {
        TodoStatus::Pending => "pending".to_string(),
        TodoStatus::InProgress => "in progress".to_string(),
        TodoStatus::Completed => "completed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todo_write() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state.clone());

        let params = json!({
            "title": "Test todos",
            "todos": [
                {"id": "1", "title": "Task 1", "status": "pending"},
                {"id": "2", "title": "Task 2", "status": "in_progress"}
            ]
        });

        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(params, &ctx).unwrap();

        assert!(result.text.contains("Test todos"));
        assert!(result.text.contains("2 items total"));

        let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(state_guard.len(), 2);
        assert_eq!(state_guard[0].title, "Task 1");
    }

    #[test]
    fn test_todo_update() {
        let state = new_todo_state();
        let write_tool = TodoWriteTool::new(state.clone());

        // Create todos
        let params = json!({
            "title": "Test",
            "todos": [
                {"id": "1", "title": "Task 1", "status": "pending"}
            ]
        });

        let ctx = ToolContext::new("/tmp");
        write_tool.execute(params, &ctx).unwrap();

        // Update todo
        let update_tool = TodoUpdateTool::new(state.clone());
        let update_params = json!({
            "id": "1",
            "status": "completed"
        });

        let result = update_tool.execute(update_params, &ctx).unwrap();
        assert!(result.text.contains("completed"));

        let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert!(matches!(state_guard[0].status, TodoStatus::Completed));
    }

    // --- TodoStatus ---

    #[test]
    fn todo_status_serde_roundtrip() {
        for status in [
            TodoStatus::Pending,
            TodoStatus::InProgress,
            TodoStatus::Completed,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let decoded: TodoStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, decoded);
        }
    }

    #[test]
    fn todo_status_rename_lowercase() {
        assert_eq!(
            serde_json::to_string(&TodoStatus::InProgress).unwrap(),
            "\"inprogress\""
        );
        assert_eq!(
            serde_json::to_string(&TodoStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&TodoStatus::Completed).unwrap(),
            "\"completed\""
        );
    }

    // --- TodoItem ---

    #[test]
    fn todo_item_serde_roundtrip() {
        let item = TodoItem {
            id: "42".into(),
            title: "Fix bug".into(),
            status: TodoStatus::InProgress,
        };
        let json = serde_json::to_string(&item).unwrap();
        let decoded: TodoItem = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "42");
        assert_eq!(decoded.title, "Fix bug");
        assert_eq!(decoded.status, TodoStatus::InProgress);
    }

    // --- new_todo_state ---

    #[test]
    fn new_todo_state_is_empty() {
        let state = new_todo_state();
        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert!(guard.is_empty());
    }

    // --- TodoWriteTool edge cases ---

    #[test]
    fn write_missing_title_errors() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({"todos": []}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn write_missing_todos_errors() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({"title": "X"}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn write_invalid_status_errors() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(
            json!({
                "title": "T",
                "todos": [{"id": "1", "title": "A", "status": "bogus"}]
            }),
            &ctx,
        );
        assert!(result.is_err());
    }

    #[test]
    fn write_empty_todos_clears_state() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state.clone());
        let ctx = ToolContext::new("/tmp");

        // Write some items first
        tool.execute(
            json!({
                "title": "Fill",
                "todos": [{"id": "1", "title": "A", "status": "pending"}]
            }),
            &ctx,
        )
        .unwrap();
        assert_eq!(state.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);

        // Overwrite with empty
        tool.execute(json!({"title": "Clear", "todos": []}), &ctx)
            .unwrap();
        assert!(state.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    }

    // --- TodoUpdateTool edge cases ---

    #[test]
    fn update_nonexistent_id_errors() {
        let state = new_todo_state();
        let tool = TodoUpdateTool::new(state);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({"id": "missing", "status": "completed"}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn update_shows_transition() {
        let state = new_todo_state();
        let write = TodoWriteTool::new(state.clone());
        let ctx = ToolContext::new("/tmp");
        write
            .execute(
                json!({
                    "title": "T",
                    "todos": [{"id": "1", "title": "A", "status": "pending"}]
                }),
                &ctx,
            )
            .unwrap();

        let update = TodoUpdateTool::new(state);
        let result = update
            .execute(json!({"id": "1", "status": "in_progress"}), &ctx)
            .unwrap();
        assert!(result.text.contains("pending"));
        assert!(result.text.contains("in progress"));
    }

    // --- format_status ---

    #[test]
    fn format_status_values() {
        assert_eq!(format_status(TodoStatus::Pending), "pending");
        assert_eq!(format_status(TodoStatus::InProgress), "in progress");
        assert_eq!(format_status(TodoStatus::Completed), "completed");
    }

    // --- Tool metadata ---

    #[test]
    fn tool_names() {
        let state = new_todo_state();
        assert_eq!(TodoWriteTool::new(state.clone()).name(), "todo_write");
        assert_eq!(TodoUpdateTool::new(state).name(), "todo_update");
    }

    #[test]
    fn tool_permissions() {
        let state = new_todo_state();
        assert_eq!(
            TodoWriteTool::new(state.clone()).permission(),
            ToolPermission::None
        );
        assert_eq!(
            TodoUpdateTool::new(state).permission(),
            ToolPermission::None
        );
    }
}
