//! RustyCode ACP (Agent Client Protocol) Implementation
//!
//! This crate provides an implementation of the Agent Client Protocol
//! for RustyCode, making it compatible with ACP clients like Zed, VS Code, etc.
//!
//! # ACP Overview
//!
//! ACP is a standardized protocol for AI agent servers using JSON-RPC over stdio.
//! Specification: https://agentclientprotocol.com/
//!
//! # Usage
//!
//! ```bash
//! # Start the ACP server
//! rustycode-acp
//!
//! # Start in a specific directory
//! rustycode-acp --cwd /path/to/project
//! ```
//!
//! # Protocol Support
//!
//! - ✅ `initialize` - Protocol negotiation
//! - ✅ `session/new` - Create sessions
//! - ✅ `session/load` - Resume sessions
//! - ⏳ `session/prompt` - Process messages (basic support)
//! - ❌ Streaming responses (planned)
//! - ❌ Tool progress reporting (planned)

pub mod llm_integration;
pub mod prompt_handler;
pub mod server;
pub mod tool_executor;
pub mod types;

pub use prompt_handler::PromptHandler;
pub use server::ACPServer;
pub use types::*;

/// ACP protocol version
pub const ACP_PROTOCOL_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lib_acp_protocol_version() {
        assert_eq!(ACP_PROTOCOL_VERSION, 1);
    }

    #[test]
    fn test_lib_re_exports_match_types_module() {
        // The lib re-exports ACP_PROTOCOL_VERSION which should match types::ACP_PROTOCOL_VERSION
        assert_eq!(ACP_PROTOCOL_VERSION, types::ACP_PROTOCOL_VERSION);
    }

    #[test]
    fn test_acp_server_new_via_reexport() {
        // Verify ACPServer can be constructed via the re-exported type
        let _server = ACPServer::new();
    }

    #[test]
    fn test_prompt_handler_default_via_reexport() {
        let _handler = PromptHandler::default();
    }
}
