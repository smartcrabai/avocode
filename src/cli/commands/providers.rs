/// # Errors
///
/// Returns `CliError` if the command fails.
#[expect(
    clippy::unused_async,
    reason = "placeholder until async operations are integrated"
)]
pub async fn execute() -> crate::cli::Result<()> {
    let providers = [
        "anthropic",
        "openai",
        "google",
        "github-copilot",
        "openai-codex",
        "xai",
        "mistral",
        "groq",
        "azure",
        "bedrock",
        "vertex",
        "openrouter",
        "cohere",
        "together",
        "perplexity",
        "deepinfra",
        "cerebras",
        "gitlab",
        "vercel",
    ];
    println!("Available providers:");
    for p in &providers {
        println!("  - {p}");
    }
    Ok(())
}
