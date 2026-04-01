#![allow(dead_code)]
#![expect(clippy::expect_used)]

use testcontainers::core::ContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage};

/// Manages a running `ghcr.io/takumi3488/openai-mokku-go:latest` container.
///
/// The container is stopped when this struct is dropped.
///
/// # Docker platform note
///
/// This image is published for `linux/amd64` only.  On Apple Silicon hosts,
/// Docker Desktop's Rosetta emulation handles the translation automatically.
/// In CI environments set `DOCKER_DEFAULT_PLATFORM=linux/amd64` to ensure the
/// correct image variant is pulled.
///
/// # Upstream contract (verified locally)
///
/// - Listens on port `8080`.
/// - `GET /v1/models` returns a list that includes `gpt-4o` and `gpt-4o-mini`.
/// - `POST /v1/chat/completions` (streaming) echoes the last user message as
///   `Echo: <message>` in the assistant content delta.
/// - Using model `credit-error` triggers a quota/credit error response.
pub struct OpenAiMock {
    /// Holds the container alive for the duration of the test.
    _container: ContainerAsync<GenericImage>,
    /// `http://127.0.0.1:<mapped-port>` — ready to use as `provider.openai.base_url`.
    pub base_url: String,
}

impl OpenAiMock {
    /// Start the container and wait until `/v1/models` returns HTTP 200.
    ///
    /// Panics if Docker is unavailable, the image cannot be started, or the
    /// server does not become ready within 30 seconds.
    pub async fn start() -> Self {
        let container = GenericImage::new("ghcr.io/takumi3488/openai-mokku-go", "latest")
            .with_exposed_port(ContainerPort::Tcp(8080))
            .start()
            .await
            .expect(
                "failed to start ghcr.io/takumi3488/openai-mokku-go:latest; \
                 ensure Docker is running and can pull/run linux/amd64 images",
            );

        let port = container
            .get_host_port_ipv4(8080)
            .await
            .expect("failed to get host port mapped to container port 8080");

        let base_url = format!("http://127.0.0.1:{port}");

        wait_for_http_ready(&base_url).await;

        OpenAiMock {
            _container: container,
            base_url,
        }
    }
}

/// Poll `GET <base_url>/v1/models` until HTTP 200 is returned.
///
/// Panics after 30 seconds.
async fn wait_for_http_ready(base_url: &str) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("failed to build reqwest client for readiness check");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);

    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "openai-mokku-go container did not become ready within 30 seconds"
        );

        match client.get(format!("{base_url}/v1/models")).send().await {
            Ok(r) if r.status().is_success() => return,
            _ => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
        }
    }
}
