pub mod catalog;
pub mod models_dev;
pub mod registry;
pub mod schema;

pub use registry::{ProviderRegistry, builtin_providers};
pub use schema::*;

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Empty catalog: API returned no providers")]
    EmptyCatalog,
}
