use super::message::{Message, Part};

#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

#[must_use]
pub fn part_tokens(part: &Part) -> usize {
    match part {
        Part::Text(p) => estimate_tokens(&p.text),
        Part::Reasoning(p) => estimate_tokens(&p.reasoning),
        Part::Tool(p) => match &p.state {
            super::message::ToolPartState::Completed { input, output, .. } => {
                estimate_tokens(output) + estimate_tokens(&input.to_string())
            }
            super::message::ToolPartState::Error { error, .. } => estimate_tokens(error),
            _ => 50,
        },
        Part::Compaction(p) => estimate_tokens(&p.summary),
        _ => 20,
    }
}

pub fn message_tokens(msg: &Message) -> usize {
    msg.parts.iter().map(part_tokens).sum()
}

/// Prune tool output content from old messages to reduce token usage.
/// Keeps the most recent `protect_tokens` worth of tool outputs.
/// Returns number of tokens pruned.
pub fn prune_tool_outputs(
    messages: &mut [Message],
    protect_tokens: usize,
    min_prune_tokens: usize,
) -> usize {
    let total: usize = messages.iter().map(message_tokens).sum();
    if total <= protect_tokens + min_prune_tokens {
        return 0;
    }

    let mut pruned = 0usize;
    for msg in &mut *messages {
        for part in &mut msg.parts {
            if let Part::Tool(tool_part) = part
                && let super::message::ToolPartState::Completed { ref mut output, .. } =
                    tool_part.state
                && !output.is_empty()
            {
                pruned += estimate_tokens(output);
                "[output pruned]".clone_into(output);
            }
        }
        if pruned >= min_prune_tokens {
            break;
        }
    }
    pruned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::message::{Message, Part, ToolPart, ToolPartState};

    #[test]
    fn estimate_tokens_returns_chars_divided_by_four() {
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens(""), 0);
        // 12 chars -> 3
        assert_eq!(estimate_tokens("abcdefghijkl"), 3);
    }

    #[test]
    fn prune_tool_outputs_sets_output_to_pruned() {
        let session_id = "sess-1".to_owned();
        let mut msg = Message::assistant(session_id.clone());
        msg.parts.push(Part::Tool(ToolPart {
            id: "p1".to_owned(),
            tool_id: "t1".to_owned(),
            tool_name: "bash".to_owned(),
            state: ToolPartState::Completed {
                input: serde_json::json!({}),
                // 400 chars -> 100 tokens
                output: "x".repeat(400),
                title: None,
                metadata: None,
                time_start: 0,
                time_end: 1,
            },
        }));

        let mut messages = vec![msg];
        let pruned = prune_tool_outputs(&mut messages, 0, 10);
        assert!(pruned > 0);

        let part = &messages[0].parts[0];
        if let Part::Tool(tp) = part {
            if let ToolPartState::Completed { ref output, .. } = tp.state {
                assert_eq!(output, "[output pruned]");
            } else {
                panic!("Expected Completed state");
            }
        } else {
            panic!("Expected Tool part");
        }
    }

    #[test]
    fn prune_tool_outputs_skips_when_below_threshold() {
        let session_id = "sess-2".to_owned();
        let mut msg = Message::assistant(session_id.clone());
        msg.parts.push(Part::Tool(ToolPart {
            id: "p1".to_owned(),
            tool_id: "t1".to_owned(),
            tool_name: "bash".to_owned(),
            state: ToolPartState::Completed {
                input: serde_json::json!({}),
                output: "small".to_owned(),
                title: None,
                metadata: None,
                time_start: 0,
                time_end: 1,
            },
        }));

        let mut messages = vec![msg];
        // protect_tokens + min_prune_tokens is very large, so skip pruning
        let pruned = prune_tool_outputs(&mut messages, 10_000, 10_000);
        assert_eq!(pruned, 0);

        let part = &messages[0].parts[0];
        if let Part::Tool(tp) = part {
            if let ToolPartState::Completed { ref output, .. } = tp.state {
                assert_eq!(output, "small");
            } else {
                panic!("Expected Completed state");
            }
        } else {
            panic!("Expected Tool part");
        }
    }

    #[test]
    fn message_tokens_sums_all_parts() {
        let mut msg = Message::user("sess".to_owned(), "abcd"); // 1 token
        msg.parts.push(Part::text("efghijkl")); // 2 tokens
        let tokens = message_tokens(&msg);
        assert_eq!(tokens, 3);
    }
}
