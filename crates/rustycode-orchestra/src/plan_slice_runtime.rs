//! Deterministic setup and validation helpers for `plan-slice`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use rustycode_llm::{ChatMessage, CompletionRequest};
use tracing::{debug, info, warn};

use crate::conversation_runtime::{
    append_assistant_and_tool_results, stream_text_round, stream_tool_round,
};
use crate::Orchestra2Executor;

pub struct PreparedPlanSlice {
    pub slice_path: PathBuf,
    pub plan_path: PathBuf,
    pub prompt: String,
}

pub fn prepare_plan_slice(
    project_root: &Path,
    slice_id: &str,
    milestone_id: &str,
    slice_title: &str,
) -> PreparedPlanSlice {
    let prompt_template = include_str!("../prompts/plan-slice.md");
    let slice_path = project_root
        .join(".orchestra/milestones")
        .join(milestone_id)
        .join("slices")
        .join(slice_id);
    let plan_path = slice_path.join("PLAN.md");

    let roadmap_path = project_root
        .join(".orchestra/milestones")
        .join(milestone_id)
        .join("ROADMAP.md");
    let roadmap_content = std::fs::read_to_string(&roadmap_path).unwrap_or_default();

    let research_path = slice_path.join("RESEARCH.md");
    let research_content = if research_path.exists() {
        std::fs::read_to_string(&research_path).unwrap_or_default()
    } else {
        String::new()
    };

    let inlined_context = format!(
        "## Milestone Roadmap\n\n{}\n\n## Slice Research\n\n{}",
        roadmap_content,
        if research_content.is_empty() {
            "No research document available.".to_string()
        } else {
            research_content
        }
    );

    let prompt = prompt_template
        .replace("{{sliceId}}", slice_id)
        .replace("{{sliceTitle}}", slice_title)
        .replace("{{milestoneId}}", milestone_id)
        .replace("{{workingDirectory}}", &project_root.to_string_lossy())
        .replace("{{inlinedContext}}", &inlined_context)
        .replace("{{outputPath}}", &plan_path.to_string_lossy())
        .replace("{{slicePath}}", &slice_path.to_string_lossy());

    PreparedPlanSlice {
        slice_path,
        plan_path,
        prompt,
    }
}

pub fn plan_slice_tools() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "name": "read_file",
            "description": "Read the contents of a file",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read"
                    }
                },
                "required": ["path"]
            }
        }),
        serde_json::json!({
            "name": "write_file",
            "description": "Write content to a file",
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
            "name": "bash",
            "description": "Execute a bash command",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Command to execute"}
                },
                "required": ["command"]
            }
        }),
    ]
}

pub fn build_plan_slice_validation_message(plan_path: &Path) -> String {
    format!(
        "CRITICAL: You must use the write_file tool to create the PLAN.md file at: {}\n\nThis file is REQUIRED before you can continue. Stop exploring and use write_file NOW.\n\nUse this exact path: {}",
        plan_path.display(),
        plan_path.display()
    )
}

pub fn note_missing_plan(plan_path: &Path, retry: usize, max_retries: usize) {
    warn!(
        "PLAN.md was not created (attempt {}/{}), asking LLM to create it... path={}",
        retry + 1,
        max_retries,
        plan_path.display()
    );
}

pub fn ensure_plan_created(plan_path: &Path, max_retries: usize) -> Result<()> {
    if plan_path.exists() {
        info!("✅ PLAN.md created successfully");
        Ok(())
    } else {
        bail!(
            "PLAN.md was not created at {:?} after {} validation attempts",
            plan_path,
            max_retries
        )
    }
}

