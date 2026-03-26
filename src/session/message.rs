/// The role of a message participant.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

/// A single conversation message containing one or more [`Part`]s.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: MessageRole,
    pub parts: Vec<Part>,
    /// Unix timestamp in milliseconds.
    pub time_created: i64,
    /// Unix timestamp in milliseconds.
    pub time_updated: i64,
}

/// A discriminated-union part that can appear inside a [`Message`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text(TextPart),
    Reasoning(ReasoningPart),
    Tool(ToolPart),
    File(FilePart),
    Compaction(CompactionPart),
    StepStart(StepStartPart),
    StepFinish(StepFinishPart),
}

/// A plain-text content part.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextPart {
    pub id: String,
    pub text: String,
}

/// A chain-of-thought / reasoning content part.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReasoningPart {
    pub id: String,
    pub reasoning: String,
}

/// A tool-call part, tracking the full lifecycle of a single tool invocation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolPart {
    pub id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub state: ToolPartState,
}

/// The lifecycle state of a [`ToolPart`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ToolPartState {
    Pending,
    Running {
        input: serde_json::Value,
    },
    Completed {
        input: serde_json::Value,
        output: String,
        title: Option<String>,
        metadata: Option<serde_json::Value>,
        time_start: i64,
        time_end: i64,
    },
    Error {
        input: serde_json::Value,
        error: String,
        time_start: i64,
        time_end: i64,
    },
}

/// A file-attachment part.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilePart {
    pub id: String,
    pub path: String,
    pub mime_type: Option<String>,
    pub url: Option<String>,
}

/// A compaction marker part, replacing earlier context with a summary.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompactionPart {
    pub id: String,
    pub summary: String,
    pub tokens_removed: u64,
}

/// Marks the beginning of an agentic step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepStartPart {
    pub id: String,
}

/// Marks the end of an agentic step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepFinishPart {
    pub id: String,
    pub finish_reason: String,
    pub usage: Option<UsageSummary>,
}

/// Aggregated token-usage statistics for a completed step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

/// Generate a new unique part ID.
#[must_use]
pub fn new_part_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Generate a new unique message ID.
#[must_use]
pub fn new_message_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_parts() -> Vec<Part> {
        vec![
            Part::Text(TextPart {
                id: new_part_id(),
                text: "hello".into(),
            }),
            Part::Reasoning(ReasoningPart {
                id: new_part_id(),
                reasoning: "thinking…".into(),
            }),
            Part::Tool(ToolPart {
                id: new_part_id(),
                tool_id: "t1".into(),
                tool_name: "read_file".into(),
                state: ToolPartState::Completed {
                    input: serde_json::json!({"path": "/tmp/foo"}),
                    output: "file content".into(),
                    title: Some("Read /tmp/foo".into()),
                    metadata: None,
                    time_start: 0,
                    time_end: 1,
                },
            }),
            Part::File(FilePart {
                id: new_part_id(),
                path: "/tmp/bar.rs".into(),
                mime_type: Some("text/x-rust".into()),
                url: None,
            }),
            Part::Compaction(CompactionPart {
                id: new_part_id(),
                summary: "earlier context".into(),
                tokens_removed: 1024,
            }),
            Part::StepStart(StepStartPart { id: new_part_id() }),
            Part::StepFinish(StepFinishPart {
                id: new_part_id(),
                finish_reason: "stop".into(),
                usage: Some(UsageSummary {
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                }),
            }),
        ]
    }

    #[test]
    fn part_roundtrip_all_variants() {
        for part in sample_parts() {
            let json = serde_json::to_string(&part).expect("serialize");
            let back: Part = serde_json::from_str(&json).expect("deserialize");
            // Re-serialise and compare strings for a stable equality check.
            let json2 = serde_json::to_string(&back).expect("re-serialize");
            assert_eq!(json, json2, "roundtrip mismatch for part");
        }
    }
}
