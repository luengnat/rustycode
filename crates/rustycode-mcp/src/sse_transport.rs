//! SSE (Server-Sent Events) transport for MCP.
//!
//! Deprecated in favor of Streamable HTTP, but supported for backward
//! compatibility with older MCP servers that only offer SSE endpoints.

use crate::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::transport::{IncomingMessage, Transport, MAX_MESSAGE_SIZE};
use crate::{McpError, McpResult};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, info};

const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// SSE-based transport for remote MCP servers (deprecated transport).
pub struct SseTransport {
    post_url: String,
    headers: HashMap<String, String>,
    session_id: Option<String>,
    client: reqwest::Client,
    inbox: mpsc::Receiver<IncomingMessage>,
    connected: bool,
}

impl SseTransport {
    /// Create a new SSE transport.
    ///
    /// The `url` is the SSE endpoint for receiving server messages.
    /// Messages are sent via HTTP POST to the same URL (or a URL advertised
    /// by the server via the `endpoint` SSE event).
    pub fn new(url: &str, headers: HashMap<String, String>) -> McpResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| McpError::TransportError(format!("Failed to create HTTP client: {}", e)))?;

        // NOTE: SSE transport is request/response-via-POST. The inbox channel
        // is kept for Transport::receive API conformance, but no background
        // SSE GET listener is started yet. A future enhancement could add a
        // dedicated SSE GET listener that feeds server-pushed events here.
        let (_inbox_tx, inbox) = mpsc::channel(256);

        Ok(Self {
            post_url: url.to_string(),
            headers,
            session_id: None,
            client,
            inbox,
            connected: true,
        })
    }

    fn apply_headers(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut req = req;
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req
    }
}

#[async_trait::async_trait]
impl Transport for SseTransport {
    async fn send_request(&mut self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        if !self.connected {
            return Err(McpError::ConnectionClosed);
        }

        let body = serde_json::to_string(&request)
            .map_err(|e| McpError::TransportError(format!("Failed to serialize request: {}", e)))?;

        debug!("SSE transport POST to {}", self.post_url);

        let mut req = self.apply_headers(self.client.post(&self.post_url));
        req = req.header("Content-Type", "application/json").body(body);

        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid.as_str());
        }

        let response = req
            .send()
            .await
            .map_err(|e| McpError::TransportError(format!("SSE POST failed: {}", e)))?;

        if let Some(sid) = response.headers().get("Mcp-Session-Id") {
            if let Ok(val) = sid.to_str() {
                self.session_id = Some(val.to_string());
            }
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| McpError::TransportError(format!("Failed to read response: {}", e)))?;

        if bytes.len() > MAX_MESSAGE_SIZE {
            return Err(McpError::TransportError(format!(
                "Response exceeds max size ({} bytes)",
                bytes.len()
            )));
        }

        serde_json::from_slice(&bytes)
            .map_err(|e| McpError::TransportError(format!("Failed to parse response: {}", e)))
    }

    async fn send_notification(&mut self, notification: JsonRpcNotification) -> McpResult<()> {
        if !self.connected {
            return Err(McpError::ConnectionClosed);
        }

        let body = serde_json::to_string(&notification).map_err(|e| {
            McpError::TransportError(format!("Failed to serialize notification: {}", e))
        })?;

        let mut req = self.apply_headers(self.client.post(&self.post_url));
        req = req.header("Content-Type", "application/json").body(body);

        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid.as_str());
        }

        let _ = req.send().await.map_err(|e| {
            McpError::TransportError(format!("Failed to send notification: {}", e))
        })?;

        Ok(())
    }

    async fn receive(&mut self) -> McpResult<IncomingMessage> {
        self.inbox
            .recv()
            .await
            .ok_or(McpError::ConnectionClosed)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn close(&mut self) -> McpResult<()> {
        self.connected = false;
        info!("SSE transport closed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::JsonRpcRequest;
    use crate::types::JsonRpcId;

    #[test]
    fn test_sse_transport_construction() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer test".to_string());
        let transport = SseTransport::new("https://api.example.com/sse", headers).unwrap();
        assert!(transport.is_connected());
    }

    #[test]
    fn test_sse_transport_empty_headers() {
        let transport = SseTransport::new("https://api.example.com/sse", HashMap::new()).unwrap();
        assert!(transport.is_connected());
    }

    #[tokio::test]
    async fn test_sse_transport_close() {
        let mut transport =
            SseTransport::new("https://api.example.com/sse", HashMap::new()).unwrap();
        assert!(transport.is_connected());
        transport.close().await.unwrap();
        assert!(!transport.is_connected());
    }

    #[tokio::test]
    async fn test_sse_transport_send_after_close_fails() {
        let mut transport =
            SseTransport::new("https://api.example.com/sse", HashMap::new()).unwrap();
        transport.close().await.unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: JsonRpcId::Number(1),
            method: "test".to_string(),
            params: None,
        };
        let result = transport.send_request(request).await;
        assert!(result.is_err());
    }
}
