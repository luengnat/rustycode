//! Plan Management Tools - Create, save, load, and manage plans
//!
//! This module provides tools for managing plans throughout their lifecycle,
//! including creating plans from templates, saving/loading plans, and approving
//! plans for execution.

use super::plan_templates::PlanTemplate;
use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use rustycode_protocol::SessionId;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

use std::time::SystemTime;

/// Create a plan from a template
pub struct CreatePlanFromTemplateTool;

impl Tool for CreatePlanFromTemplateTool {
    fn name(&self) -> &str {
        "create_plan_from_template"
    }

    fn description(&self) -> &str {
        r#"Create a new plan from a predefined template.

**Use cases:**
- Quickly create plans for common development tasks
- Use proven plan structures
- Save time on planning

**Parameters:**
- `template`: Template type (new_feature, bug_fix, refactor, add_tests, performance, documentation, security_fix, dependency_update)
- `task`: Description of the specific task
- `summary`: One-line summary of the plan
- `files`: Array of files that will be modified (optional)

**Returns:**
- Plan ID and summary of the created plan

**Example:**
```json
{
  "template": "bug_fix",
  "task": "Fix authentication bug in login flow",
  "summary": "Fix the bug where users cannot login with valid credentials",
  "files": ["src/auth.rs", "src/login.rs"]
}
```

**Available Templates:**
- `new_feature`: Implement a new feature from scratch
- `bug_fix`: Fix a reported bug
- `refactor`: Refactor code for better structure
- `add_tests`: Add test coverage
- `performance`: Optimize performance
- `documentation`: Add or update documentation
- `security_fix`: Fix security vulnerability
- `dependency_update`: Update dependencies"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["template", "task", "summary"],
            "properties": {
                "template": {
                    "type": "string",
                    "enum": ["new_feature", "bug_fix", "refactor", "add_tests", "performance", "documentation", "security_fix", "dependency_update"],
                    "description": "Type of plan template to use"
                },
                "task": {
                    "type": "string",
                    "description": "Description of the specific task"
                },
                "summary": {
                    "type": "string",
                    "description": "One-line summary of the plan"
                },
                "files": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Files that will be modified (optional)",
                    "default": []
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let template_str = required_string(&params, "template")?;
        let task = required_string(&params, "task")?;
        let summary = required_string(&params, "summary")?;
        let files = params
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Parse template type
        let template = match template_str {
            "new_feature" => PlanTemplate::NewFeature,
            "bug_fix" => PlanTemplate::BugFix,
            "refactor" => PlanTemplate::Refactor,
            "add_tests" => PlanTemplate::AddTests,
            "performance" => PlanTemplate::Performance,
            "documentation" => PlanTemplate::Documentation,
            "security_fix" => PlanTemplate::SecurityFix,
            "dependency_update" => PlanTemplate::DependencyUpdate,
            _ => return Err(anyhow!("unknown template type: {}", template_str)),
        };

        // Create plan from template
        let plan = template.create_plan(
            SessionId::new(),
            task.to_string(),
            summary.to_string(),
            files,
        );

        // Format output
        let mut output = String::new();
        output.push_str("**Plan Created from Template**\n\n");
        output.push_str(&format!("**Plan ID:** {}\n", plan.id));
        output.push_str(&format!("**Template:** {}\n", template_str));
        output.push_str(&format!("**Summary:** {}\n\n", plan.summary));
        output.push_str(&format!("**Steps:** {} steps\n", plan.steps.len()));
        output.push_str(&format!(
            "**Risks:** {} potential risks identified\n\n",
            plan.risks.len()
        ));

        output.push_str("**Plan Overview:**\n```\n");
        for (i, step) in plan.steps.iter().enumerate() {
            output.push_str(&format!("{}. {}\n", i + 1, step.title));
        }
        output.push_str("```\n\n");

        output.push_str("**Next Steps:**\n");
        output.push_str("1. Review the plan steps\n");
        output.push_str("2. Customize steps if needed\n");
        output.push_str("3. Save the plan with save_plan tool\n");
        output.push_str("4. Execute the plan when ready\n");

        let metadata = json!({
            "plan_id": plan.id.to_string(),
            "template": template_str,
            "summary": plan.summary,
            "steps_count": plan.steps.len(),
            "risks_count": plan.risks.len()
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Save a plan to disk
pub struct SavePlanTool;

impl Tool for SavePlanTool {
    fn name(&self) -> &str {
        "save_plan"
    }

    fn description(&self) -> &str {
        r#"Save a plan to disk for later use.

**Use cases:**
- Persist a plan for later execution
- Share plans with team members
- Maintain plan history

**Parameters:**
- `plan_id`: Plan ID (from create_plan_from_template)
- `file_path`: Where to save the plan (default: .rustycode/plans/<plan_id>.json)

**Note:** Currently plans are saved in JSON format. The plan_id parameter
is used to track the plan but in a real implementation, you would pass
the full plan object. This is a simplified version.

**Example:**
```json
{
  "plan_id": "plan-abc123",
  "file_path": "plans/feature-auth.json"
}
```"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["plan_id"],
            "properties": {
                "plan_id": {
                    "type": "string",
                    "description": "Plan ID to save"
                },
                "file_path": {
                    "type": "string",
                    "description": "Where to save the plan (optional)"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let plan_id_str = required_string(&params, "plan_id")?;
        let default_path = format!(".rustycode/plans/{}.json", plan_id_str);
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or(&default_path);

        // Resolve file path
        let path = if PathBuf::from(file_path).is_absolute() {
            PathBuf::from(file_path)
        } else {
            ctx.cwd.join(file_path)
        };

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        // Create a placeholder plan (in real implementation, you'd get this from state)
        let plan_data = json!({
            "id": plan_id_str,
            "saved_at": Utc::now().to_rfc3339(),
            "note": "This is a placeholder. In a real implementation, the plan would be retrieved from state."
        });

        // Write plan to file
        fs::write(&path, serde_json::to_string_pretty(&plan_data)?)
            .with_context(|| format!("failed to write plan to: {}", path.display()))?;

        let output = format!(
            "**Plan Saved**\n\n✅ Plan ID `{}` saved to: `{}`\n\n\
            **Next Steps:**\n\
            - Load the plan later with load_plan\n\
            - Execute the plan when ready",
            plan_id_str,
            path.display()
        );

        let metadata = json!({
            "plan_id": plan_id_str,
            "file_path": path.to_string_lossy().to_string(),
            "saved_at": Utc::now().to_rfc3339()
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Load a plan from disk
pub struct LoadPlanTool;

impl Tool for LoadPlanTool {
    fn name(&self) -> &str {
        "load_plan"
    }

    fn description(&self) -> &str {
        r#"Load a saved plan from disk.

**Use cases:**
- Load a previously saved plan
- Resume planning from earlier session
- Review existing plans

**Parameters:**
- `file_path`: Path to the plan file

**Example:**
```json
{
  "file_path": "plans/feature-auth.json"
}
```"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the plan file"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let file_path_str = required_string(&params, "file_path")?;

        // Resolve file path
        let path = if PathBuf::from(file_path_str).is_absolute() {
            PathBuf::from(file_path_str)
        } else {
            ctx.cwd.join(file_path_str)
        };

        // Check if file exists
        if !path.exists() {
            return Err(anyhow!("plan file not found: {}", path.display()));
        }

        // Read plan file
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read plan from: {}", path.display()))?;

        // Parse JSON
        let plan_data: Value = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse plan JSON from: {}", path.display()))?;

        let plan_id = plan_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let saved_at = plan_data
            .get("saved_at")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let output = format!(
            "**Plan Loaded**\n\n✅ Plan loaded from: `{}`\n\n\
            **Plan ID:** `{}`\n\
            **Saved At:** `{}`\n\n\
            **Next Steps:**\n\
            - Review the plan contents\n\
            - Execute the plan when ready",
            path.display(),
            plan_id,
            saved_at
        );

        let metadata = json!({
            "file_path": path.to_string_lossy().to_string(),
            "plan_data": plan_data
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// List available plans
pub struct ListPlansTool;

impl Tool for ListPlansTool {
    fn name(&self) -> &str {
        "list_plans"
    }

    fn description(&self) -> &str {
        r#"List all available saved plans.

**Use cases:**
- See what plans are available
- Find plans to load or execute
- Manage plan collection

**Parameters:**
- `directory`: Directory to search for plans (default: .rustycode/plans)

**Example:**
```json
{
  "directory": "plans"
}
```

**Returns:**
- List of plan files with metadata"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "directory": {
                    "type": "string",
                    "description": "Directory to search for plans (default: .rustycode/plans)",
                    "default": ".rustycode/plans"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let directory = params
            .get("directory")
            .and_then(|v| v.as_str())
            .unwrap_or(".rustycode/plans");

        // Resolve directory path
        let dir_path = if PathBuf::from(directory).is_absolute() {
            PathBuf::from(directory)
        } else {
            ctx.cwd.join(directory)
        };

        // Check if directory exists
        if !dir_path.exists() {
            return Ok(ToolOutput::text(
                "**No Plans Found**\n\nNo plans directory found. Create a plan first with create_plan_from_template."
            ));
        }

        // Find all plan files
        let mut plans = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(name) =
                            path.strip_prefix(&ctx.cwd).map(|p| p.display().to_string())
                        {
                            plans.push((name, metadata.modified().ok()));
                        }
                    }
                }
            }
        }

        if plans.is_empty() {
            return Ok(ToolOutput::text(
                "**No Plans Found**\n\nNo plan files found in the plans directory.",
            ));
        }

        // Sort by modification time (newest first)
        plans.sort_by(|a, b| {
            b.1.unwrap_or_else(SystemTime::now)
                .cmp(&a.1.unwrap_or_else(SystemTime::now))
        });

        // Format output
        let mut output = String::new();
        output.push_str("**Available Plans**\n\n");
        output.push_str(&format!("Found {} plan(s):\n\n", plans.len()));

        for (i, (path, modified)) in plans.iter().enumerate() {
            output.push_str(&format!("{}. `{}`\n", i + 1, path));
            if let Some(mtime) = modified {
                let datetime: DateTime<Utc> = (*mtime).into();
                let formatted_time = datetime.format("%Y-%m-%d %H:%M");
                output.push_str(&format!("   Modified: {}\n", formatted_time));
            }
            output.push('\n');
        }

        output.push_str("**Next Steps:**\n");
        output.push_str("- Load a plan with load_plan\n");
        output.push_str("- Execute a plan directly");

        let metadata = json!({
            "plans_count": plans.len(),
            "plans": plans.iter().map(|(p, _)| p).collect::<Vec<_>>()
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Approve a plan for execution
pub struct ApprovePlanTool;

impl Tool for ApprovePlanTool {
    fn name(&self) -> &str {
        "approve_plan"
    }

    fn description(&self) -> &str {
        r#"Approve a plan for execution.

**Use cases:**
- Mark a plan as ready to execute
- User confirmation before execution
- Prevent accidental plan execution

**Parameters:**
- `plan_id`: Plan ID to approve

**Example:**
```json
{
  "plan_id": "plan-abc123"
}
```

**Returns:**
- Confirmation of plan approval

**Note:** This is a planning tool. In a real implementation, this would
update the plan status in a plan store and potentially trigger user
confirmation prompts."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["plan_id"],
            "properties": {
                "plan_id": {
                    "type": "string",
                    "description": "Plan ID to approve"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let plan_id = required_string(&params, "plan_id")?;

        let output = format!(
            "**Plan Approved**\n\n✅ Plan `{}` has been approved for execution.\n\n\
            **Status:** Ready to execute\n\
            **Next Steps:**\n\
            - Execute the plan when ready\n\
            - Or make additional changes before execution",
            plan_id
        );

        let metadata = json!({
            "plan_id": plan_id,
            "status": "approved",
            "approved_at": Utc::now().to_rfc3339()
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Helper function to get a required string parameter
fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string parameter `{}`", key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_plan_tool_metadata() {
        let tool = CreatePlanFromTemplateTool;
        assert_eq!(tool.name(), "create_plan_from_template");
        assert!(tool.description().contains("template"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_create_plan_parameters_schema() {
        let tool = CreatePlanFromTemplateTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 3);
        assert!(required.contains(&json!("template")));
        assert!(required.contains(&json!("task")));
        assert!(required.contains(&json!("summary")));

        // Check template enum
        let template_enum = schema["properties"]["template"]["enum"].as_array().unwrap();
        assert!(template_enum.contains(&json!("new_feature")));
        assert!(template_enum.contains(&json!("bug_fix")));
    }

    #[test]
    fn test_save_plan_tool_metadata() {
        let tool = SavePlanTool;
        assert_eq!(tool.name(), "save_plan");
        assert!(tool.description().contains("save"));
        assert_eq!(tool.permission(), ToolPermission::Write);
    }

    #[test]
    fn test_load_plan_tool_metadata() {
        let tool = LoadPlanTool;
        assert_eq!(tool.name(), "load_plan");
        assert!(tool.description().contains("Load"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_list_plans_tool_metadata() {
        let tool = ListPlansTool;
        assert_eq!(tool.name(), "list_plans");
        assert!(tool.description().contains("List"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_approve_plan_tool_metadata() {
        let tool = ApprovePlanTool;
        assert_eq!(tool.name(), "approve_plan");
        assert!(tool.description().contains("approve"));
        assert_eq!(tool.permission(), ToolPermission::Read);
    }
}