impl Orchestra2Executor {
    /// Execute plan-slice unit type (like orchestra-2)
    pub(crate) async fn execute_plan_slice(
        &self,
        slice_id: &str,
        milestone_id: &str,
        slice_title: &str,
        _timeout_state: &mut crate::timeout::UnitTimeoutState,
        _discovered_skills: &[crate::skill_discovery::DiscoveredSkill],
    ) -> anyhow::Result<()> {
        info!("📋 Planning slice: {} ({})", slice_id, slice_title);

        let prepared = prepare_plan_slice(&self.project_root, slice_id, milestone_id, slice_title);
        let plan_path = prepared.plan_path.clone();

        let mut messages = vec![
            ChatMessage::system("You are an expert software architect and task planner."),
            ChatMessage::user(prepared.prompt),
        ];

        let tools = plan_slice_tools();

        let request = CompletionRequest::new(self.get_model(), messages.clone())
            .with_max_tokens(8192)
            .with_temperature(0.1)
            .with_tools(tools.clone());

        let (mut assistant_response, pending_tool_calls) =
            stream_tool_round(&self.provider, request).await?;
        let tool_results = self
            .execute_pending_tool_calls(slice_id, pending_tool_calls, None)
            .await?;

        if !tool_results.is_empty() {
            append_assistant_and_tool_results(&mut messages, assistant_response, tool_results);

            let request = CompletionRequest::new(self.get_model(), messages.clone())
                .with_max_tokens(8192)
                .with_temperature(0.1);

            assistant_response = stream_text_round(&self.provider, request).await?;
        }

        let mut validation_messages = messages.clone();
        validation_messages.push(ChatMessage::assistant(assistant_response));

        let max_retries = 3;
        for retry in 0..max_retries {
            if plan_path.exists() {
                info!("✅ PLAN.md created successfully");
                break;
            }

            note_missing_plan(&plan_path, retry, max_retries);
            validation_messages.push(ChatMessage::user(build_plan_slice_validation_message(
                &plan_path,
            )));

            let request = CompletionRequest::new(self.get_model(), validation_messages.clone())
                .with_max_tokens(4096)
                .with_temperature(0.1)
                .with_tools(tools.clone());

            debug!("\n🤖 LLM Response (validation retry {}):", retry + 1);
            let (response_text, pending_tool_calls) =
                stream_tool_round(&self.provider, request).await?;
            let tool_results = self
                .execute_pending_tool_calls(slice_id, pending_tool_calls, None)
                .await?;

            append_assistant_and_tool_results(
                &mut validation_messages,
                response_text,
                tool_results,
            );
        }

        ensure_plan_created(&plan_path, max_retries)?;

        info!("✅ Slice {} planned successfully", slice_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_plan_slice_builds_expected_paths_and_prompt() {
        let temp = tempfile::tempdir().unwrap();
        let project_root = temp.path();
        let slice_dir = project_root.join(".orchestra/milestones/M01/slices/S01");
        std::fs::create_dir_all(&slice_dir).unwrap();
        std::fs::write(
            project_root.join(".orchestra/milestones/M01/ROADMAP.md"),
            "# Roadmap\nMilestone content",
        )
        .unwrap();
        std::fs::write(slice_dir.join("RESEARCH.md"), "# Research\nSlice context").unwrap();

        let prepared = prepare_plan_slice(project_root, "S01", "M01", "Core Work");

        assert_eq!(prepared.slice_path, slice_dir);
        assert_eq!(prepared.plan_path, slice_dir.join("PLAN.md"));
        assert!(prepared.prompt.contains("Milestone content"));
        assert!(prepared.prompt.contains("Slice context"));
        assert!(prepared.prompt.contains("Core Work"));
    }

    #[test]
    fn plan_slice_tools_exposes_expected_tool_names() {
        let tools = plan_slice_tools();
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(|v| v.as_str()))
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["read_file", "write_file", "bash"]);
    }

    #[test]
    fn ensure_plan_created_errors_when_plan_missing() {
        let temp = tempfile::tempdir().unwrap();
        let plan_path = temp.path().join("PLAN.md");

        let err = ensure_plan_created(&plan_path, 3).unwrap_err().to_string();

        assert!(err.contains("PLAN.md was not created"));
        assert!(err.contains("3"));
    }
}
