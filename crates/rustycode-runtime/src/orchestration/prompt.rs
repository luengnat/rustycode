//! Unified orchestrator for system prompts.
use anyhow::Result;
use rustycode_prompt::{context, TemplateManager};
use rustycode_protocol::agent_protocol::get_agent_action_schema;
use rustycode_protocol::intent::{classify_intent, IntentCategory};

/// Central orchestrator for building system prompts.
///
/// Ensures TUI, CLI, and headless modes share identical prompt assembly logic.
pub struct PromptOrchestrator {
    template_manager: TemplateManager,
}

impl Default for PromptOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptOrchestrator {
    /// Create a new prompt orchestrator with default template manager.
    pub fn new() -> Self {
        Self {
            template_manager: TemplateManager::default(),
        }
    }

    /// Build the full system prompt for the given mode and context.
    pub fn build_system_prompt(
        &self,
        mode: &str,
        query: &str,
        workspace_context: &str,
        is_headless: bool,
        supports_websocket: bool,
    ) -> Result<String> {
        // 1. Classify user intent (if query provided)
        let intent = if query.is_empty() {
            IntentCategory::Implementation
        } else {
            classify_intent(query)
        };

        // 2. Base Coding Assistant Prompt
        let base_context = context! {
            "name" => "RustyCode",
            "context" => workspace_context,
            "websocket_available" => supports_websocket.to_string(),
        };
        let base_prompt = self
            .template_manager
            .coding_assistant_prompt(&base_context)?;

        // Wrap with Anthropic Cache Boundaries
        let schema = get_agent_action_schema();
        let cached_prompt = format!(
            "<anthropic-cache>\n{}\n<agent_action_schema>\n{}\n</agent_action_schema>\n</anthropic-cache>",
            base_prompt, schema
        );

        // 3. Assemble with Mode/Intent/Headless-specific tweaks
        let render_context = context! {
            "prompt" => cached_prompt,
            "mode" => mode,
            "intent" => format!("{:?}", intent),
        };

        if is_headless {
            Ok(self
                .template_manager
                .render("system/headless_coding_agent", &render_context)?)
        } else {
            Ok(self
                .template_manager
                .coding_assistant_prompt(&render_context)?)
        }
    }
}
