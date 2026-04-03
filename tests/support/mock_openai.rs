//! Helper to start `ghcr.io/takumi3488/openai-mokku-go` via testcontainers.
//!
//! The mock provides OpenAI-compatible endpoints:
//! - `GET  /v1/models`           -> lists a single `gpt-4o` model
//! - `POST /v1/chat/completions` -> echoes the user message (streaming SSE)
//!
//! # Platform note
//!
//! The published image is `linux/amd64` only.  On Apple Silicon (aarch64)
//! Docker will emulate via Rosetta/QEMU.  If you see *image platform mismatch*
//! errors, ensure Docker Desktop >= 4.19 with Rosetta enabled, or set
//! `DOCKER_DEFAULT_PLATFORM=linux/amd64`.

use std::time::Duration;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::{GenericImage, runners::AsyncRunner};

const IMAGE: &str = "ghcr.io/takumi3488/openai-mokku-go";
const TAG: &str = "latest";
const INTERNAL_PORT: u16 = 8080;
const MAX_READY_POLLS: u32 = 60;
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// A running `openai-mokku-go` container with a known `base_url`.
pub struct MockOpenAi {
    base_url: String,
    _container: testcontainers::ContainerAsync<GenericImage>,
}

impl MockOpenAi {
    /// Start the mock container and wait for readiness.
    ///
    /// Readiness is determined by polling `GET /v1/models` until it returns
    /// HTTP 200 or the timeout elapses.
    ///
    /// # Errors
    ///
    /// Returns an error if the container cannot be started or if the mock
    /// server does not become ready within the timeout.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // On aarch64 macOS, force the amd64 image.
        if cfg!(target_arch = "aarch64") && cfg!(target_os = "macos") {
            // SAFETY: test-only env mutation to select the correct image platform.
            unsafe { std::env::set_var("DOCKER_DEFAULT_PLATFORM", "linux/amd64") };
        }

        let container = GenericImage::new(IMAGE, TAG)
            .with_exposed_port(INTERNAL_PORT.tcp())
            .with_wait_for(WaitFor::seconds(2))
            .start()
            .await?;

        let host = container.get_host().await?;
        let port = container.get_host_port_ipv4(INTERNAL_PORT).await?;
        let base_url = format!("http://{host}:{port}");

        let mock = Self {
            base_url,
            _container: container,
        };

        mock.wait_ready().await?;
        Ok(mock)
    }

    /// Returns the base URL for the mock server (e.g., `http://127.0.0.1:32768`).
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Poll `GET /v1/models` until the mock is ready.
    async fn wait_ready(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;

        let url = format!("{}/v1/models", self.base_url);

        for _ in 0..MAX_READY_POLLS {
            if let Ok(resp) = client.get(&url).send().await
                && resp.status().is_success()
            {
                return Ok(());
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        let timeout_secs = u64::from(MAX_READY_POLLS)
            * u64::try_from(POLL_INTERVAL.as_millis()).unwrap_or(0)
            / 1000;
        Err(
            format!("mock openai server at {url} did not become ready within {timeout_secs}s")
                .into(),
        )
    }
}

/// Create a minimal project config (`opencode.jsonc`) in `dir` that points to
/// the mock server.
///
/// The config sets:
/// - `model`: `"openai/gpt-4o"`
/// - `provider.openai.base_url`: the mock's base URL
/// - `provider.openai.api_key`: a dummy key (required by the mock)
pub fn write_project_config(
    dir: &std::path::Path,
    base_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::io::Write;

    let config_content = format!(
        r#"{{
  "model": "openai/gpt-4o",
  "provider": {{
    "openai": {{
      "base_url": "{base_url}",
      "api_key": "dummy-key-for-testing"
    }}
  }}
}}"#
    );

    let config_path = dir.join("opencode.jsonc");
    let mut f = std::fs::File::create(&config_path)?;
    f.write_all(config_content.as_bytes())?;

    Ok(())
}

#[cfg(test)]
#[expect(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn write_project_config_creates_valid_jsonc() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_project_config(dir.path(), "http://localhost:9999").expect("write config");

        let content =
            std::fs::read_to_string(dir.path().join("opencode.jsonc")).expect("read config");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("parse json");
        assert_eq!(parsed["model"], "openai/gpt-4o");
        assert_eq!(
            parsed["provider"]["openai"]["base_url"],
            "http://localhost:9999"
        );
        assert_eq!(
            parsed["provider"]["openai"]["api_key"],
            "dummy-key-for-testing"
        );
    }
}
