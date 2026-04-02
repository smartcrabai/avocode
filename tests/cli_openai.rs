//! CLI integration test: `avocode --no-tui --model openai/gpt-4o --message ...`
//!
//! Starts the `openai-mokku-go` mock container, writes a project config,
//! runs the binary as a child process, and verifies stdout contains the
//! assistant echo response.

mod support;

use support::mock_openai::{MockOpenAi, write_project_config};

/// Given: the `openai-mokku-go` container is running and a temp project
///   directory contains `opencode.jsonc` pointing to the mock,
/// When: `avocode --no-tui --model openai/gpt-4o --message "Hello, echo!"`
///   is executed as a child process,
/// Then: stdout contains the echoed text from the mock assistant.
#[tokio::test]
#[ignore = "requires Docker, openai-mokku-go image, and enhanced process implementation"]
async fn cli_no_tui_sends_message_and_receives_echo()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Given: mock container + project config
    let mock = MockOpenAi::start().await?;
    let project_dir = tempfile::tempdir()?;
    write_project_config(project_dir.path(), mock.base_url())?;

    // Build the binary path (relative to target/debug)
    let binary = std::env::current_dir()?
        .join("target")
        .join("debug")
        .join("avocode");

    // When: run the CLI
    let output = tokio::process::Command::new(&binary)
        .arg("--no-tui")
        .arg("--model")
        .arg("openai/gpt-4o")
        .arg("--message")
        .arg("Hello, echo!")
        .current_dir(project_dir.path())
        .env("OPENAI_API_KEY", "dummy-key-for-testing")
        .env("XDG_CONFIG_HOME", project_dir.path().join(".config"))
        .env("XDG_DATA_HOME", project_dir.path().join(".local/share"))
        .output()
        .await?;

    // Then: exit code 0 and stdout contains assistant response
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(format!(
            "CLI exited with {}: stdout={stdout} stderr={stderr}",
            output.status
        )
        .into());
    }

    // The mock echoes back the user message, so we expect it in stdout
    assert!(
        stdout.contains("Hello") || stdout.contains("echo"),
        "expected assistant echo in stdout, got: {stdout}"
    );

    Ok(())
}

/// Given: no mock container is running,
/// When: `avocode --no-tui --model openai/gpt-4o --message "test"` is run
///   with an unreachable `base_url`,
/// Then: the CLI exits with a non-zero status or prints an error message
///   (no panic, no hang).
#[tokio::test]
#[ignore = "requires enhanced process implementation with error handling"]
async fn cli_shows_error_on_unreachable_endpoint()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let project_dir = tempfile::tempdir()?;

    // Write config pointing to unreachable URL
    write_project_config(project_dir.path(), "http://127.0.0.1:1")?;

    let binary = std::env::current_dir()?
        .join("target")
        .join("debug")
        .join("avocode");

    let output = tokio::process::Command::new(&binary)
        .arg("--no-tui")
        .arg("--model")
        .arg("openai/gpt-4o")
        .arg("--message")
        .arg("test unreachable")
        .current_dir(project_dir.path())
        .env("OPENAI_API_KEY", "dummy")
        .env("XDG_CONFIG_HOME", project_dir.path().join(".config"))
        .env("XDG_DATA_HOME", project_dir.path().join(".local/share"))
        .output()
        .await?;

    // Should not succeed (unreachable endpoint)
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either non-zero exit or error in output
    let has_error = !output.status.success()
        || stderr.contains("error")
        || stderr.contains("Error")
        || stdout.contains("error");

    assert!(
        has_error,
        "expected error indication for unreachable endpoint"
    );

    Ok(())
}

/// Given: no `OPENAI_API_KEY` env var and no API key in config,
/// When: `avocode --no-tui --model openai/gpt-4o --message "test"` is run,
/// Then: the CLI reports a clear error about missing API key.
#[tokio::test]
#[ignore = "requires enhanced process implementation with credential resolution"]
async fn cli_shows_error_on_missing_api_key() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let project_dir = tempfile::tempdir()?;

    // Write a minimal config without an API key so the CLI doesn't inherit
    // a real config from elsewhere.
    write_project_config_without_api_key(project_dir.path(), "http://127.0.0.1:18000")?;

    let binary = std::env::current_dir()?
        .join("target")
        .join("debug")
        .join("avocode");

    // Remove API key from env
    let output = tokio::process::Command::new(&binary)
        .arg("--no-tui")
        .arg("--model")
        .arg("openai/gpt-4o")
        .arg("--message")
        .arg("test no key")
        .current_dir(project_dir.path())
        .env_remove("OPENAI_API_KEY")
        .env("XDG_CONFIG_HOME", project_dir.path().join(".config"))
        .env("XDG_DATA_HOME", project_dir.path().join(".local/share"))
        .output()
        .await?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    let has_api_key_error = stderr.contains("API key")
        || stderr.contains("api_key")
        || stderr.contains("credential")
        || stdout.contains("API key")
        || stdout.contains("api_key")
        || !output.status.success();

    assert!(
        has_api_key_error,
        "expected API key error, got stdout={stdout} stderr={stderr}"
    );

    Ok(())
}

/// Create a minimal project config (`opencode.jsonc`) in `dir` with **no**
/// API key.  Useful for tests that verify error handling when credentials are
/// missing.
fn write_project_config_without_api_key(
    dir: &std::path::Path,
    base_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::io::Write;

    let config_content = format!(
        r#"{{
  "model": "openai/gpt-4o",
  "provider": {{
    "openai": {{
      "base_url": "{base_url}"
    }}
  }}
}}"#
    );

    let config_path = dir.join("opencode.jsonc");
    let mut f = std::fs::File::create(&config_path)?;
    f.write_all(config_content.as_bytes())?;

    Ok(())
}
