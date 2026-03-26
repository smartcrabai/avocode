use clap::Subcommand;

#[derive(clap::Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub action: McpAction,
}

#[derive(Subcommand)]
pub enum McpAction {
    /// List configured MCP servers
    List,
    /// Show MCP server tools
    Tools { server: String },
}

/// # Errors
///
/// Returns `CliError` if the command fails.
#[expect(
    clippy::unused_async,
    reason = "placeholder until async operations are integrated"
)]
pub async fn execute(args: McpArgs) -> crate::cli::Result<()> {
    match args.action {
        McpAction::List => println!("MCP servers: (not yet integrated)"),
        McpAction::Tools { server } => println!("Tools for {server}: (not yet integrated)"),
    }
    Ok(())
}
