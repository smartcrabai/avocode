pub mod export;
pub mod mcp;
pub mod models;
pub mod providers;
pub mod run;
pub mod serve;
pub mod session;

/// Look up a session by ID, returning a CLI error if not found.
///
/// # Errors
/// Returns [`super::CliError::CommandFailed`] if the session does not exist,
/// or propagates store errors.
pub fn require_session(
    store: &crate::session::SessionStore,
    id: &str,
) -> super::Result<crate::session::Session> {
    store
        .get_session(id)?
        .ok_or_else(|| super::CliError::CommandFailed(format!("session not found: {id}")))
}
