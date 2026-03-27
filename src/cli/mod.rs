use clap::{Parser, Subcommand};

pub mod commands;
pub mod ui;

use commands::{
    export::ExportArgs, mcp::McpArgs, models::ModelsArgs, run::RunArgs, serve::ServeArgs,
    session::SessionArgs,
};

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Session(#[from] crate::session::SessionError),
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error(transparent)]
    Server(#[from] crate::server::ServerError),
    #[error(transparent)]
    Tui(#[from] crate::tui::TuiError),
}

pub type Result<T> = std::result::Result<T, CliError>;

#[derive(Parser)]
#[command(name = "avocode", about = "AI coding agent", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Run in non-interactive mode with this prompt
    #[arg(short, long)]
    pub message: Option<String>,

    /// Session ID to continue
    #[arg(short, long)]
    pub session: Option<String>,

    /// Model to use (e.g. "anthropic/claude-opus-4-5")
    #[arg(long)]
    pub model: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start interactive session (default)
    Run(RunArgs),
    /// Start HTTP API server
    Serve(ServeArgs),
    /// List available providers
    Providers,
    /// List available models
    Models(ModelsArgs),
    /// Session management
    Session(SessionArgs),
    /// MCP server management
    Mcp(McpArgs),
    /// Export session
    Export(ExportArgs),
}

/// Run the CLI, dispatching to the appropriate subcommand.
///
/// # Errors
///
/// Returns `CliError` if the subcommand fails.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => {
            let args = RunArgs {
                message: cli.message,
                session: cli.session,
                model: cli.model,
                no_tui: false,
            };
            commands::run::execute(args).await
        }
        Some(Commands::Run(args)) => commands::run::execute(args).await,
        Some(Commands::Serve(args)) => commands::serve::execute(args).await,
        Some(Commands::Providers) => commands::providers::execute().await,
        Some(Commands::Models(args)) => commands::models::execute(args).await,
        Some(Commands::Session(args)) => commands::session::execute(args).await,
        Some(Commands::Mcp(args)) => commands::mcp::execute(args).await,
        Some(Commands::Export(args)) => commands::export::execute(args).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
