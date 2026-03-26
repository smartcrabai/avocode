use futures_util::StreamExt as _;

use crate::llm::{
    ChatMessage, ContentPart, FinishReason, LlmError, MessageRole, StreamDelta, StreamOptions,
    ToolCallDelta, ToolDefinition,
    sse::{SseEvent, sse_stream},
};

pub const GOOGLE_API_BASE: &str = "https://generativelanguage.googleapis.com";

pub struct GoogleClient {
    client: reqwest::Client,
}

impl GoogleClient {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Stream content generation from the Google Generative AI API.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the API returns an error status.
    pub async fn stream(
        &self,
        options: &StreamOptions,
    ) -> Result<impl futures_util::Stream<Item = Result<StreamDelta, LlmError>>, LlmError> {
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            options.base_url.trim_end_matches('/'),
            options.model,
            options.api_key,
        );

        let contents = to_google_contents(&options.messages);

        let mut body = serde_json::json!({
            "contents": contents,
        });

        if let Some(system) = &options.system {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": system}]
            });
        }

        if !options.tools.is_empty() {
            body["tools"] = to_google_tools(&options.tools);
        }

        let mut generation_config = serde_json::json!({});
        if let Some(temp) = options.temperature {
            generation_config["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = options.top_p {
            generation_config["topP"] = serde_json::json!(top_p);
        }
        if let Some(max_tokens) = options.max_tokens {
            generation_config["maxOutputTokens"] = serde_json::json!(max_tokens);
        }
        if generation_config.as_object().is_some_and(|m| !m.is_empty()) {
            body["generationConfig"] = generation_config;
        }

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);

        for (k, v) in &options.extra_headers {
            req = req.header(k, v);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status,
                message: msg,
            });
        }

        Ok(sse_stream(resp).filter_map(|event| {
            futures_util::future::ready(match event {
                Ok(e) => parse_google_event(&e).transpose(),
                Err(e) => Some(Err(e)),
            })
        }))
    }
}

impl Default for GoogleClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert internal `ChatMessage` list to Google Generative AI API format.
#[must_use]
pub fn to_google_contents(messages: &[ChatMessage]) -> serde_json::Value {
    let mut result = Vec::new();

    for msg in messages {
        let role = match msg.role {
            MessageRole::System => continue,
            MessageRole::User | MessageRole::Tool => "user",
            MessageRole::Assistant => "model",
        };

        let mut parts: Vec<serde_json::Value> = Vec::new();

        for part in &msg.content {
            match part {
                ContentPart::Text { text } => {
                    parts.push(serde_json::json!({"text": text}));
                }
                ContentPart::Image { url, media_type } => {
                    parts.push(serde_json::json!({
                        "inlineData": {
                            "mimeType": media_type,
                            "data": url,
                        }
                    }));
                }
                ContentPart::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    let args: serde_json::Value =
                        serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
                    parts.push(serde_json::json!({
                        "functionCall": {
                            "name": name,
                            "args": args,
                        }
                    }));
                    let _ = id;
                }
                ContentPart::ToolResult {
                    tool_call_id,
                    content,
                } => {
                    parts.push(serde_json::json!({
                        "functionResponse": {
                            "name": tool_call_id,
                            "response": {
                                "content": content,
                            }
                        }
                    }));
                }
            }
        }

        if !parts.is_empty() {
            result.push(serde_json::json!({"role": role, "parts": parts}));
        }
    }

    serde_json::json!(result)
}

fn to_google_tools(tools: &[ToolDefinition]) -> serde_json::Value {
    let function_declarations: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        })
        .collect();
    serde_json::json!([{"functionDeclarations": function_declarations}])
}

/// Parse a Google Generative AI SSE event into a `StreamDelta`.
///
/// # Errors
///
/// Returns an error if the event data contains invalid JSON.
pub fn parse_google_event(event: &SseEvent) -> Result<Option<StreamDelta>, LlmError> {
    if event.data.is_empty() || event.data == "[DONE]" {
        return Ok(None);
    }

    let v: serde_json::Value = serde_json::from_str(&event.data)?;

    let candidates = match v.get("candidates").and_then(|c| c.as_array()) {
        Some(c) if !c.is_empty() => c,
        _ => return Ok(None),
    };

    let candidate = &candidates[0];
    let mut result = StreamDelta::default();
    let mut has_content = false;

    // Finish reason
    if let Some(finish) = candidate.get("finishReason").and_then(|f| f.as_str()) {
        result.finish_reason = match finish {
            "STOP" => Some(FinishReason::Stop),
            "MAX_TOKENS" => Some(FinishReason::Length),
            "SAFETY" => Some(FinishReason::ContentFilter),
            _ => None,
        };
        if result.finish_reason.is_some() {
            has_content = true;
        }
    }

    let Some(content) = candidate.get("content") else {
        return if has_content {
            Ok(Some(result))
        } else {
            Ok(None)
        };
    };

    let Some(parts) = content.get("parts").and_then(|p| p.as_array()) else {
        return if has_content {
            Ok(Some(result))
        } else {
            Ok(None)
        };
    };

    for (index, part) in parts.iter().enumerate() {
        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
            if !text.is_empty() {
                let existing = result.text.get_or_insert_with(String::new);
                existing.push_str(text);
                has_content = true;
            }
        } else if let Some(func_call) = part.get("functionCall") {
            let name = func_call
                .get("name")
                .and_then(|n| n.as_str())
                .map(str::to_owned);
            let args = func_call.get("args").map(std::string::ToString::to_string);
            result.tool_calls.push(ToolCallDelta {
                index,
                id: None,
                name,
                arguments_chunk: args,
            });
            has_content = true;
        }
    }

    if has_content {
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ContentPart, MessageRole, sse::SseEvent};

    #[test]
    fn test_to_google_contents_user_message() {
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "Hello".to_owned(),
            }],
        }];
        let result = to_google_contents(&messages);
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["role"], "user");
        assert_eq!(arr[0]["parts"][0]["text"], "Hello");
    }

    #[test]
    fn test_to_google_contents_filters_system() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: vec![ContentPart::Text {
                    text: "System prompt".to_owned(),
                }],
            },
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentPart::Text {
                    text: "Hi".to_owned(),
                }],
            },
        ];
        let result = to_google_contents(&messages);
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
    }

    #[test]
    fn test_parse_google_event_text() {
        let event = SseEvent {
            event: None,
            data: r#"{"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"},"finishReason":"STOP"}]}"#.to_owned(),
        };
        let delta = parse_google_event(&event).unwrap().unwrap();
        assert_eq!(delta.text.as_deref(), Some("Hello"));
        assert_eq!(delta.finish_reason, Some(FinishReason::Stop));
    }

    #[test]
    fn test_parse_google_event_empty_data() {
        let event = SseEvent {
            event: None,
            data: String::new(),
        };
        let delta = parse_google_event(&event).unwrap();
        assert!(delta.is_none());
    }
}
