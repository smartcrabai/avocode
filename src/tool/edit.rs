use std::{future::Future, pin::Pin};

use crate::tool::{
    ToolError,
    schema::{ToolContext, ToolOutput, resolve_path},
};

pub struct EditTool;

impl crate::tool::Tool for EditTool {
    fn id(&self) -> &'static str {
        "edit"
    }

    fn description(&self) -> &'static str {
        "Edit a file by replacing old_string with new_string"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string"},
                "old_string": {"type": "string"},
                "new_string": {"type": "string"},
                "replace_all": {"type": "boolean", "default": false}
            },
            "required": ["file_path", "old_string", "new_string"]
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
            let old = args["old_string"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments("old_string required".into()))?;
            let new = args["new_string"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments("new_string required".into()))?;
            let replace_all = args["replace_all"].as_bool().unwrap_or(false);

            let path = resolve_path(path_str, &ctx.directory);
            let content = tokio::fs::read_to_string(&path)
                .await
                .map_err(ToolError::Io)?;

            let count = content.matches(old).count();
            if count == 0 {
                return Err(ToolError::Execution(format!(
                    "old_string not found in {path_str}"
                )));
            }
            if count > 1 && !replace_all {
                return Err(ToolError::Execution(format!(
                    "old_string appears {count} times in {path_str}. Use replace_all=true to replace all occurrences."
                )));
            }

            let new_content = if replace_all {
                content.replace(old, new)
            } else {
                content.replacen(old, new, 1)
            };

            tokio::fs::write(&path, &new_content)
                .await
                .map_err(ToolError::Io)?;

            Ok(ToolOutput {
                title: format!("Edit {path_str}"),
                output: format!("Successfully edited {path_str} ({count} replacement(s))"),
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
    async fn edit_replaces_content() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("avocode_test_edit.txt");
        tokio::fs::write(&file_path, "hello world")
            .await
            .map_err(|e| panic!("setup failed: {e}"))
            .ok();

        let ctx = ToolContext::new(dir);
        let tool = EditTool;
        let args = serde_json::json!({
            "file_path": file_path.to_string_lossy().as_ref(),
            "old_string": "hello",
            "new_string": "goodbye"
        });

        let result = tool.execute(args, &ctx).await;
        match result {
            Ok(output) => {
                assert!(output.output.contains("Successfully edited"));
                let content = tokio::fs::read_to_string(&file_path).await;
                match content {
                    Ok(c) => assert_eq!(c, "goodbye world"),
                    Err(e) => panic!("read back failed: {e}"),
                }
            }
            Err(e) => panic!("unexpected error: {e}"),
        }

        tokio::fs::remove_file(&file_path).await.ok();
    }

    #[tokio::test]
    async fn edit_returns_error_when_not_found() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("avocode_test_edit_notfound.txt");
        tokio::fs::write(&file_path, "hello world")
            .await
            .map_err(|e| panic!("setup failed: {e}"))
            .ok();

        let ctx = ToolContext::new(dir);
        let tool = EditTool;
        let args = serde_json::json!({
            "file_path": file_path.to_string_lossy().as_ref(),
            "old_string": "nonexistent",
            "new_string": "replacement"
        });

        let result = tool.execute(args, &ctx).await;
        match result {
            Err(ToolError::Execution(msg)) => {
                assert!(msg.contains("not found"));
            }
            Ok(_) => panic!("expected error but got Ok"),
            Err(e) => panic!("unexpected error type: {e}"),
        }

        tokio::fs::remove_file(&file_path).await.ok();
    }
}
