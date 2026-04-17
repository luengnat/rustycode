//! Streamable HTTP transport for MCP (Model Context Protocol).
//!
//! Implements the MCP "Streamable HTTP" transport where JSON-RPC messages are
//! exchanged via HTTP POST requests. The server may respond with either a
//! direct JSON response or an SSE stream, per the MCP specification.

use crate::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::transport::{IncomingMessage, Transport, MAX_MESSAGE_SIZE};
use crate::types::JsonRpcId;
use crate::{McpError, McpResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// HTTP-based transport for remote MCP servers.
pub struct HttpTransport {
    url: String,
    headers: HashMap<String, String>,
    session_id: Arc<Mutex<Option<String>>>,
    client: reqwest::Client,
    inbox: mpsc::Receiver<IncomingMessage>,
    inbox_tx: mpsc::Sender<IncomingMessage>,
    connected: bool,
    bg_task: Option<JoinHandle<()>>,
}

impl HttpTransport {
    /// Create a new HTTP transport targeting the given URL.
    pub fn new(url: &str, headers: HashMap<String, String>) -> McpResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| McpError::TransportError(format!("Failed to create HTTP client: {}", e)))?;

        let (inbox_tx, inbox) = mpsc::channel(256);

        Ok(Self {
            url: url.to_string(),
            headers,
            session_id: Arc::new(Mutex::new(None)),
            client,
            inbox,
            inbox_tx,
            connected: true,
            bg_task: None,
        })
    }

    /// Build the request builder with common headers.
    fn build_request(&self) -> reqwest::RequestBuilder {
        let mut req = self.client.post(&self.url);
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req
    }

    /// Set a header
    pub fn set_header(&mut self, key: String, value: String) {
        self.headers.insert(key, value);
    }
}

#[async_trait::async_trait]
impl Transport for HttpTransport {
    async fn send_request(&mut self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        if !self.connected {
            return Err(McpError::ConnectionClosed);
        }

        let body = serde_json::to_string(&request)
            .map_err(|e| McpError::TransportError(format!("Failed to serialize request: {}", e)))?;

        debug!("HTTP transport sending request to {}", self.url);

        let mut req = self.build_request();
        req = req
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(body);

        // Attach session ID if we have one
        {
            let sid = self.session_id.lock().await;
            if let Some(ref id) = *sid {
                req = req.header("Mcp-Session-Id", id.as_str());
            }
        }

        let response = req
            .send()
            .await
            .map_err(|e| McpError::TransportError(format!("HTTP request failed: {}", e)))?;

        // Store session ID from response
        if let Some(sid) = response.headers().get("Mcp-Session-Id") {
            if let Ok(val) = sid.to_str() {
                *self.session_id.lock().await = Some(val.to_string());
            }
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        if content_type.contains("text/event-stream") {
            // SSE streaming response — read events until we get a result for our request
            let id_str = match &request.id {
                JsonRpcId::Number(n) => n.to_string(),
                JsonRpcId::String(s) => s.clone(),
                JsonRpcId::Null => String::new(),
            };

            let text = response
                .text()
                .await
                .map_err(|e| McpError::TransportError(format!("Failed to read SSE body: {}", e)))?;

            for event_block in text.split("\n\n") {
                let mut data = String::new();
                for line in event_block.lines() {
                    if let Some(d) = line.strip_prefix("data: ") {
                        data.push_str(d.trim());
                    }
                }
                if data.is_empty() {
                    continue;
                }
                if data.len() > MAX_MESSAGE_SIZE {
                    warn!("SSE event exceeds max message size, skipping");
                    continue;
                }
                if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(&data) {
                    let resp_id_str = match &resp.id {
                        JsonRpcId::Number(n) => n.to_string(),
                        JsonRpcId::String(s) => s.clone(),
                        JsonRpcId::Null => String::new(),
                    };
                    if resp_id_str == id_str {
                        return Ok(resp);
                    }
                    // Not our response — forward to inbox
                    let _ = self.inbox_tx.try_send(IncomingMessage::Response(resp));
                } else if let Ok(notif) = serde_json::from_str::<JsonRpcNotification>(&data) {
                    let _ = self.inbox_tx.try_send(IncomingMessage::Notification(notif));
                }
            }

            Err(McpError::TransportError(
                "SSE stream ended without matching response".to_string(),
            ))
        } else {
            // Direct JSON response
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

            let resp: JsonRpcResponse = serde_json::from_slice(&bytes).map_err(|e| {
                McpError::TransportError(format!("Failed to parse JSON-RPC response: {}", e))
            })?;

            Ok(resp)
        }
    }

    async fn send_notification(&mut self, notification: JsonRpcNotification) -> McpResult<()> {
        if !self.connected {
            return Err(McpError::ConnectionClosed);
        }

        let body = serde_json::to_string(&notification).map_err(|e| {
            McpError::TransportError(format!("Failed to serialize notification: {}", e))
        })?;

        let mut req = self.build_request();
        req = req
            .header("Content-Type", "application/json")
            .body(body);

        {
            let sid = self.session_id.lock().await;
            if let Some(ref id) = *sid {
                req = req.header("Mcp-Session-Id", id.as_str());
            }
        }

        let response = req.send().await.map_err(|e| {
            McpError::TransportError(format!("Failed to send notification: {}", e))
        })?;

        if let Some(sid) = response.headers().get("Mcp-Session-Id") {
            if let Ok(val) = sid.to_str() {
                *self.session_id.lock().await = Some(val.to_string());
            }
        }

        debug!("HTTP notification sent, status: {}", response.status());
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
        if let Some(handle) = self.bg_task.take() {
            handle.abort();
        }
        info!("HTTP transport closed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JsonRpcId;

    #[test]
    fn test_http_transport_construction() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer test".to_string());
        let transport = HttpTransport::new("https://api.example.com/mcp", headers).unwrap();
        assert!(transport.is_connected());
        assert!(transport.url.contains("api.example.com"));
    }

    #[test]
    fn test_http_transport_empty_headers() {
        let transport = HttpTransport::new("https://api.example.com/mcp", HashMap::new()).unwrap();
        assert!(transport.is_connected());
    }

    #[tokio::test]
    async fn test_http_transport_close() {
        let mut transport =
            HttpTransport::new("https://api.example.com/mcp", HashMap::new()).unwrap();
        assert!(transport.is_connected());
        transport.close().await.unwrap();
        assert!(!transport.is_connected());
    }

    #[tokio::test]
    async fn test_http_transport_send_after_close_fails() {
        let mut transport =
            HttpTransport::new("https://api.example.com/mcp", HashMap::new()).unwrap();
        transport.close().await.unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: JsonRpcId::Number(1),
            method: "test".to_string(),
            params: None,
        };
        let result = transport.send_request(request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("closed"));
    }

    #[test]
    fn test_session_id_initially_none() {
        let transport =
            HttpTransport::new("https://api.example.com/mcp", HashMap::new()).unwrap();
        let sid = transport.session_id.try_lock().unwrap();
        assert!(sid.is_none());
    }
}
