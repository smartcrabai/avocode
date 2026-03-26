#[derive(clap::Args)]
pub struct RunArgs {
    #[arg(short, long)]
    pub message: Option<String>,
    #[arg(short, long)]
    pub session: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    /// Non-interactive mode: print output and exit
    #[arg(long)]
    pub no_tui: bool,
}

/// # Errors
///
/// Returns `CliError` if the command fails.
#[expect(
    clippy::unused_async,
    reason = "placeholder until async operations are integrated"
)]
pub async fn execute(args: RunArgs) -> crate::cli::Result<()> {
    if args.no_tui {
        if let Some(msg) = &args.message {
            println!("avocode: processing '{msg}' (non-interactive mode - not yet implemented)");
        }
    } else {
        println!("avocode: TUI mode (not yet integrated)");
    }
    Ok(())
}
