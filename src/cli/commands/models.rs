#[derive(clap::Args)]
pub struct ModelsArgs {
    /// Filter by provider
    #[arg(short, long)]
    pub provider: Option<String>,
}

/// # Errors
///
/// Returns `CliError` if the command fails.
#[expect(
    clippy::unused_async,
    reason = "placeholder until async operations are integrated"
)]
pub async fn execute(args: ModelsArgs) -> crate::cli::Result<()> {
    println!("avocode models: listing models (not yet integrated)");
    if let Some(provider) = &args.provider {
        println!("  Filter: provider={provider}");
    }
    Ok(())
}
