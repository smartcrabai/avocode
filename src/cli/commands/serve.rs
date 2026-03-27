#[derive(clap::Args)]
pub struct ServeArgs {
    #[arg(short, long, default_value = "3000")]
    pub port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
}

/// # Errors
///
/// Returns `CliError` if the command fails.
pub async fn execute(args: ServeArgs) -> crate::cli::Result<()> {
    println!("avocode serve: starting on {}:{}", args.host, args.port);
    crate::server::serve(&args.host, args.port).await?;
    Ok(())
}
