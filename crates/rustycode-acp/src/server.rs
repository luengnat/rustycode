//! ACP Server - JSON-RPC over stdio
//!
//! Handles JSON-RPC communication over stdin/stdout for ACP protocol.

use crate::prompt_handler::PromptHandler;
use crate::types::*;
use anyhow::Result;
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Write};
use std::sync::{Arc, RwLock};
use tracing::{debug, error, info, warn};

/// Maximum number of concurrent sessions
const MAX_SESSIONS: usize = 100;

/// ACP server instance
pub struct ACPServer {
    sessions: Arc<RwLock<Vec<SessionState>>>,
    server_info: ServerInfo,
    agent_capabilities: AgentCapabilities,
    prompt_handler: PromptHandler,
}

impl ACPServer {
    /// Create a new ACP server
    pub fn new() -> Self {
        let server_info = ServerInfo {
            name: "RustyCode".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: Some("Rust-based AI coding assistant".to_string()),
        };

        let agent_capabilities = AgentCapabilities {
            tools: ToolCapabilities {
                list: true,
                call: true,
            },
            modes: Some(vec![
                Mode {
                    id: "ask".to_string(),
                    name: "Ask".to_string(),
                    description: Some("Question and answer mode".to_string()),
                },
                Mode {
                    id: "code".to_string(),
                    name: "Code".to_string(),
                    description: Some("Code generation and editing".to_string()),
                },
            ]),
            models: None, // Will be populated from config
            features: Some(vec!["session/persistence".to_string()]),
        };

        // Get default model from environment or use sensible default
        let default_model =
            std::env::var("RUSTYCODE_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

        let prompt_handler = PromptHandler::new(".".to_string(), default_model.clone());

        Self {
            sessions: Arc::new(RwLock::new(Vec::new())),
            server_info,
            agent_capabilities,
            prompt_handler,
        }
    }

    /// Start the server (blocking)
    pub fn run(&mut self) -> Result<()> {
        info!("Starting ACP server v{}", ACP_PROTOCOL_VERSION);

        // Initialize prompt handler (LLM and tools)
        // Use the current runtime if available, otherwise create a new one
        let init_result = if tokio::runtime::Handle::try_current().is_ok() {
            // Already in a runtime, use block_on_local
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(self.prompt_handler.initialize())
            })
        } else {
            // Not in a runtime, create a new one
            tokio::runtime::Runtime::new()
                .map_err(|e| anyhow::anyhow!("Failed to create runtime: {}", e))?
                .block_on(self.prompt_handler.initialize())
        };

        if let Err(e) = init_result {
            warn!(
                "Failed to initialize prompt handler: {}. Server will run in fallback mode.",
                e
            );
        }

        let stdin = io::stdin();
        let stdout = io::stdout();
        let reader = BufReader::new(stdin.lock());
        let mut writer = stdout.lock();

