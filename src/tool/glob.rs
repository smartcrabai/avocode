use std::{future::Future, pin::Pin};

use crate::tool::{
    ToolError,
    schema::{ToolContext, ToolOutput, resolve_path},
};

pub struct GlobTool;

impl crate::tool::Tool for GlobTool {
    fn id(&self) -> &'static str {
        "glob"
    }

    fn description(&self) -> &'static str {
        "Find files matching a glob pattern"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match (e.g. '**/*.rs')"
                },
                "path": {
                    "type": "string",
                    "description": "Root directory to search in (defaults to working directory)"
                }
            },
            "required": ["pattern"]
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'a>> {
        Box::pin(async move {
            let pattern = args["pattern"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments("pattern required".into()))?;

            let root = args["path"].as_str().map_or_else(
                || ctx.directory.clone(),
                |p| resolve_path(p, &ctx.directory),
            );

            let glob_matcher = globset::GlobBuilder::new(pattern)
                .literal_separator(false)
                .build()
                .map_err(|e| ToolError::InvalidArguments(format!("invalid glob pattern: {e}")))?;
            let glob_set = globset::GlobSet::builder()
                .add(glob_matcher)
                .build()
                .map_err(|e| ToolError::InvalidArguments(format!("invalid glob: {e}")))?;

            let mut found: Vec<String> = Vec::new();
            walk_dir(&root, &root, &glob_set, &mut found).await?;
            found.sort();

            Ok(ToolOutput {
                title: format!("Glob {pattern}"),
                output: found.join("\n"),
                metadata: Some(serde_json::json!({ "count": found.len() })),
            })
        })
    }
}

fn walk_dir<'a>(
    root: &'a std::path::Path,
    dir: &'a std::path::Path,
    glob_set: &'a globset::GlobSet,
    found: &'a mut Vec<String>,
) -> Pin<Box<dyn Future<Output = Result<(), ToolError>> + Send + 'a>> {
    Box::pin(async move {
        let mut rd = tokio::fs::read_dir(dir).await.map_err(ToolError::Io)?;
        while let Some(entry) = rd.next_entry().await.map_err(ToolError::Io)? {
            let entry_path = entry.path();
            let relative = entry_path
                .strip_prefix(root)
                .map_err(|e| ToolError::Execution(format!("path strip error: {e}")))?;

            if glob_set.is_match(relative) {
                found.push(entry_path.to_string_lossy().into_owned());
            }

            let file_type = entry.file_type().await.map_err(ToolError::Io)?;
            if file_type.is_dir() {
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with('.') {
                    walk_dir(root, &entry_path, glob_set, found).await?;
                }
            }
        }
        Ok(())
    })
}
