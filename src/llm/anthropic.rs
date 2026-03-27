use futures_util::StreamExt as _;

use crate::llm::{
    ChatMessage, ContentPart, FinishReason, LlmError, MessageRole, StreamDelta, StreamOptions,
    ToolCallDelta, ToolDefinition, json_index,
    sse::{SseEvent, sse_stream},
};

pub const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com";

pub struct AnthropicClient {
    client: reqwest::Client,
}

impl AnthropicClient {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Stream messages from the Anthropic API.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the API returns an error status.
    pub async fn stream(
        &self,
        options: &StreamOptions,
    ) -> Result<impl futures_util::Stream<Item = Result<StreamDelta, LlmError>>, LlmError> {
        let url = format!("{}/v1/messages", options.base_url.trim_end_matches('/'));

        let messages = to_anthropic_messages(&options.messages);
        let max_tokens = options.max_tokens.unwrap_or(4096);

        let mut body = serde_json::json!({
            "model": options.model,
            "max_tokens": max_tokens,
            "messages": messages,
            "stream": true,
        });

        if let Some(system) = &options.system {
            body["system"] = serde_json::json!(system);
        }

        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = options.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }

        if !options.tools.is_empty() {
            body["tools"] = to_anthropic_tools(&options.tools);
        }

        let mut req = self
            .client
            .post(&url)
            .header("x-api-key", &options.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "interleaved-thinking-2025-05-14")
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
                Ok(e) => parse_anthropic_event(&e).transpose(),
                Err(e) => Some(Err(e)),
            })
        }))
    }
}

impl Default for AnthropicClient {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_content_block_start(data: &str) -> Result<Option<StreamDelta>, LlmError> {
    let v: serde_json::Value = serde_json::from_str(data)?;
    let Some(block) = v.get("content_block") else {
        return Ok(None);
    };
    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if block_type == "tool_use" {
        let index = json_index(&v);
        let id = block.get("id").and_then(|i| i.as_str()).map(str::to_owned);
        let name = block
            .get("name")
            .and_then(|n| n.as_str())
            .map(str::to_owned);
        let mut delta = StreamDelta::default();
        delta.tool_calls.push(ToolCallDelta {
            index,
            id,
            name,
            arguments_chunk: None,
        });
        return Ok(Some(delta));
    }
    Ok(None)
}

fn parse_content_block_delta(data: &str) -> Result<Option<StreamDelta>, LlmError> {
    let v: serde_json::Value = serde_json::from_str(data)?;
    let Some(delta_obj) = v.get("delta") else {
        return Ok(None);
    };
    let delta_type = delta_obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let index = json_index(&v);

    match delta_type {
        "text_delta" => {
            let text = delta_obj.get("text").and_then(|t| t.as_str()).unwrap_or("");
            if text.is_empty() {
                return Ok(None);
            }
            Ok(Some(StreamDelta {
                text: Some(text.to_owned()),
                ..Default::default()
            }))
        }
        "thinking_delta" => {
            let thinking = delta_obj
                .get("thinking")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            if thinking.is_empty() {
                return Ok(None);
            }
            Ok(Some(StreamDelta {
                reasoning: Some(thinking.to_owned()),
                ..Default::default()
            }))
        }
        "input_json_delta" => {
            let partial = delta_obj
                .get("partial_json")
                .and_then(|j| j.as_str())
                .unwrap_or("");
            if partial.is_empty() {
                return Ok(None);
            }
            let mut result = StreamDelta::default();
            result.tool_calls.push(ToolCallDelta {
                index,
                id: None,
                name: None,
                arguments_chunk: Some(partial.to_owned()),
            });
            Ok(Some(result))
        }
        _ => Ok(None),
    }
}

fn parse_message_delta(data: &str) -> Result<Option<StreamDelta>, LlmError> {
    let v: serde_json::Value = serde_json::from_str(data)?;
    let Some(delta_obj) = v.get("delta") else {
        return Ok(None);
    };
    let stop_reason = delta_obj
        .get("stop_reason")
        .and_then(|r| r.as_str())
        .unwrap_or("");
    let finish_reason = match stop_reason {
        "end_turn" => Some(FinishReason::Stop),
        "max_tokens" => Some(FinishReason::Length),
        "tool_use" => Some(FinishReason::ToolCalls),
        _ => None,
    };
    if let Some(reason) = finish_reason {
        return Ok(Some(StreamDelta {
            finish_reason: Some(reason),
            ..Default::default()
        }));
    }
    Ok(None)
}

/// Parse an Anthropic SSE event into a `StreamDelta`.
///
/// # Errors
///
/// Returns an error if the event data contains invalid JSON.
pub fn parse_anthropic_event(event: &SseEvent) -> Result<Option<StreamDelta>, LlmError> {
    let event_type = match &event.event {
        Some(t) => t.as_str(),
        None => return Ok(None),
    };

    match event_type {
        "content_block_start" => parse_content_block_start(&event.data),
        "content_block_delta" => parse_content_block_delta(&event.data),
        "message_delta" => parse_message_delta(&event.data),
        _ => Ok(None),
    }
}

/// Convert internal `ChatMessage` list to Anthropic API format.
/// System messages are filtered out (they must be passed separately).
#[must_use]
pub fn to_anthropic_messages(messages: &[ChatMessage]) -> serde_json::Value {
    let mut result = Vec::new();

    for msg in messages {
        let role = match msg.role {
            MessageRole::System => continue,
            MessageRole::User | MessageRole::Tool => "user",
            MessageRole::Assistant => "assistant",
        };

        let mut parts: Vec<serde_json::Value> = Vec::new();

        for part in &msg.content {
            match part {
                ContentPart::Text { text } => {
                    parts.push(serde_json::json!({"type": "text", "text": text}));
                }
                ContentPart::Image { url, media_type } => {
                    parts.push(serde_json::json!({
                        "type": "image",
                        "source": {
                            "type": "url",
                            "url": url,
                            "media_type": media_type,
                        }
                    }));
                }
                ContentPart::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    let input: serde_json::Value =
                        serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
                    parts.push(serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input,
                    }));
                }
                ContentPart::ToolResult {
                    tool_call_id,
                    content,
                } => {
                    parts.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": content,
                    }));
                }
            }
        }

        if !parts.is_empty() {
            result.push(serde_json::json!({"role": role, "content": parts}));
        }
    }

    serde_json::json!(result)
}

