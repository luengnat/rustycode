//! Core traits for the plugin system
//!
//! Three main plugin types are supported:
//! - `ToolPlugin`: Provides tools/commands that agents can invoke
//! - `AgentPlugin`: Provides agent implementations
//! - `LLMProviderPlugin`: Provides LLM provider implementations

use crate::metadata::PluginMetadata;
use anyhow::Result;
use serde_json::Value;

/// A plugin that provides tools/capabilities to the system
///
/// Tool plugins extend the tool ecosystem by registering new tools
/// that can be invoked by agents.
pub trait ToolPlugin: Send + Sync {
    /// Get the plugin name (must be unique)
    fn name(&self) -> &str;

    /// Get the plugin version
    fn version(&self) -> &str;

    /// Get the plugin description
    fn description(&self) -> &str;

    /// Get plugin metadata including dependencies
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(self.name(), self.version(), self.description())
    }

    /// Initialize the plugin
    ///
    /// Called once when the plugin is loaded. Use this to:
    /// - Set up connections
    /// - Validate configuration
    /// - Pre-load resources
    ///
    /// Return an error if initialization fails.
    fn init(&self) -> Result<()> {
        Ok(())
    }

    /// Clean up resources and shutdown the plugin
    ///
    /// Called when the plugin is unloaded. Use this to:
    /// - Close connections
    /// - Release resources
    /// - Clean up state
    fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    /// Get the list of tools provided by this plugin
    ///
    /// Each tool name should be unique within the plugin.
    fn get_tools(&self) -> Result<Vec<ToolDescriptor>> {
        Ok(vec![])
    }

    /// Get configuration schema for this plugin (JSON Schema format)
    fn config_schema(&self) -> Value {
        serde_json::json!({})
    }
}

/// Descriptor for a tool provided by a plugin
#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    /// Tool name (unique within plugin)
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool parameter schema (JSON Schema)
    pub parameters_schema: Value,
}

impl ToolDescriptor {
    /// Create a new tool descriptor
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters_schema,
        }
    }
}

/// A plugin that provides agent implementations
///
/// Agent plugins extend the agent system by registering new agent types
/// that can execute tasks independently.
pub trait AgentPlugin: Send + Sync {
    /// Get the plugin name (must be unique)
    fn name(&self) -> &str;

    /// Get the plugin version
    fn version(&self) -> &str;

    /// Get the plugin description
    fn description(&self) -> &str;

    /// Get plugin metadata including dependencies
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(self.name(), self.version(), self.description())
    }

    /// Initialize the plugin
    fn init(&self) -> Result<()> {
        Ok(())
    }

    /// Clean up resources and shutdown the plugin
    fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    /// Execute an agent operation
    ///
    /// The context provides information about the current execution environment.
    /// Returns the result of agent execution.
    fn execute(&self, context: AgentExecutionContext) -> Result<AgentExecutionResult> {
        let _ = context;
        Ok(AgentExecutionResult::default())
    }

    /// Get the list of agent types provided by this plugin
    fn get_agent_types(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }

    /// Get configuration schema for this plugin
    fn config_schema(&self) -> Value {
        serde_json::json!({})
    }
}

/// Context for agent execution
#[derive(Debug, Clone)]
pub struct AgentExecutionContext {
    /// Agent type to execute
    pub agent_type: String,
    /// Execution parameters
    pub parameters: Value,
    /// Session ID for correlation
    pub session_id: Option<String>,
}

/// Result of agent execution
#[derive(Debug, Clone, Default)]
pub struct AgentExecutionResult {
    /// Execution succeeded
    pub success: bool,
    /// Result data
    pub data: Option<Value>,
    /// Error message if failed
    pub error: Option<String>,
}

/// A plugin that provides LLM provider implementations
///
/// LLM provider plugins extend the LLM system by registering new providers
/// (e.g., OpenAI, Anthropic, local models).
pub trait LLMProviderPlugin: Send + Sync {
    /// Get the plugin name (must be unique)
    fn name(&self) -> &str;

    /// Get the plugin version
    fn version(&self) -> &str;

    /// Get the plugin description
    fn description(&self) -> &str;

    /// Get plugin metadata including dependencies
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(self.name(), self.version(), self.description())
    }

    /// Initialize the plugin
    fn init(&self) -> Result<()> {
        Ok(())
    }

    /// Clean up resources and shutdown the plugin
    fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    /// Get the provider implementation
    ///
    /// Returns a provider object that can be used to make LLM requests.
    fn get_provider(&self) -> Result<Box<dyn LLMProvider>> {
        Err(anyhow::anyhow!("get_provider not implemented"))
    }

    /// Get the list of models supported by this provider
    fn supported_models(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }

    /// Get configuration schema for this plugin
    fn config_schema(&self) -> Value {
        serde_json::json!({})
    }
}

