pub mod evaluate;
pub mod manager;
pub mod schema;
pub mod wildcard;

pub use evaluate::evaluate;
pub use manager::PermissionManager;
pub use schema::*;

#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("Request not found: {0}")]
    NotFound(String),
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Internal error: {0}")]
    Internal(String),
}
