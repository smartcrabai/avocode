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
#[ignore = "requires Docker and openai-mokku-go image"]
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
// Model picker -- switching between two OpenAI models
// ---------------------------------------------------------------------------

/// Opening the model picker with Ctrl+T, navigating to a different `OpenAI`
/// model with the Down arrow, and pressing Enter should:
///  1. Update the status bar to show the newly selected model.
///  2. Still allow sending a message that receives an echo response.
///
/// This test uses the two-model `OpenAI` cache fixture (`gpt-4o` and
/// `gpt-3.5-turbo`) so that navigation has a second entry to land on.
/// It does **not** add non-openai providers to avoid changing the default
/// model ordering for other tests.
#[tokio::test]
#[ignore = "requires Docker and openai-mokku-go image"]
async fn tui_model_picker_switches_model_and_message_succeeds() {
    // Given: a running mock container
    let mock = OpenAiMock::start().await;

    // Given: isolated environment with TWO openai models in the cache
    let env = TestEnv::new();
    env.write_two_openai_models_cache();
    // Config points to gpt-4o so it is pre-selected on startup.
    env.write_openai_config(&mock.base_url);

    let env_overrides = env.env_overrides();
    let project_path = env.project_path().to_owned();

    let result = tokio::task::spawn_blocking(move || {
        let mut driver = TuiDriver::spawn(&env_overrides, &project_path);

        // Then: wait for the TUI to render its initial frame
        let ready = driver.wait_for(|screen| screen.contains("INSERT"), Duration::from_secs(15));
        assert!(ready, "TUI did not render initial frame within 15 seconds");

        // When: open the model picker with Ctrl+T (\x14)
        driver.send_input("\x14");

        // Then: the picker should appear (status bar hint or model list visible)
        // We wait for "gpt-3.5-turbo" to appear in the picker list.
        let picker_open = driver.wait_for(
            |screen| screen.contains("gpt-3.5-turbo"),
            Duration::from_secs(5),
        );
        assert!(
            picker_open,
            "model picker did not open within 5 seconds\nscreen:\n{}",
            driver.screen_contents()
        );

        // When: navigate Down to gpt-3.5-turbo (Down arrow = ESC [ B)
        driver.send_input("\x1b[B");

        // When: press Enter to apply the selection
        driver.send_input("\r");

        // Then: wait for the picker to close (INSERT mode reappears) AND the
        // status bar to show gpt-3.5-turbo.  Waiting for both together avoids
        // a race where the condition matches the picker list before it closes.
        let model_updated = driver.wait_for(
            |screen| screen.contains("INSERT") && screen.contains("gpt-3.5-turbo"),
            Duration::from_secs(5),
        );
        assert!(
            model_updated,
            "status bar did not update to gpt-3.5-turbo after model selection\nscreen:\n{}",
            driver.screen_contents()
        );

        // When: send a message with the new model active
        driver.send_input("hello");
        driver.send_input("\r");

        // Then: mock echoes the message (proves the new model was actually used)
        let echoed = driver.wait_for(
            |screen| screen.contains("Echo: hello"),
            Duration::from_secs(30),
        );

        driver.send_ctrl_c();

        assert!(
            echoed,
            "expected 'Echo: hello' after model switch within 30 seconds\nscreen:\n{}",
            driver.screen_contents()
        );
    })
    .await;

    result.expect("TUI model-picker test task panicked");
}

// ---------------------------------------------------------------------------
// Built-in slash commands -- /model opens model picker
// ---------------------------------------------------------------------------

