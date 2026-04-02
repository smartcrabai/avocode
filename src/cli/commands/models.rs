#[derive(clap::Args)]
pub struct ModelsArgs {
    /// Filter by provider
    #[arg(short, long)]
    pub provider: Option<String>,
}

/// # Errors
///
/// Returns `CliError` if the dynamic provider list cannot be loaded.
pub async fn execute(args: ModelsArgs) -> crate::cli::Result<()> {
    let config = crate::config::loader::load_global().unwrap_or_default();
    let providers = crate::provider::models_dev::fetch_dynamic_providers().await?;
    let providers = crate::provider::models_dev::filter_by_configured(
        providers,
        &config.configured_provider_ids(),
    );
    let mut choices = crate::provider::models_dev::to_model_choices(&providers);

    if let Some(ref provider_id) = args.provider {
        choices.retain(|c| &c.provider_id == provider_id);
    }

    if choices.is_empty() {
        if let Some(provider_id) = &args.provider {
            println!("No models found for provider: {provider_id}");
        } else {
            println!("No models found.");
        }
        return Ok(());
    }

    println!("Available models ({}):", choices.len());
    for c in &choices {
        let ctx_str = c
            .context_length
            .map_or_else(String::new, |ctx| format!(" ctx={ctx}"));
        println!("  {} - {}{}", c.qualified_id(), c.display_name, ctx_str);
    }
    Ok(())
}