        for line in reader.lines() {
            let line = line.map_err(|e| anyhow::anyhow!("Failed to read line: {}", e))?;

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            debug!("Received request: {}", line);

            // Parse and handle request
            let response = match self.handle_request(&line) {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Failed to handle request: {}", e);
                    JsonRpcResponse::<Value> {
                        jsonrpc: "2.0".to_string(),
                        id: RequestId::Num(0),
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Internal error: {}", e),
                            data: None,
                        }),
                    }
                }
            };

            // Send response
            let response_json = serde_json::to_string(&response)
                .map_err(|e| anyhow::anyhow!("Failed to serialize response: {}", e))?;

            writeln!(writer, "{}", response_json)
                .map_err(|e| anyhow::anyhow!("Failed to write response: {}", e))?;
            writer
                .flush()
                .map_err(|e| anyhow::anyhow!("Failed to flush stdout: {}", e))?;

            debug!("Sent response: {}", response_json);
        }

        Ok(())
    }

    /// Handle a single JSON-RPC request
    fn handle_request(&mut self, request_str: &str) -> Result<JsonRpcResponse<Value>> {
        let raw: Value = serde_json::from_str(request_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;

        let id = raw
            .get("id")
            .and_then(|v| {
                if let Some(n) = v.as_u64() {
                    Some(RequestId::Num(n))
                } else {
                    v.as_str().map(|s| RequestId::Str(s.to_string()))
                }
            })
            .unwrap_or(RequestId::Num(0));

        let method = raw
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'method' field"))?;

        debug!("Handling method: {}", method);

        match method {
            "initialize" => self.handle_initialize(raw, id),
            "session/new" => self.handle_session_new(raw, id),
            "session/load" => self.handle_session_load(raw, id),
            "session/prompt" => self.handle_session_prompt(raw, id),
            "shutdown" => {
                info!("Shutdown requested");
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(serde_json::json!({})),
                    error: None,
                })
            }
            _ => {
                warn!("Unknown method: {}", method);
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method not found: {}", method),
                        data: None,
                    }),
                })
            }
        }
    }

    /// Handle initialize request
    fn handle_initialize(&self, raw: Value, id: RequestId) -> Result<JsonRpcResponse<Value>> {
        let _req: InitializeRequest = serde_json::from_value(
            raw.get("params")
                .ok_or_else(|| anyhow::anyhow!("Missing params"))?
                .clone(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to parse InitializeRequest: {}", e))?;

        info!("Initialized with protocol version {}", ACP_PROTOCOL_VERSION);

        let response = InitializeResponse {
            protocol_version: ACP_PROTOCOL_VERSION,
            capabilities: self.agent_capabilities.clone(),
            server: self.server_info.clone(),
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(response)?),
            error: None,
        })
    }

    /// Handle session/new request
    fn handle_session_new(&self, raw: Value, id: RequestId) -> Result<JsonRpcResponse<Value>> {
        let req: NewSessionRequest = serde_json::from_value(
            raw.get("params")
                .ok_or_else(|| anyhow::anyhow!("Missing params"))?
                .clone(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to parse NewSessionRequest: {}", e))?;

        let cwd = req.cwd.unwrap_or_else(|| ".".to_string());
        let mut session = SessionState::new(cwd);
        session.model = req.model;
        session.mode = req.mode;
        session.mcp_servers = req.mcp_servers.unwrap_or_default();

        let session_id = session.id.clone();
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire sessions lock for write: {}", e))?;

        // Enforce session limit
        if sessions.len() >= MAX_SESSIONS {
            drop(sessions);
            warn!(
                "Maximum session limit ({}) reached, rejecting new session",
                MAX_SESSIONS
            );
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32002,
                    message: format!(
                        "Maximum session limit ({}) reached. Close some sessions first.",
                        MAX_SESSIONS
                    ),
                    data: None,
                }),
            });
        }

        sessions.push(session);
        drop(sessions);

        info!("Created new session: {}", session_id);

        let response = NewSessionResponse { session_id };
        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(response)?),
            error: None,
        })
    }

    /// Handle session/load request
    fn handle_session_load(&self, raw: Value, id: RequestId) -> Result<JsonRpcResponse<Value>> {
        let req: LoadSessionRequest = serde_json::from_value(
            raw.get("params")
                .ok_or_else(|| anyhow::anyhow!("Missing params"))?
                .clone(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to parse LoadSessionRequest: {}", e))?;

        // Check if session exists
        let sessions = self
            .sessions
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire sessions lock for read: {}", e))?;
        let session_exists = sessions.iter().any(|s| s.id == req.session_id);
        drop(sessions);

        if session_exists {
            let sessions = self
                .sessions
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire sessions lock for read: {}", e))?;
            let session_data = sessions.iter().find(|s| s.id == req.session_id).cloned();
            drop(sessions);

            if let Some(session) = session_data {
                info!("Loaded session: {}", req.session_id);
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(serde_json::to_value(session)?),
                    error: None,
                })
            } else {
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32001,
                        message: format!("Session not found: {}", req.session_id),
                        data: None,
                    }),
                })
            }
        } else {
            warn!("Session not found: {}", req.session_id);
            Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32001,
                    message: format!("Session not found: {}", req.session_id),
                    data: None,
                }),
            })
        }
    }

    /// Handle session/prompt request
    fn handle_session_prompt(&self, raw: Value, id: RequestId) -> Result<JsonRpcResponse<Value>> {
        let req: PromptRequest = serde_json::from_value(
            raw.get("params")
                .ok_or_else(|| anyhow::anyhow!("Missing params"))?
                .clone(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to parse PromptRequest: {}", e))?;

        // Find session
        let sessions = self
            .sessions
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire sessions lock for read: {}", e))?;
        let session = sessions
            .iter()
            .find(|s| s.id == req.session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", req.session_id))?;

        let cwd = session.cwd.clone();

        // Drop the lock before async operation
        drop(sessions);

        // Process the prompt (using current runtime if available)
        let result = if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    self.prompt_handler
                        .process_prompt(&req.session_id, &req.messages, &cwd)
                        .await
                })
            })?
        } else {
            tokio::runtime::Runtime::new()
                .map_err(|e| anyhow::anyhow!("Failed to create runtime: {}", e))?
                .block_on(async {
                    self.prompt_handler
                        .process_prompt(&req.session_id, &req.messages, &cwd)
                        .await
                })?
        };

        info!(
            "Processed prompt for session {}, response length: {}",
            req.session_id,
            result.content.len()
        );

        let response = PromptResponse {
            stop_reason: StopReason::EndTurn,
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(response)?),
            error: None,
        })
    }
}

