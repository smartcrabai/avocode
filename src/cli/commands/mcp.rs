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
#[expect(clippy::unused_async, reason = "async is part of the public CLI API")]
pub async fn execute(args: McpArgs) -> crate::cli::Result<()> {
    let cwd = std::env::current_dir()?;
    let config = crate::config::load(&cwd)?;

    match args.action {
        McpAction::List => {
            if config.mcp.is_empty() {
                println!("No MCP servers configured.");
            } else {
                println!("MCP servers ({}):", config.mcp.len());
                for (name, mcp_config) in &config.mcp {
                    let type_str = match mcp_config {
                        crate::config::McpConfig::Stdio { command, .. } => {
                            format!("stdio ({command})")
                        }
                        crate::config::McpConfig::Sse { url, .. } => {
                            format!("sse ({url})")
                        }
                    };
                    println!("  {name} - {type_str}");
                }
            }
        }
        McpAction::Tools { server } => {
            if let Some(mcp_config) = config.mcp.get(&server) {
                let type_str = match mcp_config {
                    crate::config::McpConfig::Stdio { command, args, .. } => {
                        format!("stdio: {} {}", command, args.join(" "))
                    }
                    crate::config::McpConfig::Sse { url, .. } => {
                        format!("sse: {url}")
                    }
                };
                println!("Server: {server}");
                println!("  Type: {type_str}");
                println!("  (connect to server to discover tools)");
            } else {
                return Err(crate::cli::CliError::CommandFailed(format!(
                    "MCP server not found: {server}"
                )));
            }
        }
    }
    Ok(())
}
