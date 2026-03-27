pub mod client;
pub mod oauth;
pub mod sse;
pub mod stdio;
pub mod streamable_http;
pub mod transport;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::mcp::client::{McpClient, McpError, McpServerConfig, McpTool, TransportKind};
use crate::mcp::sse::SseTransport;
use crate::mcp::stdio::StdioTransport;
use crate::mcp::streamable_http::StreamableHttpTransport;

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: RpcId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: RpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 notification (no id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC request/response identifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RpcId {
    String(String),
    Number(i64),
    Null,
}

/// Manages multiple MCP server connections.
pub struct McpManager {
    clients: HashMap<String, McpClient>,
}

impl McpManager {
    /// Create a new empty manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    /// Connect to an MCP server using the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport cannot be created or the MCP
    /// handshake fails.
    pub async fn connect(&mut self, config: McpServerConfig) -> Result<(), McpError> {
        let transport: Box<dyn crate::mcp::transport::Transport> = match config.transport {
            TransportKind::Stdio => {
                let command = config.command.as_deref().unwrap_or_default();
                let args = config.args.as_deref().unwrap_or(&[]);
                let env = config.env.clone().unwrap_or_default();
                Box::new(StdioTransport::new(command, args, &env)?)
            }
            TransportKind::Sse => {
                let url = config.url.as_deref().ok_or(McpError::MissingField("url"))?;
                let headers = config.headers.clone().unwrap_or_default();
                Box::new(SseTransport::new(url, headers)?)
            }
            TransportKind::StreamableHttp => {
                let url = config.url.as_deref().ok_or(McpError::MissingField("url"))?;
                let headers = config.headers.clone().unwrap_or_default();
                Box::new(StreamableHttpTransport::new(url, headers))
            }
        };

        let mut client = McpClient::new(config.name.clone(), transport);
        client.initialize().await?;
        self.clients.insert(config.name, client);
        Ok(())
    }

    /// List all tools from all connected servers.
    ///
    /// Returns `(server_name, tool)` pairs.
    ///
    /// # Errors
    ///
    /// Returns an error if any server fails to respond to `tools/list`.
    pub async fn list_all_tools(&mut self) -> Result<Vec<(String, McpTool)>, McpError> {
        let mut result = Vec::new();
        for (name, client) in &mut self.clients {
            let tools = client.list_tools().await?;
            for tool in tools {
                result.push((name.clone(), tool));
            }
        }
        Ok(result)
    }

    /// Call a tool on a specific server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server is not found or the tool call fails.
    pub async fn call_tool(
        &mut self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let client = self
            .clients
            .get_mut(server)
            .ok_or_else(|| McpError::ServerNotFound(server.to_owned()))?;
        client.call_tool(tool, args).await
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_owned(),
            id: RpcId::Number(1),
            method: "initialize".to_owned(),
            params: Some(serde_json::json!({"key": "value"})),
        };
        let json = serde_json::to_string(&req)?;
        let back: JsonRpcRequest = serde_json::from_str(&json)?;
        assert_eq!(back.jsonrpc, "2.0");
        assert_eq!(back.method, "initialize");
        assert_eq!(back.id, RpcId::Number(1));
        Ok(())
    }

    #[test]
    fn test_json_rpc_response_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: RpcId::String("abc".to_owned()),
            result: Some(serde_json::json!({"tools": []})),
            error: None,
        };
        let json = serde_json::to_string(&resp)?;
        let back: JsonRpcResponse = serde_json::from_str(&json)?;
        assert_eq!(back.id, RpcId::String("abc".to_owned()));
        assert!(back.error.is_none());
        Ok(())
    }

    #[test]
    fn test_json_rpc_notification_no_id() -> Result<(), Box<dyn std::error::Error>> {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_owned(),
            method: "initialized".to_owned(),
            params: None,
        };
        let json = serde_json::to_string(&notif)?;
        // notifications have no id field
        assert!(!json.contains("\"id\""));
        Ok(())
    }

    #[test]
    fn test_rpc_id_variants() -> Result<(), Box<dyn std::error::Error>> {
        let id_num: RpcId = serde_json::from_str("42")?;
        assert_eq!(id_num, RpcId::Number(42));

        let id_str: RpcId = serde_json::from_str("\"req-1\"")?;
        assert_eq!(id_str, RpcId::String("req-1".to_owned()));

        let id_null: RpcId = serde_json::from_str("null")?;
        assert_eq!(id_null, RpcId::Null);
        Ok(())
    }

    #[test]
    fn test_mcp_manager_default() {
        let manager = McpManager::default();
        assert!(manager.clients.is_empty());
    }
}
