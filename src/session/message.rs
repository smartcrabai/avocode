#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: MessageRole,
    pub parts: Vec<Part>,
    pub time_created: i64,
    pub time_updated: i64,
}

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextPart {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReasoningPart {
    pub id: String,
    pub reasoning: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolPart {
    pub id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub state: ToolPartState,
}

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilePart {
    pub id: String,
    pub path: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompactionPart {
    pub id: String,
    pub summary: String,
    pub tokens_removed: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepStartPart {
    pub id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepFinishPart {
    pub id: String,
    pub finish_reason: String,
    pub usage: Option<UsageSummary>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl Part {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(TextPart {
            id: super::schema::new_id(),
            text: text.into(),
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Text(p) => &p.id,
            Self::Reasoning(p) => &p.id,
            Self::Tool(p) => &p.id,
            Self::File(p) => &p.id,
            Self::Compaction(p) => &p.id,
            Self::StepStart(p) => &p.id,
            Self::StepFinish(p) => &p.id,
        }
    }
}

impl Message {
    pub fn user(session_id: String, content: impl Into<String>) -> Self {
        let now = super::schema::now_ms();
        Self {
            id: super::schema::new_id(),
            session_id,
            role: MessageRole::User,
            parts: vec![Part::text(content)],
            time_created: now,
            time_updated: now,
        }
    }

    #[must_use]
    pub fn assistant(session_id: String) -> Self {
        let now = super::schema::now_ms();
        Self {
            id: super::schema::new_id(),
            session_id,
            role: MessageRole::Assistant,
            parts: Vec::new(),
            time_created: now,
            time_updated: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_text_serializes_and_deserializes() -> Result<(), Box<dyn std::error::Error>> {
        let part = Part::text("hello world");
        let json = serde_json::to_string(&part)?;
        let back: Part = serde_json::from_str(&json)?;
        assert!(matches!(back, Part::Text(ref p) if p.text == "hello world"));
        Ok(())
    }

    #[test]
    fn part_tool_pending_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let part = Part::Tool(ToolPart {
            id: "p1".to_owned(),
            tool_id: "t1".to_owned(),
            tool_name: "bash".to_owned(),
            state: ToolPartState::Pending,
        });
        let json = serde_json::to_string(&part)?;
        let back: Part = serde_json::from_str(&json)?;
        assert!(matches!(back, Part::Tool(_)));
        Ok(())
    }

    #[test]
    fn part_tool_completed_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let part = Part::Tool(ToolPart {
            id: "p2".to_owned(),
            tool_id: "t2".to_owned(),
            tool_name: "read_file".to_owned(),
            state: ToolPartState::Completed {
                input: serde_json::json!({"path": "/foo"}),
                output: "file contents".to_owned(),
                title: Some("Read file".to_owned()),
                metadata: None,
                time_start: 1000,
                time_end: 2000,
            },
        });
        let json = serde_json::to_string(&part)?;
        let back: Part = serde_json::from_str(&json)?;
        assert!(matches!(back, Part::Tool(_)));
        Ok(())
    }

    #[test]
    fn part_compaction_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let part = Part::Compaction(CompactionPart {
            id: "c1".to_owned(),
            summary: "summary text".to_owned(),
            tokens_removed: 500,
        });
        let json = serde_json::to_string(&part)?;
        let back: Part = serde_json::from_str(&json)?;
        assert!(matches!(back, Part::Compaction(ref p) if p.tokens_removed == 500));
        Ok(())
    }

    #[test]
    fn part_step_start_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let part = Part::StepStart(StepStartPart {
            id: "s1".to_owned(),
        });
        let json = serde_json::to_string(&part)?;
        let back: Part = serde_json::from_str(&json)?;
        assert!(matches!(back, Part::StepStart(_)));
        Ok(())
    }

    #[test]
    fn part_step_finish_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let part = Part::StepFinish(StepFinishPart {
            id: "sf1".to_owned(),
            finish_reason: "stop".to_owned(),
            usage: Some(UsageSummary {
                input_tokens: 10,
                output_tokens: 20,
                cache_read_tokens: 0,
                cache_write_tokens: 5,
            }),
        });
        let json = serde_json::to_string(&part)?;
        let back: Part = serde_json::from_str(&json)?;
        assert!(matches!(back, Part::StepFinish(_)));
        Ok(())
    }

    #[test]
    fn part_file_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let part = Part::File(FilePart {
            id: "f1".to_owned(),
            path: "/some/file.txt".to_owned(),
            mime_type: Some("text/plain".to_owned()),
        });
        let json = serde_json::to_string(&part)?;
        let back: Part = serde_json::from_str(&json)?;
        assert!(matches!(back, Part::File(ref p) if p.path == "/some/file.txt"));
        Ok(())
    }

    #[test]
    fn part_reasoning_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let part = Part::Reasoning(ReasoningPart {
            id: "r1".to_owned(),
            reasoning: "step by step".to_owned(),
        });
        let json = serde_json::to_string(&part)?;
        let back: Part = serde_json::from_str(&json)?;
        assert!(matches!(back, Part::Reasoning(_)));
        Ok(())
    }

    #[test]
    fn message_user_has_text_part() {
        let msg = Message::user("session-1".to_owned(), "hello");
        assert!(matches!(msg.role, MessageRole::User));
        assert_eq!(msg.parts.len(), 1);
        assert!(matches!(&msg.parts[0], Part::Text(p) if p.text == "hello"));
    }

    #[test]
    fn message_assistant_has_no_parts() {
        let msg = Message::assistant("session-1".to_owned());
        assert!(matches!(msg.role, MessageRole::Assistant));
        assert!(msg.parts.is_empty());
    }

    #[test]
    fn part_id_returns_inner_id() {
        let part = Part::text("test");
        let id = part.id();
        assert!(!id.is_empty());
    }
}
