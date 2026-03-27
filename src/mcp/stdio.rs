use std::collections::HashMap;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::mcp::client::McpError;
use crate::mcp::transport::Transport;

/// Transport that communicates with a child process over its stdin/stdout,
/// using newline-delimited JSON.
pub struct StdioTransport {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
}

impl StdioTransport {
    /// Spawn `command` with `args` and `env`, returning a transport wrapping
    /// the child's stdin/stdout.
    ///
    /// # Errors
    ///
    /// Returns an error if the process cannot be spawned or its stdio pipes
    /// cannot be captured.
    pub fn new(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self, McpError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn().map_err(|e| McpError::Spawn(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Spawn("could not capture stdin".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Spawn("could not capture stdout".to_owned()))?;

        Ok(Self {
            child,
            stdin,
            reader: BufReader::new(stdout),
        })
    }
}

#[async_trait::async_trait]
impl Transport for StdioTransport {
    async fn send(&mut self, msg: &str) -> Result<(), McpError> {
        // Write message + newline in one buffer to avoid an extra syscall.
        let mut buf = msg.as_bytes().to_vec();
        buf.push(b'\n');
        self.stdin.write_all(&buf).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<String, McpError> {
        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        Ok(line
            .trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_owned())
    }

    async fn close(&mut self) -> Result<(), McpError> {
        let _ = self.child.kill().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the `StdioTransport` type satisfies `Send`.
    #[test]
    fn test_stdio_transport_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<StdioTransport>();
    }
}
