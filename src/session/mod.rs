pub mod compaction;
pub mod message;
pub mod processor;
pub mod schema;
pub mod store;

pub use message::{
    CompactionPart, FilePart, Message, MessageRole, Part, ReasoningPart, StepFinishPart,
    StepStartPart, TextPart, ToolPart, ToolPartState, UsageSummary, new_message_id, new_part_id,
};
pub use schema::{Session, SessionSummary, new_session_id, now_ms};
pub use store::SessionStore;

/// Errors that can occur within the session subsystem.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}
