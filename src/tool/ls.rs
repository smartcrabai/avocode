use std::{future::Future, pin::Pin};

use crate::tool::{
    ToolError,
    schema::{ToolContext, ToolOutput, resolve_path},
};

pub struct LsTool;

impl crate::tool::Tool for LsTool {
    fn id(&self) -> &'static str {
        "ls"
    }

    fn description(&self) -> &'static str {
        "List directory contents"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to list (defaults to working directory)"
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'a>> {
        Box::pin(async move {
            let dir_path = args["path"].as_str().map_or_else(
                || ctx.directory.clone(),
                |p| resolve_path(p, &ctx.directory),
            );

            let path_display = dir_path.to_string_lossy().into_owned();

            let mut entries = Vec::new();
            let mut rd = tokio::fs::read_dir(&dir_path)
                .await
                .map_err(ToolError::Io)?;
            while let Some(entry) = rd.next_entry().await.map_err(ToolError::Io)? {
                let name = entry.file_name().to_string_lossy().into_owned();
                let file_type = entry.file_type().await.map_err(ToolError::Io)?;
                let metadata = entry.metadata().await.map_err(ToolError::Io)?;
                let size = metadata.len();

                let type_char = if file_type.is_dir() {
                    'd'
                } else if file_type.is_symlink() {
                    'l'
                } else {
                    '-'
                };

                let display_name = if file_type.is_dir() {
                    format!("{name}/")
                } else {
                    name
                };

                entries.push((display_name, type_char, size));
            }

            entries.sort_by(|a, b| a.0.cmp(&b.0));

            let lines: Vec<String> = entries
                .iter()
                .map(|(name, type_char, size)| format!("{type_char} {size:>10} {name}"))
                .collect();

            Ok(ToolOutput {
                title: format!("ls {path_display}"),
                output: lines.join("\n"),
                metadata: Some(serde_json::json!({ "count": entries.len() })),
            })
        })
    }
}
