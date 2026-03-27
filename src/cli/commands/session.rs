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
    /// Archive a session
    Archive { id: String },
}

/// # Errors
///
/// Returns `CliError` if the command fails.
#[expect(clippy::unused_async, reason = "async is part of the public CLI API")]
pub async fn execute(args: SessionArgs) -> crate::cli::Result<()> {
    let ctx = crate::app::AppContext::new(std::env::current_dir()?);
    let store = ctx.open_session_store()?;

    match args.action {
        SessionAction::List => {
            let sessions = store.list_sessions(&ctx.project_id().to_string())?;
            if sessions.is_empty() {
                println!("No sessions found.");
            } else {
                println!("Sessions ({}):", sessions.len());
                for s in &sessions {
                    let title = s.title.as_deref().unwrap_or("(untitled)");
                    let date = chrono::DateTime::from_timestamp_millis(s.time_created)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default();
                    println!("  {} - {} [{}]", s.id, title, date);
                }
            }
        }
        SessionAction::Show { id } => {
            let session = super::require_session(&store, &id)?;
            let messages = store.list_messages(&id)?;
            println!("Session: {}", session.id);
            println!(
                "  Title:   {}",
                session.title.as_deref().unwrap_or("(untitled)")
            );
            println!("  Project: {}", session.project_id);
            println!("  Dir:     {}", session.directory);
            println!("  Messages: {}", messages.len());
            let date = chrono::DateTime::from_timestamp_millis(session.time_created)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default();
            println!("  Created: {date}");
        }
        SessionAction::Archive { id } => {
            store.archive_session(&id)?;
            println!("Session {id} archived.");
        }
    }
    Ok(())
}
