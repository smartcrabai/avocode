pub mod builtin;
pub mod prompts;
pub mod schema;

pub use builtin::{builtin_agents, resolve_agent};
pub use prompts::{assemble_system, system_prompt_for_model};
pub use schema::{AgentInfo, AgentMode, AgentPermissionRule, PermissionAction};

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Agent not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}
