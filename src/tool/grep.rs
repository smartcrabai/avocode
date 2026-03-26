use std::{future::Future, pin::Pin};

use crate::tool::{
    ToolError,
    schema::{
        MAX_OUTPUT_BYTES, MAX_OUTPUT_LINES, ToolContext, ToolOutput, resolve_path, truncate_output,
    },
};

pub struct GrepTool;

impl crate::tool::Tool for GrepTool {
    fn id(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> &'static str {
        "Search for a pattern in files using ripgrep or fallback to manual search"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (defaults to working directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.rs')"
                },
                "output_mode": {
                    "type": "string",
                    "description": "Output mode: 'content' (default), 'files_with_matches', or 'count'",
                    "enum": ["content", "files_with_matches", "count"]
                },
                "context": {
                    "type": "integer",
                    "description": "Number of lines of context around matches"
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

            let search_path = args["path"].as_str().map_or_else(
                || ctx.directory.clone(),
                |p| resolve_path(p, &ctx.directory),
            );

            let output_mode = args["output_mode"].as_str().unwrap_or("content");
            let context_lines =
                u32::try_from(args["context"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
            let glob_filter = args["glob"].as_str();

            if let Ok(rg_output) = try_ripgrep(
                pattern,
                &search_path,
                glob_filter,
                output_mode,
                context_lines,
            )
            .await
            {
                return Ok(ToolOutput {
                    title: format!("Grep {pattern}"),
                    output: truncate_output(&rg_output, MAX_OUTPUT_LINES, MAX_OUTPUT_BYTES),
                    metadata: None,
                });
            }

            let result = manual_grep(pattern, &search_path, glob_filter, output_mode).await?;
            Ok(ToolOutput {
                title: format!("Grep {pattern}"),
                output: truncate_output(&result, MAX_OUTPUT_LINES, MAX_OUTPUT_BYTES),
                metadata: None,
            })
        })
    }
}

async fn try_ripgrep(
    pattern: &str,
    path: &std::path::Path,
    glob_filter: Option<&str>,
    output_mode: &str,
    context_lines: u32,
) -> Result<String, ToolError> {
    let mut cmd = tokio::process::Command::new("rg");
    cmd.arg("--color=never");

    match output_mode {
        "files_with_matches" => {
            cmd.arg("-l");
        }
        "count" => {
            cmd.arg("-c");
        }
        _ => {
            cmd.arg("-n");
        }
    }

    if context_lines > 0 {
        cmd.arg(format!("-C{context_lines}"));
    }

    if let Some(g) = glob_filter {
        cmd.arg("-g").arg(g);
    }

    cmd.arg(pattern).arg(path);

    let output = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(ToolError::Io)?;

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn manual_grep(
    pattern: &str,
    search_path: &std::path::Path,
    glob_filter: Option<&str>,
    output_mode: &str,
) -> Result<String, ToolError> {
    let glob_matcher = if let Some(g) = glob_filter {
        let built = globset::GlobBuilder::new(g)
            .literal_separator(false)
            .build()
            .map_err(|e| ToolError::InvalidArguments(format!("invalid glob: {e}")))?;
        Some(
            globset::GlobSet::builder()
                .add(built)
                .build()
                .map_err(|e| ToolError::InvalidArguments(format!("invalid glob set: {e}")))?,
        )
    } else {
        None
    };

    let mut all_files: Vec<std::path::PathBuf> = Vec::new();
    collect_files(search_path, glob_matcher.as_ref(), &mut all_files).await?;

    let mut lines: Vec<String> = Vec::new();

    for file in &all_files {
        let Ok(content) = tokio::fs::read_to_string(file).await else {
            continue; // skip binary/unreadable files
        };

        let file_display = file.to_string_lossy();
        let mut file_matches = 0u64;

        for (line_num, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                file_matches += 1;
                if output_mode == "content" {
                    lines.push(format!("{}:{}:{}", file_display, line_num + 1, line));
                }
            }
        }

        if file_matches > 0 {
            match output_mode {
                "files_with_matches" => lines.push(file_display.into_owned()),
                "count" => lines.push(format!("{file_display}:{file_matches}")),
                _ => {}
            }
        }
    }

    Ok(lines.join("\n"))
}

fn collect_files<'a>(
    dir: &'a std::path::Path,
    glob_matcher: Option<&'a globset::GlobSet>,
    files: &'a mut Vec<std::path::PathBuf>,
) -> Pin<Box<dyn Future<Output = Result<(), ToolError>> + Send + 'a>> {
    Box::pin(async move {
        if dir.is_file() {
            files.push(dir.to_path_buf());
            return Ok(());
        }
        let Ok(mut rd) = tokio::fs::read_dir(dir).await else {
            return Ok(());
        };
        while let Some(entry) = rd.next_entry().await.map_err(ToolError::Io)? {
            let path = entry.path();
            let file_type = entry.file_type().await.map_err(ToolError::Io)?;
            if file_type.is_dir() {
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with('.') {
                    collect_files(&path, glob_matcher, files).await?;
                }
            } else if file_type.is_file() {
                let include = glob_matcher.is_none_or(|gm| gm.is_match(entry.file_name()));
                if include {
                    files.push(path);
                }
            }
        }
        Ok(())
    })
}
