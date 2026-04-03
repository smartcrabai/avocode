use std::collections::{HashMap, VecDeque};

use futures_util::StreamExt;
use reqwest::Client;

use crate::mcp::client::McpError;
use crate::mcp::transport::Transport;

/// Transport that receives server-sent events (SSE) from an event-source
/// endpoint and POSTs JSON-RPC requests to a separate POST endpoint.
pub struct SseTransport {
    pub(crate) post_url: String,
    pub(crate) event_source_url: String,
    client: Client,
    pending: VecDeque<String>,
    headers: HashMap<String, String>,
}

impl SseTransport {
    /// Connect to the SSE endpoint and prepare the POST URL.
    ///
    /// By convention the POST URL is `{url}/message` and the SSE endpoint is
    /// `{url}/sse`, but some servers use the same URL for both -- in that case
    /// the caller should pass the full URLs directly.
    ///
    /// # Errors
    ///
    /// Returns an error if the `reqwest` client cannot be constructed.
    pub fn new(url: &str, headers: HashMap<String, String>) -> Result<Self, McpError> {
        // Derive SSE and POST endpoints from the base URL.
        let (event_source_url, post_url) = if url.ends_with("/sse") {
            (
                url.to_owned(),
                format!("{}/message", url.trim_end_matches("/sse")),
            )
        } else {
            (format!("{url}/sse"), format!("{url}/message"))
        };

        let client = Client::new();

        Ok(Self {
            post_url,
            event_source_url,
            client,
            pending: VecDeque::new(),
            headers,
        })
    }

    /// Fetch the next batch of SSE events from the server and buffer them.
    async fn fetch_events(&mut self) -> Result<(), McpError> {
        let mut req = self.client.get(&self.event_source_url);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }

        let response = req.send().await?;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let text = String::from_utf8_lossy(&chunk);
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data:") {
                    let trimmed = data.trim();
                    if !trimmed.is_empty() {
                        self.pending.push_back(trimmed.to_owned());
                    }
                }
            }
            if !self.pending.is_empty() {
                break;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Transport for SseTransport {
    async fn send(&mut self, msg: &str) -> Result<(), McpError> {
        let mut req = self
            .client
            .post(&self.post_url)
            .header("Content-Type", "application/json")
            .body(msg.to_owned());

        for (k, v) in &self.headers {
            req = req.header(k, v);
        }

        req.send().await?.error_for_status()?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<String, McpError> {
        while self.pending.is_empty() {
            self.fetch_events().await?;
        }
        self.pending
            .pop_front()
            .ok_or_else(|| McpError::UnexpectedResponse("empty event queue".to_owned()))
    }

    async fn close(&mut self) -> Result<(), McpError> {
        // HTTP connections are stateless; nothing to tear down explicitly.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_url_derivation_from_base() -> Result<(), Box<dyn std::error::Error>> {
        let transport = SseTransport::new("https://example.com/mcp", HashMap::new())?;
        assert_eq!(transport.event_source_url, "https://example.com/mcp/sse");
        assert_eq!(transport.post_url, "https://example.com/mcp/message");
        Ok(())
    }

    #[test]
    fn test_sse_url_already_sse() -> Result<(), Box<dyn std::error::Error>> {
        let transport = SseTransport::new("https://example.com/mcp/sse", HashMap::new())?;
        assert_eq!(transport.event_source_url, "https://example.com/mcp/sse");
        assert_eq!(transport.post_url, "https://example.com/mcp/message");
        Ok(())
    }
}
