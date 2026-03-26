pub mod loader;
pub mod paths;
pub mod schema;

pub use loader::{load, load_global};
pub use schema::*;

/// Errors that can occur while loading or parsing configuration files.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Wraps an IO error encountered while reading a config file.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A parse error encountered in a specific config file.
    #[error("Parse error in {file}: {message}")]
    Parse { file: String, message: String },
}
