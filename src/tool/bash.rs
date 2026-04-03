use std::{future::Future, pin::Pin};

use crate::tool::{
    ToolError,
    schema::{MAX_OUTPUT_BYTES, MAX_OUTPUT_LINES, ToolContext, ToolOutput, truncate_output},
};

pub struct BashTool;

impl crate::tool::Tool for BashTool {
    fn id(&self) -> &'static str {
        "bash"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command and return its output"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 30, max 300)"
                }
            },
            "required": ["command"]
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'a>> {
        Box::pin(async move {
            let command = args["command"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments("command required".into()))?;
            let timeout_secs = args["timeout"].as_u64().unwrap_or(30).min(300);

            let title = if command.chars().count() > 80 {
                format!("{}...", command.chars().take(79).collect::<String>())
            } else {
                command.to_owned()
            };

            let mut cmd = tokio::process::Command::new("sh");
            cmd.arg("-c")
                .arg(command)
                .current_dir(&ctx.directory)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let child = cmd.spawn().map_err(ToolError::Io)?;

            let output = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                child.wait_with_output(),
            )
            .await
            .map_err(|_| ToolError::Execution("Command timed out".into()))?
            .map_err(ToolError::Io)?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let combined = if stderr.is_empty() {
                stdout.into_owned()
            } else if stdout.is_empty() {
                stderr.into_owned()
            } else {
                format!("{stdout}\n---stderr---\n{stderr}")
            };

            Ok(ToolOutput {
                title,
                output: truncate_output(&combined, MAX_OUTPUT_LINES, MAX_OUTPUT_BYTES),
                metadata: Some(serde_json::json!({ "exit_code": output.status.code() })),
            })
        })
    }
}
