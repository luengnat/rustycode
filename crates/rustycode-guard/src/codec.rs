use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct HookInput {
    pub session_id: Option<String>,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub cwd: Option<String>,
    pub hook_event_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookResult {
    pub hook_event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

pub fn parse_input(json: &str) -> Result<HookInput> {
    serde_json::from_str(json).context("Failed to parse hook input JSON")
}

#[allow(dead_code)]
pub fn write_result(result: &HookResult) -> Result<()> {
    let json = serde_json::to_string(result)?;
    println!("{json}");
    Ok(())
}

impl HookResult {
    pub fn allow() -> Self {
        Self {
            hook_event_name: "PreToolUse".to_string(),
            permission_decision: None,
            permission_decision_reason: None,
            updated_input: None,
            additional_context: None,
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            hook_event_name: "PreToolUse".to_string(),
            permission_decision: Some("deny".to_string()),
            permission_decision_reason: Some(reason.into()),
            updated_input: None,
            additional_context: None,
        }
    }

    pub fn warn(context: impl Into<String>) -> Self {
        Self {
            hook_event_name: "PostToolUse".to_string(),
            permission_decision: None,
            permission_decision_reason: None,
            updated_input: None,
            additional_context: Some(context.into()),
        }
    }

    pub fn ask(reason: impl Into<String>) -> Self {
        Self {
            hook_event_name: "PreToolUse".to_string(),
            permission_decision: Some("ask".to_string()),
            permission_decision_reason: Some(reason.into()),
            updated_input: None,
            additional_context: None,
        }
    }
}

#[allow(dead_code)]
pub fn format_result_string(result: &HookResult) -> String {
    serde_json::to_string(result).unwrap_or_else(|_| String::from("{}"))
}
