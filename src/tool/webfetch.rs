use std::{future::Future, pin::Pin, sync::OnceLock};

use crate::tool::{
    ToolError,
    schema::{MAX_OUTPUT_LINES, ToolContext, ToolOutput, truncate_output},
};

pub struct WebFetchTool;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("avocode/0.1")
            .build()
            .unwrap_or_default()
    })
}

impl crate::tool::Tool for WebFetchTool {
    fn id(&self) -> &'static str {
        "webfetch"
    }

    fn description(&self) -> &'static str {
        "Fetch the text content of a URL"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum number of bytes to return (default 65536)"
                }
            },
            "required": ["url"]
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        _ctx: &'a ToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'a>> {
        Box::pin(async move {
            let url = args["url"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments("url required".into()))?;
            let max_length =
                usize::try_from(args["max_length"].as_u64().unwrap_or(65_536)).unwrap_or(65_536);

            let response = http_client()
                .get(url)
                .send()
                .await
                .map_err(ToolError::Http)?;
            let status = response.status();
            let text = response.text().await.map_err(ToolError::Http)?;

            Ok(ToolOutput {
                title: format!("Fetch {url}"),
                output: truncate_output(&text, MAX_OUTPUT_LINES, max_length),
                metadata: Some(serde_json::json!({ "status": status.as_u16() })),
            })
        })
    }
}
