use crate::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::{McpError, McpResult};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use crate::transport::{IncomingMessage, Transport};

#[allow(dead_code)]
/// Simple HTTP transport for MCP using POST requests.
pub struct HttpTransport {
    client: Client,
    url: String,
    headers: HashMap<String, String>,
    session_id: Option<String>,
    connected: bool,
    pending_requests: HashMap<String, oneshot::Sender<JsonRpcResponse>>, // map of id -> responder
    inbox: mpsc::Receiver<IncomingMessage>,
    // Optional sender to push into inbox from internal listeners (not required for tests)
    inbox_sender: Option<mpsc::Sender<IncomingMessage>>,
}

impl HttpTransport {
    /// Create a new HTTP transport with a per-request timeout of 30 seconds.
    pub fn new(url: &str, headers: HashMap<String, String>) -> McpResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| McpError::TransportError(format!("HTTP client error: {}", e)))?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        Ok(Self {
            client,
            url: url.to_string(),
            headers,
            session_id: None,
            connected: true,
            pending_requests: HashMap::new(),
            inbox: rx,
            inbox_sender: Some(tx),
        })
    }

    /// Internal helper to set session id (test visibility only)
    #[cfg(test)]
    pub fn test_set_session_id(&mut self, sid: Option<String>) {
        self.session_id = sid;
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send_request(&mut self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        // Serialize request to JSON
        let json = request
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Serialize error: {}", e)))?;

        // Build request with headers and optional session id
        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json");

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid.as_str());
        }
        req = req.body(json);

        let resp = req
            .send()
            .await
            .map_err(|e| McpError::TransportError(format!("HTTP request failed: {}", e)))?;

        if let Some(val) = resp.headers().get("Mcp-Session-Id") {
            if let Ok(s) = val.to_str() {
                self.session_id = Some(s.to_string());
            }
        }

        // Infer content type
        if let Some(ct) = resp.headers().get(reqwest::header::CONTENT_TYPE) {
            if let Ok(ct_str) = ct.to_str() {
                if ct_str.contains("application/json") {
                    let text = resp
                        .text()
                        .await
                        .map_err(|e| McpError::TransportError(format!("HTTP read error: {}", e)))?;
                    let json_resp = JsonRpcResponse::from_json(&text)
                        .map_err(|e| McpError::ProtocolError(format!("Invalid JSON: {}", e)))?;
                    return Ok(json_resp);
                } else if ct_str.contains("text/event-stream") {
                    let text = resp.text().await.map_err(|e| {
                        McpError::TransportError(format!("BOM SSE read error: {}", e))
                    })?;
                    // Try to parse as a single JSON-RPC response
                    let json_resp = JsonRpcResponse::from_json(&text)
                        .map_err(|e| McpError::ProtocolError(format!("Invalid JSON: {}", e)))?;
                    return Ok(json_resp);
                }
            }
        }

        Err(McpError::TransportError(
            "Unsupported response content-type".to_string(),
        ))
    }

    async fn send_notification(&mut self, notification: JsonRpcNotification) -> McpResult<()> {
        let json = notification
            .to_json()
            .map_err(|e| McpError::ProtocolError(format!("Serialize error: {}", e)))?;

        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json");
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(ref sid) = self.session_id {
            req = req.header("Mcp-Session-Id", sid.as_str());
        }
        req = req.body(json);

        let _resp = req
            .send()
            .await
            .map_err(|e| McpError::TransportError(format!("HTTP notify failed: {}", e)))?;
        Ok(())
    }

    async fn receive(&mut self) -> McpResult<IncomingMessage> {
        match self.inbox.recv().await {
            Some(msg) => Ok(msg),
            None => Err(McpError::ConnectionClosed),
        }
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn close(&mut self) -> McpResult<()> {
        self.connected = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_http_transport_new_builds_client() {
        let headers: HashMap<String, String> = HashMap::new();
        let t = HttpTransport::new("http://example.invalid/mcp", headers).unwrap();
        // is_connected must be true on creation in our simplified implementation
        assert!(t.is_connected());
    }
}
