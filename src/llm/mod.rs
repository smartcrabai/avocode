pub mod anthropic;
pub mod google;
pub mod messages;
pub mod openai;
pub mod sse;

pub use messages::*;

/// Shared HTTP client for all LLM providers.
///
/// A single `reqwest::Client` manages one connection pool for all providers,
/// which is more efficient than per-provider clients.
pub(crate) static HTTP_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

/// Extract a `"index"` field from a JSON object as `usize`, defaulting to 0.
pub(crate) fn json_index(v: &serde_json::Value) -> usize {
    usize::try_from(
        v.get("index")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    )
    .unwrap_or(0)
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("API error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("Stream ended unexpectedly")]
    StreamEnded,
    #[error("{0}")]
    Other(String),
}
