use std::collections::{HashMap, VecDeque};

use futures_util::StreamExt;
use reqwest::Client;

use crate::mcp::client::McpError;
use crate::mcp::transport::Transport;

/// Transport that POSTs JSON-RPC requests to a single HTTP endpoint and reads
/// the response as a chunked / newline-delimited stream.
pub struct StreamableHttpTransport {
    pub(crate) url: String,
    client: Client,
    headers: HashMap<String, String>,
    pub(crate) pending: VecDeque<String>,
}

impl StreamableHttpTransport {
    /// Create a new transport targeting `url`.
    #[must_use]
    pub fn new(url: &str, headers: HashMap<String, String>) -> Self {
        Self {
            url: url.to_owned(),
            client: Client::new(),
            headers,
            pending: VecDeque::new(),
        }
    }

    /// POST `msg` and buffer all non-empty lines from the response body.
    async fn post_and_buffer(&mut self, msg: &str) -> Result<(), McpError> {
        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .body(msg.to_owned());

        for (k, v) in &self.headers {
            req = req.header(k, v);
        }

        let response = req.send().await?.error_for_status()?;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let text = String::from_utf8_lossy(&chunk);
            for line in text.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    self.pending.push_back(trimmed.to_owned());
                }
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Transport for StreamableHttpTransport {
    async fn send(&mut self, msg: &str) -> Result<(), McpError> {
        self.post_and_buffer(msg).await
    }

    async fn recv(&mut self) -> Result<String, McpError> {
        self.pending
            .pop_front()
            .ok_or_else(|| McpError::UnexpectedResponse("no buffered response".to_owned()))
    }

    async fn close(&mut self) -> Result<(), McpError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streamable_http_construction() {
        let mut headers = HashMap::new();
        headers.insert("X-Api-Key".to_owned(), "secret".to_owned());
        let transport = StreamableHttpTransport::new("https://example.com/mcp", headers);
        assert_eq!(transport.url, "https://example.com/mcp");
        assert!(transport.pending.is_empty());
    }

    #[tokio::test]
    async fn test_streamable_http_recv_empty_returns_error() {
        let mut transport = StreamableHttpTransport::new("https://example.com/mcp", HashMap::new());
        // recv on an empty transport (no prior send/buffer) must return an error.
        let err = transport.recv().await;
        assert!(err.is_err());
    }
}
