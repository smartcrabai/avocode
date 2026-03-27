/// # Errors
///
/// Returns `CliError` if the command fails.
#[expect(clippy::unused_async, reason = "async is part of the public CLI API")]
pub async fn execute() -> crate::cli::Result<()> {
    let registry = crate::provider::ProviderRegistry::new(crate::provider::builtin_providers());
    let providers = registry.list_providers();

    println!("Available providers ({}):", providers.len());
    for p in &providers {
        let key_status = if crate::provider::ProviderRegistry::has_api_key(p) {
            "configured"
        } else if p.env.is_empty() {
            "oauth"
        } else {
            "not configured"
        };
        println!("  {} ({}) [{}]", p.name, p.id, key_status);
    }
    Ok(())
}
