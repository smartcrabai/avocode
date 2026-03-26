use crate::mcp::client::McpError;

/// Abstraction over the wire format used to communicate with an MCP server.
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    /// Send a JSON string to the server.
    async fn send(&mut self, msg: &str) -> Result<(), McpError>;

    /// Receive the next JSON string from the server.
    async fn recv(&mut self) -> Result<String, McpError>;

    /// Close the transport connection.
    async fn close(&mut self) -> Result<(), McpError>;
}
