use std::{future::Future, pin::Pin};

use crate::tool::{
    ToolError,
    schema::{ToolContext, ToolOutput, resolve_path},
};

pub struct ReadTool;

impl crate::tool::Tool for ReadTool {
    fn id(&self) -> &'static str {
        "read"
    }

    fn description(&self) -> &'static str {
        "Read file contents with optional line offset and limit"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string"},
                "offset": {
                    "type": "integer",
                    "description": "Starting line (1-indexed)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max lines to read (default 2000)"
                }
            },
            "required": ["file_path"]
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'a>> {
        Box::pin(async move {
            let path_str = args["file_path"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments("file_path required".into()))?;
            let path = resolve_path(path_str, &ctx.directory);

            let offset =
                usize::try_from(args["offset"].as_u64().unwrap_or(1).max(1)).unwrap_or(usize::MAX);
            let limit = usize::try_from(args["limit"].as_u64().unwrap_or(2000)).unwrap_or(2000);

            if path.is_dir() {
                let mut entries = Vec::new();
                let mut rd = tokio::fs::read_dir(&path).await.map_err(ToolError::Io)?;
                while let Some(entry) = rd.next_entry().await.map_err(ToolError::Io)? {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                    entries.push(if is_dir { format!("{name}/") } else { name });
                }
                entries.sort();
                return Ok(ToolOutput {
                    title: format!("Read {path_str}"),
                    output: entries.join("\n"),
                    metadata: None,
                });
            }

            let content = tokio::fs::read_to_string(&path)
                .await
                .map_err(ToolError::Io)?;
            let lines: Vec<&str> = content.lines().collect();
            let start = (offset - 1).min(lines.len());
            let end = (start + limit).min(lines.len());

            let numbered: String = lines[start..end]
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{:>4}\t{line}", start + i + 1))
                .collect::<Vec<_>>()
                .join("\n");

            Ok(ToolOutput {
                title: format!("Read {path_str}"),
                output: numbered,
                metadata: Some(serde_json::json!({ "total_lines": lines.len() })),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{Tool, ToolContext};

    #[tokio::test]
    async fn read_file_with_line_numbers() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("avocode_test_read.txt");
        tokio::fs::write(&file_path, "line one\nline two\nline three\n")
            .await
            .map_err(|e| panic!("write failed: {e}"))
            .ok();

        let ctx = ToolContext::new(dir);
        let tool = ReadTool;
        let args = serde_json::json!({
            "file_path": file_path.to_string_lossy().as_ref()
        });

        let result = tool.execute(args, &ctx).await;
        match result {
            Ok(output) => {
                assert!(output.output.contains("   1\tline one"));
                assert!(output.output.contains("   2\tline two"));
                assert!(output.output.contains("   3\tline three"));
            }
            Err(e) => panic!("unexpected error: {e}"),
        }

        tokio::fs::remove_file(&file_path).await.ok();
    }
}
