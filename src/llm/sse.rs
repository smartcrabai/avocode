use futures_util::{StreamExt as _, TryStreamExt as _};

use crate::llm::LlmError;

#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Parse raw bytes into SSE events.
/// SSE format: lines of "field: value", events separated by blank lines.
#[must_use]
pub fn parse_sse_chunk(chunk: &[u8]) -> Vec<SseEvent> {
    let Ok(text) = std::str::from_utf8(chunk) else {
        return Vec::new();
    };

    let mut events = Vec::new();

    for raw_event in text.split("\n\n") {
        let trimmed = raw_event.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut event_type: Option<String> = None;
        let mut data_lines: Vec<&str> = Vec::new();

        for line in trimmed.lines() {
            if let Some(val) = line.strip_prefix("event:") {
                event_type = Some(val.trim().to_owned());
            } else if let Some(val) = line.strip_prefix("data:") {
                data_lines.push(val.trim());
            } else if line == "data" {
                data_lines.push("");
            }
        }

        if !data_lines.is_empty() {
            events.push(SseEvent {
                event: event_type,
                data: data_lines.join("\n"),
            });
        }
    }

    events
}

/// Build an SSE stream from a streaming HTTP response.
/// Yields parsed `SseEvent`s from the response body.
pub fn sse_stream(
    response: reqwest::Response,
) -> impl futures_util::Stream<Item = Result<SseEvent, LlmError>> {
    let byte_stream = response.bytes_stream().map_err(LlmError::Http);

    byte_stream.flat_map(|chunk_result| {
        let events = match chunk_result {
            Ok(bytes) => parse_sse_chunk(&bytes),
            Err(e) => {
                return futures_util::stream::once(async move { Err(e) }).left_stream();
            }
        };
        futures_util::stream::iter(events.into_iter().map(Ok)).right_stream()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sse_chunk_simple_data() {
        let events = parse_sse_chunk(b"data: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
        assert!(events[0].event.is_none());
    }

    #[test]
    fn test_parse_sse_chunk_with_event_type() {
        let events = parse_sse_chunk(b"event: foo\ndata: bar\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("foo"));
        assert_eq!(events[0].data, "bar");
    }

    #[test]
    fn test_parse_sse_chunk_multiple_events() {
        let events = parse_sse_chunk(b"data: first\n\ndata: second\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "first");
        assert_eq!(events[1].data, "second");
    }

    #[test]
    fn test_parse_sse_chunk_empty() {
        let events = parse_sse_chunk(b"\n\n");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_parse_sse_chunk_multiline_data() {
        let events = parse_sse_chunk(b"data: line1\ndata: line2\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2");
    }
}
