const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4-5";

#[derive(clap::Args)]
pub struct RunArgs {
    /// Run in non-interactive mode with this prompt
    #[arg(short, long)]
    pub message: Option<String>,
    #[arg(short, long)]
    pub session: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    /// Non-interactive mode: print output and exit
    #[arg(long, requires = "message")]
    pub no_tui: bool,
}

/// # Errors
///
/// Returns `CliError` if the command fails.
pub async fn execute(args: RunArgs) -> crate::cli::Result<()> {
    if !args.no_tui {
        crate::tui::run().await?;
        return Ok(());
    }

    let ctx = crate::app::AppContext::new(std::env::current_dir()?);
    let model = args.model.unwrap_or_else(|| DEFAULT_MODEL.to_owned());
    let store = ctx.open_session_store()?;

    let session = if let Some(ref session_id) = args.session {
        super::require_session(&store, session_id)?
    } else {
        let s = crate::session::Session::new(
            ctx.project_id().to_string(),
            ctx.project_root().display().to_string(),
        );
        store.create_session(&s)?;
        s
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let options = crate::session::processor::ProcessOptions {
        session_id: session.id,
        // clap `requires = "message"` guarantees this is Some when --no-tui is set
        user_message: args.message.unwrap_or_default(),
        model,
        agent: "default".to_owned(),
        max_turns: None,
    };
    crate::session::processor::process(&store, options, tx).await?;

    while let Some(event) = rx.recv().await {
        match event {
            crate::session::processor::ProcessEvent::MessageCreated { message } => {
                for part in &message.parts {
                    if let crate::session::Part::Text(t) = part {
                        println!("{}", t.text);
                    }
                }
            }
            crate::session::processor::ProcessEvent::Done => break,
            _ => {}
        }
    }

    Ok(())
}
