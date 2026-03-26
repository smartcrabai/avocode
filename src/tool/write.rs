use std::{future::Future, pin::Pin};

use crate::tool::{
    ToolError,
    schema::{ToolContext, ToolOutput, resolve_path},
};

pub struct WriteTool;

impl crate::tool::Tool for WriteTool {
    fn id(&self) -> &'static str {
        "write"
    }

    fn description(&self) -> &'static str {
        "Write content to a file, creating it and any parent directories"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["file_path", "content"]
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
            let content = args["content"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments("content required".into()))?;
            let path = resolve_path(path_str, &ctx.directory);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(ToolError::Io)?;
            }
            tokio::fs::write(&path, content)
                .await
                .map_err(ToolError::Io)?;
            Ok(ToolOutput {
                title: format!("Write {path_str}"),
                output: format!("Successfully wrote {} bytes to {path_str}", content.len()),
                metadata: None,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{Tool, ToolContext};

    #[tokio::test]
    async fn write_creates_file() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("avocode_test_write.txt");
        tokio::fs::remove_file(&file_path).await.ok();

        let ctx = ToolContext::new(dir);
        let tool = WriteTool;
        let args = serde_json::json!({
            "file_path": file_path.to_string_lossy().as_ref(),
            "content": "hello world"
        });

        let result = tool.execute(args, &ctx).await;
        match result {
            Ok(output) => {
                assert!(output.output.contains("Successfully wrote"));
                let written = tokio::fs::read_to_string(&file_path).await;
                match written {
                    Ok(content) => assert_eq!(content, "hello world"),
                    Err(e) => panic!("read back failed: {e}"),
                }
            }
            Err(e) => panic!("unexpected error: {e}"),
        }

        tokio::fs::remove_file(&file_path).await.ok();
    }
}