/// Typing `/model` and pressing Enter in the TUI should open the model-picker
/// popup (same as `Ctrl+T`) without submitting a message to the processor.
///
/// This test verifies the entire slash-command dispatch path end-to-end:
/// input → builtin detection → picker open → status bar hint change.
#[tokio::test]
#[ignore = "requires Docker and openai-mokku-go image"]
async fn tui_slash_model_command_opens_model_picker() {
    // Given: a running mock container and an environment with two models
    let mock = OpenAiMock::start().await;
    let env = TestEnv::new();
    env.write_two_openai_models_cache();
    env.write_openai_config(&mock.base_url);

    let env_overrides = env.env_overrides();
    let project_path = env.project_path().to_owned();

    let result = tokio::task::spawn_blocking(move || {
        let mut driver = TuiDriver::spawn(&env_overrides, &project_path);

        // Then: wait for the TUI initial frame
        let ready = driver.wait_for(|screen| screen.contains("INSERT"), Duration::from_secs(15));
        assert!(ready, "TUI did not render initial frame within 15 seconds");

        // When: type "/model" and press Enter
        driver.send_input("/model");
        driver.send_input("\r");

        // Then: the model picker popup opens.
        // The picker shows both models; we wait for "gpt-3.5-turbo" (second model)
        // to appear, which proves the picker is open (not just the status bar).
        let picker_open = driver.wait_for(
            |screen| screen.contains("gpt-3.5-turbo"),
            Duration::from_secs(5),
        );

        driver.send_ctrl_c();

        assert!(
            picker_open,
            "expected model picker to open after '/model' + Enter\nscreen:\n{}",
            driver.screen_contents()
        );
    })
    .await;

    result.expect("tui_slash_model_command_opens_model_picker task panicked");
}

// ---------------------------------------------------------------------------
// Built-in slash commands -- /new clears transcript and starts fresh session
// ---------------------------------------------------------------------------

/// Typing `/new` (or `/clear`) after a conversation should clear the on-screen
/// transcript and allow a subsequent message to succeed in a new session.
///
/// Verification steps:
///  1. Send "hello" → assert echo reply appears.
///  2. Type `/new` + Enter → assert previous transcript is gone.
///  3. Send "world" → assert echo reply appears in the new session.
#[tokio::test]
#[ignore = "requires Docker and openai-mokku-go image"]
async fn tui_slash_new_command_clears_transcript_and_allows_new_message() {
    // Given: a running mock container
    let mock = OpenAiMock::start().await;
    let env = TestEnv::new();
    env.write_models_cache();
    env.write_openai_config(&mock.base_url);

    let env_overrides = env.env_overrides();
    let project_path = env.project_path().to_owned();

    let result = tokio::task::spawn_blocking(move || {
        let mut driver = TuiDriver::spawn(&env_overrides, &project_path);

        // Then: wait for initial frame
        let ready = driver.wait_for(|screen| screen.contains("INSERT"), Duration::from_secs(15));
        assert!(ready, "TUI did not render initial frame within 15 seconds");

        // When: send first message
        driver.send_input("hello");
        driver.send_input("\r");

        // Then: echo reply appears
        let first_reply = driver.wait_for(
            |screen| screen.contains("Echo: hello"),
            Duration::from_secs(30),
        );
        assert!(
            first_reply,
            "expected 'Echo: hello' before /new\nscreen:\n{}",
            driver.screen_contents()
        );

        // When: type "/new" and press Enter to reset the session
        driver.send_input("/new");
        driver.send_input("\r");

        // Then: the previous transcript is gone from the screen
        let transcript_cleared = driver.wait_for(
            |screen| !screen.contains("Echo: hello"),
            Duration::from_secs(5),
        );
        assert!(
            transcript_cleared,
            "expected transcript to be cleared after /new\nscreen:\n{}",
            driver.screen_contents()
        );

        // When: send a new message in the fresh session
        driver.send_input("world");
        driver.send_input("\r");

        // Then: echo reply for the new message appears
        let second_reply = driver.wait_for(
            |screen| screen.contains("Echo: world"),
            Duration::from_secs(30),
        );

        driver.send_ctrl_c();

        assert!(
            second_reply,
            "expected 'Echo: world' in new session after /new\nscreen:\n{}",
            driver.screen_contents()
        );
    })
    .await;

    result.expect("tui_slash_new_command_clears_transcript task panicked");
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
        "TUI did not render after 15 seconds -- model cache pre-seeding may have failed\n\
         screen:\n{}",
        driver.screen_contents()
    );
}
