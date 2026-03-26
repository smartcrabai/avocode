use crate::session::message::{Message, Part, ToolPartState};

/// Rough token estimate: one token per four characters.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Estimate the number of tokens contributed by a single [`Part`].
#[must_use]
pub fn part_tokens(part: &Part) -> usize {
    match part {
        Part::Text(p) => estimate_tokens(&p.text),
        Part::Reasoning(p) => estimate_tokens(&p.reasoning),
        Part::Tool(p) => {
            let state_tokens = match &p.state {
                ToolPartState::Pending => 0,
                ToolPartState::Running { input } => estimate_tokens(&input.to_string()),
                ToolPartState::Completed {
                    input,
                    output,
                    title,
                    metadata,
                    ..
                } => {
                    estimate_tokens(&input.to_string())
                        + estimate_tokens(output)
                        + title.as_deref().map_or(0, estimate_tokens)
                        + metadata
                            .as_ref()
                            .map_or(0, |m| estimate_tokens(&m.to_string()))
                }
                ToolPartState::Error { input, error, .. } => {
                    estimate_tokens(&input.to_string()) + estimate_tokens(error)
                }
            };
            estimate_tokens(&p.tool_name) + state_tokens
        }
        Part::File(p) => {
            estimate_tokens(&p.path)
                + p.mime_type.as_deref().map_or(0, estimate_tokens)
                + p.url.as_deref().map_or(0, estimate_tokens)
        }
        Part::Compaction(p) => estimate_tokens(&p.summary),
        Part::StepStart(_) => 0,
        Part::StepFinish(p) => estimate_tokens(&p.finish_reason),
    }
}

/// Prune completed tool-call outputs from older messages to reduce context size.
///
/// Starting from the oldest message, tool parts whose output is large are
/// cleared (output replaced with an empty string) until the requested number of
/// tokens has been freed, while always protecting the most-recent
/// `protect_tokens` worth of tool output.
///
/// Returns the total number of tokens that were pruned.
pub fn prune_tool_outputs(
    messages: &mut [Message],
    protect_tokens: usize,
    min_prune_tokens: usize,
) -> usize {
    // First pass: walk newest-to-oldest to find which (msg_idx, part_idx) pairs
    // should be protected based on recent token budget.
    let mut protected = std::collections::HashSet::<(usize, usize)>::new();
    let mut acc: usize = 0;

    'protect: for (mi, msg) in messages.iter().enumerate().rev() {
        for (pi, part) in msg.parts.iter().enumerate() {
            if acc >= protect_tokens {
                break 'protect;
            }
            if let Part::Tool(tp) = part
                && let ToolPartState::Completed { output, .. } = &tp.state
            {
                protected.insert((mi, pi));
                acc += estimate_tokens(output);
            }
        }
    }

    // Second pass: walk oldest-to-newest and clear unprotected outputs.
    let mut pruned: usize = 0;
    'outer: for (mi, msg) in messages.iter_mut().enumerate() {
        for (pi, part) in msg.parts.iter_mut().enumerate() {
            if protected.contains(&(mi, pi)) {
                continue;
            }
            if let Part::Tool(tp) = part
                && let ToolPartState::Completed {
                    output,
                    input,
                    title,
                    metadata,
                    time_start,
                    time_end,
                } = &tp.state
            {
                let freed = estimate_tokens(output);
                if freed < min_prune_tokens {
                    continue;
                }
                tp.state = ToolPartState::Completed {
                    input: input.clone(),
                    output: String::new(),
                    title: title.clone(),
                    metadata: metadata.clone(),
                    time_start: *time_start,
                    time_end: *time_end,
                };
                pruned += freed;
                if pruned >= min_prune_tokens {
                    break 'outer;
                }
            }
        }
    }

    pruned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::message::{
        MessageRole, TextPart, ToolPart, ToolPartState as TPS, new_message_id, new_part_id,
    };
    use crate::session::schema::now_ms;

    #[test]
    fn estimate_tokens_chars_over_four() {
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abc"), 0); // integer division
    }

    fn tool_message_with_output(output: &str) -> Message {
        Message {
            id: new_message_id(),
            session_id: "sess".into(),
            role: MessageRole::Assistant,
            parts: vec![Part::Tool(ToolPart {
                id: new_part_id(),
                tool_id: "t1".into(),
                tool_name: "read_file".into(),
                state: TPS::Completed {
                    input: serde_json::json!({}),
                    output: output.to_string(),
                    title: None,
                    metadata: None,
                    time_start: now_ms(),
                    time_end: now_ms(),
                },
            })],
            time_created: now_ms(),
            time_updated: now_ms(),
        }
    }

    #[test]
    fn prune_tool_outputs_removes_tokens() {
        // Create a large output that should be pruned.
        let big_output = "x".repeat(400); // 100 tokens
        let mut messages = vec![
            tool_message_with_output(&big_output),
            Message {
                id: new_message_id(),
                session_id: "sess".into(),
                role: MessageRole::User,
                parts: vec![Part::Text(TextPart {
                    id: new_part_id(),
                    text: "follow-up".into(),
                })],
                time_created: now_ms(),
                time_updated: now_ms(),
            },
        ];

        let pruned = prune_tool_outputs(&mut messages, 0, 1);
        assert!(pruned > 0, "expected some tokens to be pruned");

        // Verify the output was cleared.
        if let Part::Tool(tp) = &messages[0].parts[0] {
            if let TPS::Completed { output, .. } = &tp.state {
                assert!(output.is_empty(), "output should have been cleared");
            } else {
                panic!("expected Completed state");
            }
        } else {
            panic!("expected Tool part");
        }
    }
}
