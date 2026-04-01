//! TUI integration tests using the `openai-mokku-go` Docker container.
//!
//! Each test:
//!  1. Starts the mock container.
//!  2. Creates an isolated temp HOME + project directory.
//!  3. Pre-seeds the models cache so TUI startup never hits live `models.dev`.
//!  4. Writes `opencode.jsonc` with the container's base URL.
//!  5. Spawns `avocode` (TUI mode) under a PTY via [`common::pty::TuiDriver`].
//!  6. Waits for the TUI to render, types input, and asserts screen content.
//!
//! None of these tests talk to real `OpenAI` or any other live service.
#![expect(clippy::expect_used)]

mod common;

use std::time::Duration;

use common::fs::TestEnv;
use common::openai_mock::OpenAiMock;
use common::pty::TuiDriver;

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

/// Typing a message and pressing Enter in the TUI should produce a rendered
/// assistant reply containing `Echo: hello` in the chat widget.
#[tokio::test]
async fn tui_send_message_displays_echo_response() {
    // Given: a running mock container
    let mock = OpenAiMock::start().await;

    // Given: isolated environment with models cache and project config
    let env = TestEnv::new();
    env.write_models_cache();
    env.write_openai_config(&mock.base_url);

    // When: spawn avocode in TUI mode under a PTY
    // (The test runs in a blocking thread because TuiDriver uses sync I/O)
    let env_overrides = env.env_overrides();
    let project_path = env.project_path().to_owned();

    let result = tokio::task::spawn_blocking(move || {
        let mut driver = TuiDriver::spawn(&env_overrides, &project_path);

        // Then: wait for the TUI to finish rendering its initial frame
        // (status bar should show "INSERT" mode)
        let ready = driver.wait_for(|screen| screen.contains("INSERT"), Duration::from_secs(15));
        assert!(ready, "TUI did not render initial frame within 15 seconds");

        // When: type a message and press Enter
        driver.send_input("hello");
        driver.send_input("\r"); // Enter key

        // Then: wait for the echo reply to appear in the chat area
        let echoed = driver.wait_for(
            |screen| screen.contains("Echo: hello"),
            Duration::from_secs(30),
        );

        // Cleanup: send Ctrl+C to quit before asserting so the process doesn't
        // linger if the assertion panics.
        driver.send_ctrl_c();

        assert!(
            echoed,
            "expected 'Echo: hello' in TUI chat area within 30 seconds\nscreen:\n{}",
            driver.screen_contents()
        );
    })
    .await;

    result.expect("TUI test task panicked");
}

// ---------------------------------------------------------------------------
// Model picker pre-seeding
// ---------------------------------------------------------------------------

/// The TUI must start successfully with the pre-seeded models cache rather
/// than fetching from live `models.dev`.  If the TUI renders the status bar
/// we know `fetch_dynamic_providers()` succeeded from cache.
#[test]
fn tui_starts_with_pre_seeded_models_cache_without_network() {
    // Given: isolated environment with models cache (no network config needed)
    let env = TestEnv::new();
    env.write_models_cache();

    // Record no-network by pointing HOME at a place with no real models cache
    // but with our seeded one.  The test runs synchronously without tokio
    // because TuiDriver::spawn is sync.
    let env_overrides = env.env_overrides();
    let project_path = env.project_path().to_owned();

    let mut driver = TuiDriver::spawn(&env_overrides, &project_path);

    // Then: the TUI renders the status bar (proof that model loading succeeded)
    let rendered = driver.wait_for(|screen| screen.contains("INSERT"), Duration::from_secs(15));

    // Quit before asserting to avoid leaving a hanging process
    driver.send_ctrl_c();

    assert!(
        rendered,
        "TUI did not render after 15 seconds — model cache pre-seeding may have failed\n\
         screen:\n{}",
        driver.screen_contents()
    );
}
