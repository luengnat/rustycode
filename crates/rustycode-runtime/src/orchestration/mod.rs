//! Unified orchestration for system prompts, intent detection, and context assembly.
//!
//! This service acts as the single source of truth for preparing AI requests,
//! ensuring that TUI, CLI, and Terminal Bench share the exact same logic.

use anyhow::Result;
use rustycode_llm::provider_v2::ChatMessage;

pub mod intent;
pub mod prompt;

pub use prompt::PromptOrchestrator;

#[derive(Debug)]
pub enum AgentEvent {
    TextDelta(String),
    ToolCall(String, String, String), // id, name, args
    TurnComplete,
}

pub trait AgentSession: Send + Sync {
    fn send_input(
        &mut self,
        messages: Vec<ChatMessage>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;
    fn receive_event(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentEvent>> + Send + '_>>;
}
