#[derive(clap::Args)]
pub struct ExportArgs {
    /// Session ID to export
    pub session_id: String,
    /// Output file (default: stdout)
    #[arg(short, long)]
    pub output: Option<std::path::PathBuf>,
}

/// # Errors
///
/// Returns `CliError` if the command fails.
#[expect(clippy::unused_async, reason = "async is part of the public CLI API")]
pub async fn execute(args: ExportArgs) -> crate::cli::Result<()> {
    let ctx = crate::app::AppContext::new(std::env::current_dir()?);
    let store = ctx.open_session_store()?;

    let session = super::require_session(&store, &args.session_id)?;
    let messages = store.list_messages(&args.session_id)?;

    let export_data = serde_json::json!({
        "session": session,
        "messages": messages,
    });

    let json = serde_json::to_string_pretty(&export_data)
        .map_err(|e| crate::cli::CliError::CommandFailed(e.to_string()))?;

    if let Some(path) = &args.output {
        std::fs::write(path, &json)?;
        println!("Exported to {}", path.display());
    } else {
        println!("{json}");
    }

    Ok(())
}
