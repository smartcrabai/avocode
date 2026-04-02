#![expect(dead_code)]
#![expect(clippy::expect_used)]

use std::time::Duration;
use tokio::process::Command;

/// Path to the compiled `avocode` binary, resolved by Cargo at build time.
pub const AVOCODE_BIN: &str = env!("CARGO_BIN_EXE_avocode");

/// Output of a completed `avocode` invocation.
pub struct ProcessOutput {
    pub stdout: String,
    pub stderr: String,
    pub status: std::process::ExitStatus,
}

/// Spawn `avocode` with the given `args` and environment overrides, wait for
/// it to finish (bounded by `timeout`), and return the collected output.
///
/// The child process inherits the parent's environment except for the keys
/// present in `env_overrides`, which take precedence.  Pass at least the
/// `HOME` override from [`super::fs::TestEnv::env_overrides`] to isolate the
/// child from the developer's real state.
pub async fn run_avocode(
    args: &[&str],
    env_overrides: &[(String, String)],
    cwd: &std::path::Path,
    timeout: Duration,
) -> ProcessOutput {
    let mut cmd = Command::new(AVOCODE_BIN);
    cmd.args(args);
    cmd.current_dir(cwd);
    for (k, v) in env_overrides {
        cmd.env(k, v);
    }

    let output = tokio::time::timeout(timeout, cmd.output())
        .await
        .unwrap_or_else(|_| panic!("avocode timed out after {timeout:?}"))
        .expect("failed to execute avocode");

    ProcessOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        status: output.status,
    }
}

/// Spawn `avocode serve --host <host> --port <port>` as a background process.
///
/// Returns the child handle.  The caller must kill and await it when done.
pub fn spawn_avocode_serve(
    host: &str,
    port: u16,
    env_overrides: &[(String, String)],
    cwd: &std::path::Path,
) -> tokio::process::Child {
    let mut cmd = Command::new(AVOCODE_BIN);
    cmd.args(["serve", "--host", host, "--port", &port.to_string()]);
    cmd.current_dir(cwd);
    for (k, v) in env_overrides {
        cmd.env(k, v);
    }
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    cmd.spawn().expect("failed to spawn avocode serve")
}

/// Poll `GET <url>` until it returns HTTP 2xx or the 15-second deadline
/// is reached.
///
/// Panics on timeout.
pub async fn wait_for_server(url: &str) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("failed to build reqwest client");

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "server at {url} did not become ready within 15 seconds"
        );
        match client.get(url).send().await {
            Ok(r) if r.status().is_success() => return,
            _ => tokio::time::sleep(Duration::from_millis(300)).await,
        }
    }
}

/// Allocate a free TCP port on `127.0.0.1` by binding to port 0 and reading
/// the assigned port, then closing the listener before returning.
///
/// There is an inherent TOCTOU race but it is acceptable for tests since
/// ports are recycled only after a delay by most OS kernels.
pub fn free_local_port() -> u16 {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind to ephemeral port");
    listener
        .local_addr()
        .expect("failed to read local addr")
        .port()
}
