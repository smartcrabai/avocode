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
        crate::tui::run(args.model).await?;
        return Ok(());
    }

    let ctx = crate::app::AppContext::new(std::env::current_dir()?);
    let store = std::sync::Arc::new(ctx.open_session_store()?);

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
        user_message: args
            .message
            .expect("clap `requires = \"message\"` guarantees this is Some"),
        model: args.model,
        agent: "default".to_owned(),
        max_turns: None,
    };
    // Spawn the processor so we can drain the channel concurrently.
    // Without this, a response longer than the channel capacity (64 chunks) would
    // fill the channel and deadlock before the drain loop below starts.
    let store_clone = std::sync::Arc::clone(&store);
    let proc_handle =
        tokio::spawn(
            async move { crate::session::processor::process(&store_clone, options, tx).await },
        );

    let mut error_message: Option<String> = None;
    while let Some(event) = rx.recv().await {
        match event {
            crate::session::processor::ProcessEvent::PartUpdated { part, .. } => {
                if let crate::session::Part::Text(t) = part {
                    print!("{}", t.text);
                }
            }
            crate::session::processor::ProcessEvent::Done => {
                println!();
                break;
            }
            crate::session::processor::ProcessEvent::Error(e) => {
                eprintln!("Error: {e}");
                error_message = Some(e);
                break;
            }
            crate::session::processor::ProcessEvent::MessageCreated { .. } => {}
        }
    }

    proc_handle
        .await
        .map_err(|e| crate::cli::CliError::CommandFailed(e.to_string()))??;

    if let Some(e) = error_message {
        return Err(crate::cli::CliError::CommandFailed(e));
    }

    Ok(())
}
