//! Edit file tool - inline editing capabilities
use crate::security::{create_file_symlink_safe, open_file_symlink_safe, validate_write_path};
use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum size for edit operations to prevent memory issues
const MAX_EDIT_SIZE: usize = 1024 * 1024; // 1 MB

#[derive(Debug, Serialize, Deserialize)]
pub struct EditFileInput {
    pub path: PathBuf,
    pub old_text: String,
    pub new_text: String,
}

pub struct EditFile;

impl Tool for EditFile {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Replace text in a file (inline editing)"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path relative to workspace root"
                },
                "old_text": {
                    "type": "string",
                    "description": "Text to search for and replace"
                },
                "new_text": {
                    "type": "string",
                    "description": "Replacement text"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let input: EditFileInput = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        // Validate path and check size limits
        let path_str = input
            .path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path: contains non-UTF-8 characters"))?;

        // Validate the path is within workspace and safe
        let validated_path = validate_write_path(path_str, &ctx.cwd, input.new_text.len())?;

        // Read the current file content using symlink-safe operation
        let mut file = open_file_symlink_safe(&validated_path)
            .map_err(|e| anyhow::anyhow!("Failed to open file: {}", e))?;
        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content)
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;

        // Check file size for edit operations
        if content.len() > MAX_EDIT_SIZE {
            return Ok(ToolOutput::text(format!(
                "File is too large for inline editing ({} bytes). Use other tools for large files.",
                content.len()
            )));
        }

        // Check if old_text exists (after reading, so no TOCTOU)
        if !content.contains(&input.old_text) {
            return Ok(ToolOutput::text(
                "Old text not found in file. No changes made.".to_string(),
            ));
        }

        // Perform the replacement
        let new_content = content.replace(&input.old_text, &input.new_text);

        // Verify replacement didn't dramatically increase file size
        if new_content.len() > MAX_EDIT_SIZE * 2 {
            return Err(anyhow::anyhow!(
                "Edit would increase file size beyond safe limit"
            ));
        }

        // Write the new content using symlink-safe operation
        let mut file = create_file_symlink_safe(&validated_path)
            .map_err(|e| anyhow::anyhow!("Failed to create file: {}", e))?;
        use std::io::Write;
        file.write_all(new_content.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to write file: {}", e))?;
        file.sync_all()
            .map_err(|e| anyhow::anyhow!("Failed to sync file: {}", e))?;

        Ok(ToolOutput::text(format!(
            "Successfully replaced text in {}",
            input.path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_edit_file_valid_operation() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello world").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "world",
            "new_text": "rust"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        // Verify the file was modified
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hello rust");
    }

    #[test]
    fn test_edit_file_blocks_path_traversal() {
        let workspace = tempdir().unwrap();
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "../../../etc/passwd",
            "old_text": "root",
            "new_text": "hacked"
        });

        let tool = EditFile;
        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_edit_file_respects_size_limits() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "small").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        // Try to write content larger than limit
        let huge_content = "x".repeat(20 * 1024 * 1024); // 20 MB

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "small",
            "new_text": huge_content
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("limit"));
    }

    // --- EditFile metadata ---

    #[test]
    fn edit_file_tool_name() {
        assert_eq!(EditFile.name(), "edit_file");
    }

    #[test]
    fn edit_file_tool_permission() {
        assert_eq!(EditFile.permission(), ToolPermission::Write);
    }

    #[test]
    fn edit_file_schema_has_required_fields() {
        let schema = EditFile.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "path"));
        assert!(required.iter().any(|r| r == "old_text"));
        assert!(required.iter().any(|r| r == "new_text"));
    }

    // --- EditFileInput serde ---

    #[test]
    fn edit_file_input_serde_roundtrip() {
        let input = EditFileInput {
            path: PathBuf::from("src/main.rs"),
            old_text: "fn main".into(),
            new_text: "fn main()".into(),
        };
        let json = serde_json::to_string(&input).unwrap();
        let decoded: EditFileInput = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.path, PathBuf::from("src/main.rs"));
        assert_eq!(decoded.old_text, "fn main");
    }

    // --- Edit edge cases ---

    #[test]
    fn edit_file_old_text_not_found() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello world").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "does_not_exist",
            "new_text": "replacement"
        });

        let result = tool.execute(params, &ctx).unwrap();
        assert!(result.text.contains("not found"));
    }

    #[test]
    fn edit_file_invalid_params() {
        let workspace = tempdir().unwrap();
        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": 123
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
    }
}
