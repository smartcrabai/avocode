use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mcp::transport::Transport;
use crate::mcp::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, RpcId};

const JSONRPC_VERSION: &str = "2.0";

/// Errors produced by the MCP client.
#[derive(Debug, Error)]
pub enum McpError {
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON-RPC error {code}: {message}")]
    Rpc { code: i64, message: String },

    #[error("unexpected response: {0}")]
    UnexpectedResponse(String),

    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("server not found: {0}")]
    ServerNotFound(String),

    #[error("process spawn error: {0}")]
    Spawn(String),
}

/// Which transport the server configuration specifies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TransportKind {
    Stdio,
    Sse,
    StreamableHttp,
}

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    /// Logical name used to identify this server.
    pub name: String,
    /// Transport type.
    pub transport: TransportKind,
    /// Executable to launch (stdio transport).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Arguments passed to the executable (stdio transport).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Extra environment variables (stdio transport).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// Server URL (SSE / streamable-HTTP transports).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Extra HTTP headers (SSE / streamable-HTTP transports).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

/// A tool advertised by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

/// A connected MCP client communicating over a [`Transport`].
pub struct McpClient {
    name: String,
    transport: Box<dyn Transport>,
    next_id: i64,
}

impl McpClient {
    /// Create a new client wrapping the given transport.
    #[must_use]
    pub fn new(name: String, transport: Box<dyn Transport>) -> Self {
        Self {
            name,
            transport,
            next_id: 1,
        }
    }

    /// Server name as provided in the config.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    fn bump_id(&mut self) -> RpcId {
        let id = self.next_id;
        self.next_id += 1;
        RpcId::Number(id)
    }

    /// Send a JSON-RPC request and wait for the matching response.
    async fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, McpError> {
        let id = self.bump_id();
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_owned(),
            id: id.clone(),
            method: method.to_owned(),
            params,
        };
        let msg = serde_json::to_string(&req)?;
        self.transport.send(&msg).await?;

        loop {
            let raw = self.transport.recv().await?;
            let resp: JsonRpcResponse = serde_json::from_str(&raw)?;
            if resp.id != id {
                // Not our response — skip (could be a notification).
                continue;
            }
            if let Some(err) = resp.error {
                return Err(McpError::Rpc {
                    code: err.code,
                    message: err.message,
                });
            }
            return resp
                .result
                .ok_or_else(|| McpError::UnexpectedResponse("missing result".to_owned()));
        }
    }

    /// Send a JSON-RPC notification (fire and forget).
    async fn notify(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), McpError> {
        let notif = JsonRpcNotification {
            jsonrpc: JSONRPC_VERSION.to_owned(),
            method: method.to_owned(),
            params,
        };
        let msg = serde_json::to_string(&notif)?;
        self.transport.send(&msg).await
    }

    /// Perform the MCP handshake.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the server rejects the
    /// `initialize` request.
    pub async fn initialize(&mut self) -> Result<(), McpError> {
        self.request(
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "avocode",
                    "version": "0.1.0"
                }
            })),
        )
        .await?;
        self.notify("notifications/initialized", None).await
    }

    /// Fetch the list of tools from the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the server response is
    /// malformed.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>, McpError> {
        let result = self.request("tools/list", None).await?;
        let tools = result
            .get("tools")
            .ok_or_else(|| McpError::UnexpectedResponse("missing tools array".to_owned()))?
            .clone();
        let tools: Vec<McpTool> = serde_json::from_value(tools)?;
        Ok(tools)
    }

    /// Invoke a tool on the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport fails or the server returns an RPC
    /// error.
    pub async fn call_tool(
        &mut self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        self.request(
            "tools/call",
            Some(serde_json::json!({
                "name": name,
                "arguments": args
            })),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_config_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let config = McpServerConfig {
            name: "test-server".to_owned(),
            transport: TransportKind::Stdio,
            command: Some("npx".to_owned()),
            args: Some(vec!["-y".to_owned(), "some-mcp".to_owned()]),
            env: None,
            url: None,
            headers: None,
        };
        let json = serde_json::to_string(&config)?;
        let back: McpServerConfig = serde_json::from_str(&json)?;
        assert_eq!(back.name, config.name);
        assert_eq!(back.transport, TransportKind::Stdio);
        assert_eq!(back.command, config.command);
        Ok(())
    }

    #[test]
    fn test_mcp_server_config_sse_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_owned(), "Bearer token".to_owned());
        let config = McpServerConfig {
            name: "sse-server".to_owned(),
            transport: TransportKind::Sse,
            command: None,
            args: None,
            env: None,
            url: Some("https://example.com/sse".to_owned()),
            headers: Some(headers),
        };
        let json = serde_json::to_string(&config)?;
        let back: McpServerConfig = serde_json::from_str(&json)?;
        assert_eq!(back.transport, TransportKind::Sse);
        assert_eq!(back.url.as_deref(), Some("https://example.com/sse"));
        Ok(())
    }

    #[test]
    fn test_mcp_tool_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let tool = McpTool {
            name: "search".to_owned(),
            description: Some("Search the web".to_owned()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        };
        let json = serde_json::to_string(&tool)?;
        let back: McpTool = serde_json::from_str(&json)?;
        assert_eq!(back.name, "search");
        assert_eq!(back.description.as_deref(), Some("Search the web"));
        assert!(back.input_schema.is_object());
        Ok(())
    }

    #[test]
    fn test_transport_kind_serde() -> Result<(), Box<dyn std::error::Error>> {
        let stdio: TransportKind = serde_json::from_str("\"stdio\"")?;
        assert_eq!(stdio, TransportKind::Stdio);

        let sse: TransportKind = serde_json::from_str("\"sse\"")?;
        assert_eq!(sse, TransportKind::Sse);

        let http: TransportKind = serde_json::from_str("\"streamableHttp\"")?;
        assert_eq!(http, TransportKind::StreamableHttp);
        Ok(())
    }
}
