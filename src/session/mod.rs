pub mod compaction;
pub mod message;
pub mod processor;
pub mod schema;
pub mod store;

pub use message::{Message, MessageRole, Part};
pub use schema::{Session, new_id, now_ms};
pub use store::SessionStore;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}
