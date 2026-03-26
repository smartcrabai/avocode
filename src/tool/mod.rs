pub mod bash;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod ls;
pub mod read;
pub mod registry;
pub mod schema;
pub mod webfetch;
pub mod write;

pub use registry::ToolRegistry;
pub use schema::{
    MAX_OUTPUT_BYTES, MAX_OUTPUT_LINES, Tool, ToolContext, ToolOutput, resolve_path,
    truncate_output,
};

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("Execution error: {0}")]
    Execution(String),
    #[error("Operation aborted")]
    Aborted,
}