impl Default for ACPServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acp_server_new_creates_instance() {
        let server = ACPServer::new();
        // Server should be created without panic
        assert_eq!(server.server_info.name, "RustyCode");
        assert_eq!(server.server_info.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_acp_server_default() {
        let server = ACPServer::default();
        assert_eq!(server.server_info.name, "RustyCode");
    }

    #[test]
    fn test_acp_server_starts_with_empty_sessions() {
        let server = ACPServer::new();
        let sessions = server.sessions.read().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_acp_server_agent_capabilities() {
        let server = ACPServer::new();
        assert!(server.agent_capabilities.tools.list);
        assert!(server.agent_capabilities.tools.call);
        assert!(server.agent_capabilities.modes.is_some());
        let modes = server.agent_capabilities.modes.as_ref().unwrap();
        assert_eq!(modes.len(), 2);
        assert_eq!(modes[0].id, "ask");
        assert_eq!(modes[1].id, "code");
    }

    #[test]
    fn test_acp_server_features() {
        let server = ACPServer::new();
        let features = server.agent_capabilities.features.as_ref().unwrap();
        assert!(features.contains(&"session/persistence".to_string()));
    }

    #[test]
    fn test_server_info_has_description() {
        let server = ACPServer::new();
        assert!(server.server_info.description.is_some());
        let desc = server.server_info.description.as_ref().unwrap();
        assert!(!desc.is_empty());
    }

    #[test]
    fn test_max_sessions_constant() {
        assert_eq!(MAX_SESSIONS, 100);
    }

    #[test]
    fn test_handle_request_invalid_json() {
        let mut server = ACPServer::new();
        let result = server.handle_request("not valid json{{{");
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_request_missing_method() {
        let mut server = ACPServer::new();
        let result = server.handle_request(r#"{"jsonrpc":"2.0","id":1}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_request_unknown_method() {
        let mut server = ACPServer::new();
        let result = server.handle_request(r#"{"jsonrpc":"2.0","id":1,"method":"foo/bar"}"#);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn test_handle_shutdown_method() {
        let mut server = ACPServer::new();
        let result = server.handle_request(r#"{"jsonrpc":"2.0","id":5,"method":"shutdown"}"#);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_handle_initialize_missing_params() {
        let mut server = ACPServer::new();
        let result = server.handle_request(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_initialize_valid() {
        let mut server = ACPServer::new();
        let result = server.handle_request(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocol_version":1}}"#,
        );
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        // Verify the response contains expected fields
        let result_val = resp.result.unwrap();
        assert_eq!(result_val["protocol_version"], 1);
        assert_eq!(result_val["server"]["name"], "RustyCode");
    }

    #[test]
    fn test_handle_session_new_valid() {
        let mut server = ACPServer::new();
        let result = server.handle_request(
            r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"/tmp"}}"#,
        );
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        let result_val = resp.result.unwrap();
        // session_id should be a non-empty string
        assert!(!result_val["session_id"].as_str().unwrap().is_empty());
    }

    #[test]
    fn test_handle_session_new_without_params() {
        let mut server = ACPServer::new();
        let result =
            server.handle_request(r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{}}"#);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_handle_session_new_with_model_and_mode() {
        let mut server = ACPServer::new();
        let result = server.handle_request(
            r#"{"jsonrpc":"2.0","id":3,"method":"session/new","params":{"cwd":"/project","model":{"provider_id":"anthropic","model_id":"claude-3"},"mode":"code"}}"#,
        );
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.result.is_some());
        // Verify session was stored
        let sessions = server.sessions.read().unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].model.is_some());
        assert_eq!(sessions[0].mode, Some("code".to_string()));
    }

    #[test]
    fn test_handle_session_load_not_found() {
        let mut server = ACPServer::new();
        let result = server.handle_request(
            r#"{"jsonrpc":"2.0","id":4,"method":"session/load","params":{"session_id":"nonexistent"}}"#,
        );
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32001);
    }

    #[test]
    fn test_handle_session_load_after_create() {
        let mut server = ACPServer::new();

        // Create a session first
        let create_result = server
            .handle_request(
                r#"{"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"/test"}}"#,
            )
            .unwrap();
        let session_id = create_result.result.unwrap()["session_id"]
            .as_str()
            .unwrap()
            .to_string();

        // Load it back
        let load_result = server.handle_request(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"session/load","params":{{"session_id":"{}"}}}}"#,
            session_id,
        ));
        assert!(load_result.is_ok());
        let resp = load_result.unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_handle_session_new_max_limit() {
        let mut server = ACPServer::new();

        // Create MAX_SESSIONS sessions
        for i in 0..MAX_SESSIONS {
            let result = server.handle_request(&format!(
                r#"{{"jsonrpc":"2.0","id":{},"method":"session/new","params":{{}}}}"#,
                i,
            ));
            assert!(result.is_ok());
        }

        // The next one should be rejected
        let result = server
            .handle_request(r#"{"jsonrpc":"2.0","id":9999,"method":"session/new","params":{}}"#);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32002);
    }

    #[test]
    fn test_handle_request_with_string_id() {
        let mut server = ACPServer::new();
        let result =
            server.handle_request(r#"{"jsonrpc":"2.0","id":"req-string-42","method":"shutdown"}"#);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.id, RequestId::Str("req-string-42".to_string()));
    }

    #[test]
    fn test_handle_request_without_id_uses_default() {
        let mut server = ACPServer::new();
        // No "id" field -- should default to RequestId::Num(0)
        let result = server.handle_request(r#"{"jsonrpc":"2.0","method":"shutdown"}"#);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.id, RequestId::Num(0));
    }
}
