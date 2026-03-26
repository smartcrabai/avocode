use clap::Subcommand;

#[derive(clap::Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub action: SessionAction,
}

#[derive(Subcommand)]
pub enum SessionAction {
    /// List all sessions
    List,
    /// Show session details
    Show { id: String },
    /// Delete a session
    Delete { id: String },
}

/// # Errors
///
/// Returns `CliError` if the command fails.
#[expect(
    clippy::unused_async,
    reason = "placeholder until async operations are integrated"
)]
pub async fn execute(args: SessionArgs) -> crate::cli::Result<()> {
    match args.action {
        SessionAction::List => println!("Sessions: (not yet integrated)"),
        SessionAction::Show { id } => println!("Session {id}: (not yet integrated)"),
        SessionAction::Delete { id } => println!("Deleted session {id}: (not yet integrated)"),
    }
    Ok(())
}
