pub mod codex;
pub mod copilot;
pub mod oauth;
pub mod store;

pub use oauth::{DeviceFlowSession, OAuthTokens};
pub use store::{AuthInfo, AuthStore};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Auth flow timed out")]
    Timeout,
    #[error("Auth was rejected: {0}")]
    Rejected(String),
    #[error("Token expired")]
    TokenExpired,
    #[error("{0}")]
    Other(String),
}
