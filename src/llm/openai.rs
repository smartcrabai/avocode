use futures_util::StreamExt as _;

use crate::llm::{
    ChatMessage, ContentPart, FinishReason, LlmError, MessageRole, StreamDelta, StreamOptions,
    ToolCallDelta, json_index, sse::sse_stream,
};

pub const OPENAI_API_BASE: &str = "https://api.openai.com";

pub struct OpenAiClient {
    client: reqwest::Client,
}

impl OpenAiClient {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Stream chat completions from the `OpenAI` API.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the API returns an error status.
    pub async fn stream(
        &self,
        options: &StreamOptions,
    ) -> Result<impl futures_util::Stream<Item = Result<StreamDelta, LlmError>>, LlmError> {
        let url = format!(
            "{}/v1/chat/completions",
            options.base_url.trim_end_matches('/')
        );

        let body = build_openai_request(options);

        let mut req = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", options.api_key))
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
                Ok(e) if e.data == "[DONE]" => None,
                Ok(e) => parse_openai_delta(&e.data).transpose(),
                Err(e) => Some(Err(e)),
            })
        }))
    }
}

impl Default for OpenAiClient {
    fn default() -> Self {
        Self::new()
    }
}

fn build_openai_request(options: &StreamOptions) -> serde_json::Value {
    let messages = to_openai_messages(&options.messages);

    let mut body = serde_json::json!({
        "model": options.model,
        "messages": messages,
        "stream": true,
    });

    if let Some(temp) = options.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(top_p) = options.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(max_tokens) = options.max_tokens {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }

    if !options.tools.is_empty() {
        let tools: Vec<serde_json::Value> = options
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        body["tools"] = serde_json::json!(tools);
    }

    if let Some(system) = &options.system
        && let Some(arr) = body["messages"].as_array_mut()
    {
        arr.insert(
            0,
            serde_json::json!({
                "role": "system",
                "content": system,
            }),
        );
    }

    body
}

fn parse_openai_delta(data: &str) -> Result<Option<StreamDelta>, LlmError> {
    let v: serde_json::Value = serde_json::from_str(data)?;

    let choices = match v.get("choices").and_then(|c| c.as_array()) {
        Some(c) if !c.is_empty() => c,
        _ => return Ok(None),
    };

    let choice = &choices[0];
    let Some(delta) = choice.get("delta") else {
        return Ok(None);
    };

    let mut result = StreamDelta::default();
    let mut has_content = false;

    if let Some(content) = delta.get("content").and_then(|c| c.as_str())
        && !content.is_empty()
    {
        result.text = Some(content.to_owned());
        has_content = true;
    }

    // reasoning_content is a non-standard extension used by o1/o3 models
    if let Some(reasoning) = delta.get("reasoning_content").and_then(|c| c.as_str())
        && !reasoning.is_empty()
    {
        result.reasoning = Some(reasoning.to_owned());
        has_content = true;
    }

    if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tool_calls {
            let index = json_index(tc);

            let id = tc.get("id").and_then(|i| i.as_str()).map(str::to_owned);
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .map(str::to_owned);
            let arguments_chunk = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .map(str::to_owned);

            if id.is_some() || name.is_some() || arguments_chunk.is_some() {
                result.tool_calls.push(ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments_chunk,
                });
                has_content = true;
            }
        }
    }

    if let Some(finish) = choice.get("finish_reason").and_then(|f| f.as_str()) {
        result.finish_reason = match finish {
            "stop" => Some(FinishReason::Stop),
            "length" => Some(FinishReason::Length),
            "tool_calls" => Some(FinishReason::ToolCalls),
            "content_filter" => Some(FinishReason::ContentFilter),
            _ => None,
        };
        if result.finish_reason.is_some() {
            has_content = true;
        }
    }

    if has_content {
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

#[must_use]
pub fn to_openai_messages(messages: &[ChatMessage]) -> serde_json::Value {
    let mut result = Vec::new();

    for msg in messages {
        let role = match msg.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };

        let mut text_parts: Vec<&str> = Vec::new();
        let mut tool_calls: Vec<serde_json::Value> = Vec::new();
        let mut tool_results: Vec<(&str, &str)> = Vec::new();

        for part in &msg.content {
            match part {
                ContentPart::Text { text } => text_parts.push(text.as_str()),
                ContentPart::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    tool_calls.push(serde_json::json!({
                        "id": id,
                        "type": "function",
                        "function": { "name": name, "arguments": arguments }
                    }));
                }
                ContentPart::ToolResult {
                    tool_call_id,
                    content,
                } => {
                    tool_results.push((tool_call_id.as_str(), content.as_str()));
                }
                ContentPart::Image { .. } => {}
            }
        }

        if !tool_results.is_empty() {
            for (tool_call_id, content) in &tool_results {
                result.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": content,
                }));
            }
        } else if !tool_calls.is_empty() {
            let mut msg_obj = serde_json::json!({
                "role": role,
                "tool_calls": tool_calls,
            });
            if !text_parts.is_empty() {
                msg_obj["content"] = serde_json::json!(text_parts.join(""));
            }
            result.push(msg_obj);
        } else {
            let content = text_parts.join("");
            result.push(serde_json::json!({
                "role": role,
                "content": content,
            }));
        }
    }

    serde_json::json!(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::messages::MessageRole;

    #[test]
    fn test_parse_openai_delta_text() {
        let data = r#"{"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let delta = parse_openai_delta(data).unwrap().unwrap();
        assert_eq!(delta.text.as_deref(), Some("Hello"));
        assert!(delta.tool_calls.is_empty());
        assert!(delta.finish_reason.is_none());
    }

    #[test]
    fn test_parse_openai_delta_tool_call() {
        let data = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","function":{"name":"my_tool","arguments":"{\"key\":"}}]},"finish_reason":null}]}"#;
        let delta = parse_openai_delta(data).unwrap().unwrap();
        assert_eq!(delta.tool_calls.len(), 1);
        assert_eq!(delta.tool_calls[0].id.as_deref(), Some("call_abc"));
        assert_eq!(delta.tool_calls[0].name.as_deref(), Some("my_tool"));
    }

    #[test]
    fn test_parse_openai_delta_finish_reason() {
        let data = r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        let delta = parse_openai_delta(data).unwrap().unwrap();
        assert_eq!(delta.finish_reason, Some(FinishReason::Stop));
    }

    #[test]
    fn test_parse_openai_delta_empty() {
        let data = r#"{"choices":[{"delta":{},"finish_reason":null}]}"#;
        let delta = parse_openai_delta(data).unwrap();
        assert!(delta.is_none());
    }

    #[test]
    fn test_to_openai_messages_user_text() {
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentPart::Text {
                text: "Hello".to_owned(),
            }],
        }];
        let result = to_openai_messages(&messages);
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["role"], "user");
        assert_eq!(arr[0]["content"], "Hello");
    }

    #[test]
    fn test_to_openai_messages_tool_result() {
        let messages = vec![ChatMessage {
            role: MessageRole::Tool,
            content: vec![ContentPart::ToolResult {
                tool_call_id: "call_123".to_owned(),
                content: "result".to_owned(),
            }],
        }];
        let result = to_openai_messages(&messages);
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["role"], "tool");
        assert_eq!(arr[0]["tool_call_id"], "call_123");
        assert_eq!(arr[0]["content"], "result");
    }
}