/// Convert tool definitions to Anthropic API format.
#[must_use]
pub fn to_anthropic_tools(tools: &[ToolDefinition]) -> serde_json::Value {
    let converted: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters,
            })
        })
        .collect();
    serde_json::json!(converted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ContentPart, MessageRole, sse::SseEvent};

    #[test]
    fn test_parse_anthropic_event_text_delta() -> Result<(), Box<dyn std::error::Error>> {
        let event = SseEvent {
            event: Some("content_block_delta".to_owned()),
            data: r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#.to_owned(),
        };
        let delta = parse_anthropic_event(&event)?.ok_or("expected delta")?;
        assert_eq!(delta.text.as_deref(), Some("Hello"));
        assert!(delta.tool_calls.is_empty());
        Ok(())
    }

    #[test]
    fn test_parse_anthropic_event_tool_call_start() -> Result<(), Box<dyn std::error::Error>> {
        let event = SseEvent {
            event: Some("content_block_start".to_owned()),
            data: r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_01","name":"my_tool"}}"#.to_owned(),
        };
        let delta = parse_anthropic_event(&event)?.ok_or("expected delta")?;
        assert_eq!(delta.tool_calls.len(), 1);
        assert_eq!(delta.tool_calls[0].name.as_deref(), Some("my_tool"));
        Ok(())
    }

    #[test]
    fn test_parse_anthropic_event_input_json_delta() -> Result<(), Box<dyn std::error::Error>> {
        let event = SseEvent {
            event: Some("content_block_delta".to_owned()),
            data: r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"key\":"}}"#.to_owned(),
        };
        let delta = parse_anthropic_event(&event)?.ok_or("expected delta")?;
        assert_eq!(delta.tool_calls.len(), 1);
        assert_eq!(
            delta.tool_calls[0].arguments_chunk.as_deref(),
            Some("{\"key\":")
        );
        Ok(())
    }

    #[test]
    fn test_to_anthropic_messages_filters_system() -> Result<(), Box<dyn std::error::Error>> {
        let messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: vec![ContentPart::Text {
                    text: "You are helpful".to_owned(),
                }],
            },
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentPart::Text {
                    text: "Hello".to_owned(),
                }],
            },
        ];
        let result = to_anthropic_messages(&messages);
        let arr = result.as_array().ok_or("expected array")?;
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["role"], "user");
        Ok(())
    }

    #[test]
    fn test_parse_anthropic_event_message_delta_stop() -> Result<(), Box<dyn std::error::Error>> {
        let event = SseEvent {
            event: Some("message_delta".to_owned()),
            data: r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":10}}"#.to_owned(),
        };
        let delta = parse_anthropic_event(&event)?.ok_or("expected delta")?;
        assert_eq!(delta.finish_reason, Some(FinishReason::Stop));
        Ok(())
    }
}
