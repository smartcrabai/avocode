#[derive(clap::Args)]
pub struct ModelsArgs {
    /// Filter by provider
    #[arg(short, long)]
    pub provider: Option<String>,
}

/// # Errors
///
/// Returns `CliError` if the command fails.
#[expect(clippy::unused_async, reason = "async is part of the public CLI API")]
pub async fn execute(args: ModelsArgs) -> crate::cli::Result<()> {
    let registry = crate::provider::ProviderRegistry::new(crate::provider::builtin_providers());

    let models: Vec<&crate::provider::ModelInfo> = if let Some(ref provider_id) = args.provider {
        registry.list_models(provider_id)
    } else {
        registry.all_models()
    };

    if models.is_empty() {
        if let Some(provider_id) = &args.provider {
            println!("No models found for provider: {provider_id}");
        } else {
            println!("No models found.");
        }
        return Ok(());
    }

    println!("Available models ({}):", models.len());
    for m in &models {
        let ctx_str = m
            .context_length
            .map_or_else(String::new, |c| format!(" ctx={c}"));
        println!("  {}/{} - {}{}", m.provider_id, m.id, m.name, ctx_str);
    }
    Ok(())
}