/// Trait for LLM provider implementations
pub trait LLMProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;

    /// Send a request to the LLM
    fn send_request(&self, request: LLMRequest) -> Result<LLMResponse>;

    /// Check if the provider is available/healthy
    fn health_check(&self) -> Result<()> {
        Ok(())
    }
}

/// Request to send to an LLM provider
#[derive(Debug, Clone)]
pub struct LLMRequest {
    /// Model identifier
    pub model: String,
    /// Prompt/messages
    pub prompt: String,
    /// Temperature for generation
    pub temperature: Option<f32>,
    /// Maximum tokens to generate
    pub max_tokens: Option<usize>,
}

/// Response from an LLM provider
#[derive(Debug, Clone)]
pub struct LLMResponse {
    /// Generated text
    pub text: String,
    /// Token usage information
    pub usage: Option<TokenUsage>,
}

/// Token usage statistics
#[derive(Debug, Clone)]
pub struct TokenUsage {
    /// Tokens used in prompt
    pub prompt_tokens: usize,
    /// Tokens generated
    pub completion_tokens: usize,
    /// Total tokens used
    pub total_tokens: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestToolPlugin;

    impl ToolPlugin for TestToolPlugin {
        fn name(&self) -> &str {
            "test_tool"
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "A test tool plugin"
        }
    }

    struct TestAgentPlugin;

    impl AgentPlugin for TestAgentPlugin {
        fn name(&self) -> &str {
            "test_agent"
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "A test agent plugin"
        }
    }

    struct TestProviderPlugin;

    impl LLMProviderPlugin for TestProviderPlugin {
        fn name(&self) -> &str {
            "test_provider"
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "A test provider plugin"
        }
    }

    #[test]
    fn test_tool_plugin_name_version_description() {
        let plugin = TestToolPlugin;
        assert_eq!(plugin.name(), "test_tool");
        assert_eq!(plugin.version(), "1.0.0");
        assert_eq!(plugin.description(), "A test tool plugin");
    }

    #[test]
    fn test_tool_plugin_init_shutdown() {
        let plugin = TestToolPlugin;
        assert!(plugin.init().is_ok());
        assert!(plugin.shutdown().is_ok());
    }

    #[test]
    fn test_tool_plugin_metadata() {
        let plugin = TestToolPlugin;
        let meta = plugin.metadata();
        assert_eq!(meta.name, "test_tool");
        assert_eq!(meta.version, "1.0.0");
    }

    #[test]
    fn test_agent_plugin_name_version_description() {
        let plugin = TestAgentPlugin;
        assert_eq!(plugin.name(), "test_agent");
        assert_eq!(plugin.version(), "1.0.0");
        assert_eq!(plugin.description(), "A test agent plugin");
    }

    #[test]
    fn test_llm_provider_plugin_name_version_description() {
        let plugin = TestProviderPlugin;
        assert_eq!(plugin.name(), "test_provider");
        assert_eq!(plugin.version(), "1.0.0");
        assert_eq!(plugin.description(), "A test provider plugin");
    }

    #[test]
    fn test_tool_descriptor() {
        let desc = ToolDescriptor::new(
            "read_file",
            "Read a file",
            serde_json::json!({ "type": "object" }),
        );
        assert_eq!(desc.name, "read_file");
        assert_eq!(desc.description, "Read a file");
    }

    #[test]
    fn test_agent_execution_context() {
        let ctx = AgentExecutionContext {
            agent_type: "test_agent".to_string(),
            parameters: serde_json::json!({ "key": "value" }),
            session_id: Some("session-123".to_string()),
        };
        assert_eq!(ctx.agent_type, "test_agent");
        assert_eq!(ctx.session_id.as_ref().unwrap(), "session-123");
    }

    #[test]
    fn test_llm_request() {
        let req = LLMRequest {
            model: "gpt-4".to_string(),
            prompt: "Hello".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(100),
        };
        assert_eq!(req.model, "gpt-4");
        assert_eq!(req.temperature, Some(0.7));
    }

    #[test]
    fn test_llm_response() {
        let resp = LLMResponse {
            text: "Response text".to_string(),
            usage: Some(TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
        };
        assert_eq!(resp.text, "Response text");
        assert_eq!(resp.usage.unwrap().total_tokens, 30);
    }

    #[test]
    fn test_agent_execution_result_default() {
        let result = AgentExecutionResult::default();
        assert!(!result.success);
        assert!(result.data.is_none());
        assert!(result.error.is_none());
    }
}
