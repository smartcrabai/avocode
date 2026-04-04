//! CLI integration tests using the `openai-mokku-go` Docker container.
//!
//! Each test:
//!  1. Starts the container once (shared within the test).
//!  2. Creates an isolated temp HOME + project directory.
//!  3. Writes `opencode.jsonc` with the container's base URL.
//!  4. Spawns `avocode run --no-tui --message ...` as a subprocess.
//!  5. Asserts stdout / exit code.
//!
//! None of these tests talk to real `OpenAI` or any other live service.

mod common;

use std::time::Duration;

use common::fs::TestEnv;
use common::openai_mock::OpenAiMock;
use common::process::run_avocode;

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

/// `avocode run --no-tui --message "hello"` should print the assistant reply
/// containing `Echo: hello` and exit with status 0.
#[tokio::test]
#[ignore = "requires Docker, openai-mokku-go image, and enhanced process implementation"]
async fn cli_happy_path_echoes_user_message_and_exits_zero() {
    // Given: a running OpenAI-compatible mock container
    let mock = OpenAiMock::start().await;

    // Given: an isolated project directory with config pointing at the mock
    let env = TestEnv::new();
    env.write_openai_config(&mock.base_url);

    // When: running avocode in non-interactive mode
    let out = run_avocode(
        &[
            "run",
            "--no-tui",
            "--message",
            "hello",
            "--model",
            "openai/gpt-4o",
        ],
        &env.env_overrides(),
        env.project_path(),
        Duration::from_secs(30),
    )
    .await;

    // Then: stdout contains the echoed assistant reply
    assert!(
        out.stdout.contains("Echo: hello"),
        "expected 'Echo: hello' in stdout, got:\nstdout: {}\nstderr: {}",
        out.stdout,
        out.stderr,
    );

    // Then: the process exits with status 0
    assert!(
        out.status.success(),
        "expected exit status 0, got {:?}\nstdout: {}\nstderr: {}",
        out.status,
        out.stdout,
        out.stderr,
    );
}

/// The model can also be taken from `opencode.jsonc`; the `--model` flag
/// is optional when the config already declares a model.
#[tokio::test]
#[ignore = "requires Docker, openai-mokku-go image, and enhanced process implementation"]
async fn cli_uses_model_from_config_when_flag_is_omitted() {
    // Given: config already sets model to openai/gpt-4o
    let mock = OpenAiMock::start().await;
    let env = TestEnv::new();
    env.write_openai_config(&mock.base_url);

    // When: running without --model flag
    let out = run_avocode(
        &["run", "--no-tui", "--message", "world"],
        &env.env_overrides(),
        env.project_path(),
        Duration::from_secs(30),
    )
    .await;

    // Then: the echo reply is still produced
    assert!(
        out.stdout.contains("Echo: world"),
        "expected 'Echo: world' in stdout\nstdout: {}\nstderr: {}",
        out.stdout,
        out.stderr,
    );
    assert!(out.status.success());
}

// ---------------------------------------------------------------------------
// Multi-word message
// ---------------------------------------------------------------------------

/// Messages containing spaces must be passed through verbatim and echoed back.
#[tokio::test]
#[ignore = "requires Docker, openai-mokku-go image, and enhanced process implementation"]
async fn cli_multi_word_message_is_echoed_in_full() {
    // Given: mock + isolated env
    let mock = OpenAiMock::start().await;
    let env = TestEnv::new();
    env.write_openai_config(&mock.base_url);

    // When: message contains spaces
    let out = run_avocode(
        &[
            "run",
            "--no-tui",
            "--message",
            "how are you today",
            "--model",
            "openai/gpt-4o",
        ],
        &env.env_overrides(),
        env.project_path(),
        Duration::from_secs(30),
    )
    .await;

    // Then: the full message is echoed
    assert!(
        out.stdout.contains("Echo: how are you today"),
        "expected full-message echo\nstdout: {}\nstderr: {}",
        out.stdout,
        out.stderr,
    );
    assert!(out.status.success());
}

// ---------------------------------------------------------------------------
// Negative path -- credit-error model
// ---------------------------------------------------------------------------

/// Using the `credit-error` model triggers a quota error from the mock.
/// The CLI must surface the error (non-zero exit and/or error text on
/// stderr/stdout) rather than silently succeeding or hanging.
#[tokio::test]
#[ignore = "requires Docker, openai-mokku-go image, and enhanced process implementation"]
async fn cli_credit_error_model_surfaces_error_and_exits_nonzero() {
    // Given: config pointing at the mock with the credit-error model
    let mock = OpenAiMock::start().await;
    let env = TestEnv::new();
    env.write_credit_error_config(&mock.base_url);

    // When: running with the error-triggering model
    let out = run_avocode(
        &[
            "run",
            "--no-tui",
            "--message",
            "trigger error",
            "--model",
            "openai/credit-error",
        ],
        &env.env_overrides(),
        env.project_path(),
        Duration::from_secs(30),
    )
    .await;

    // Then: at least one of (non-zero exit, error text present) must hold
    let has_error_text = out.stderr.contains("error")
        || out.stderr.contains("Error")
        || out.stdout.contains("error")
        || out.stdout.contains("Error");
    let exited_nonzero = !out.status.success();

    assert!(
        has_error_text || exited_nonzero,
        "expected error indication (non-zero exit or error text), but:\n\
         exit status: {:?}\nstdout: {}\nstderr: {}",
        out.status,
        out.stdout,
        out.stderr,
    );
}
