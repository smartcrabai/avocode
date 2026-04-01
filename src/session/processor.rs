use futures_util::StreamExt as _;

use crate::llm::openai::{OPENAI_API_BASE, OpenAiClient};
use crate::llm::{ChatMessage, ContentPart, MessageRole as LlmRole, StreamOptions};

use super::message::{Message, Part, TextPart};
use super::model_parser::parse_qualified_model;

pub struct ProcessOptions {
    pub session_id: String,
    pub user_message: String,
    pub model: Option<String>,
    pub agent: String,
    pub max_turns: Option<u32>,
}

pub enum ProcessEvent {
    MessageCreated {
        message: super::message::Message,
    },
    /// Carries an incremental text delta (not full snapshot).
    PartUpdated {
        message_id: String,
        part: super::message::Part,
    },
    Done,
    Error(String),
}

/// Run the agent loop: persist user message, call the OpenAI-compatible LLM,
/// stream the assistant reply, and emit `ProcessEvent`s to `tx`.
///
/// # Streaming contract
/// `PartUpdated.part` carries the **delta** text chunk, not the accumulated
/// full text.  Consumers that want the full text must accumulate deltas
/// themselves.
///
/// # Errors
/// Returns [`super::SessionError`] only for unrecoverable store failures.
/// LLM/config errors are communicated through `ProcessEvent::Error` so the
/// caller's channel remains the single error-reporting path.
#[expect(clippy::too_many_lines)]
pub async fn process(
    store: &super::store::SessionStore,
    options: ProcessOptions,
    tx: tokio::sync::mpsc::Sender<ProcessEvent>,
) -> Result<(), super::SessionError> {
    let user_message = Message::user(options.session_id.clone(), options.user_message.clone());
    store.add_message(&user_message)?;
    let _ = tx
        .send(ProcessEvent::MessageCreated {
            message: user_message,
        })
        .await;

    // Load session to find its directory for config resolution.
    let Some(session) = store.get_session(&options.session_id)? else {
        let _ = tx
            .send(ProcessEvent::Error(format!(
                "session {} not found",
                options.session_id
            )))
            .await;
        return Ok(());
    };

    let config =
        crate::config::loader::load(std::path::Path::new(&session.directory)).unwrap_or_default();

    // CLI arg > config.model precedence.
    let Some(model_str) = options.model.or(config.model) else {
        let _ = tx
            .send(ProcessEvent::Error("no model configured".to_owned()))
            .await;
        return Ok(());
    };

    let Some((provider_id, model_id)) = parse_qualified_model(&model_str) else {
        let _ = tx
            .send(ProcessEvent::Error(format!(
                "invalid model format (expected provider/model): {model_str}"
            )))
            .await;
        return Ok(());
    };

    // Only the openai provider is supported for now.
    if provider_id != "openai" {
        let _ = tx
            .send(ProcessEvent::Error(format!(
                "unsupported provider for this execution path: {provider_id}"
            )))
            .await;
        return Ok(());
    }

    let provider_config = config.provider.get(&provider_id);

    // Precedence: env var > config api_key
    let api_key = if let Ok(k) = std::env::var("OPENAI_API_KEY")
        && !k.is_empty()
    {
        k
    } else if let Some(k) = provider_config.and_then(|c| c.api_key.as_ref())
        && !k.is_empty()
    {
        k.clone()
    } else {
        let _ = tx
            .send(ProcessEvent::Error(
                "no OpenAI API key configured (set OPENAI_API_KEY or provider.openai.api_key)"
                    .to_owned(),
            ))
            .await;
        return Ok(());
    };

    let base_url = provider_config
        .and_then(|c| c.base_url.clone())
        .unwrap_or_else(|| OPENAI_API_BASE.to_owned());

    let stored_messages = store.list_messages(&options.session_id)?;
    let llm_messages: Vec<ChatMessage> = stored_messages
        .iter()
        .map(|m| {
            let role = match m.role {
                super::message::MessageRole::User => LlmRole::User,
                super::message::MessageRole::Assistant => LlmRole::Assistant,
            };
            let content: Vec<ContentPart> = m
                .parts
                .iter()
                .filter_map(|p| {
                    if let Part::Text(t) = p {
                        Some(ContentPart::Text {
                            text: t.text.clone(),
                        })
                    } else {
                        None
                    }
                })
                .collect();
            ChatMessage { role, content }
        })
        .collect();

    let stream_opts = StreamOptions {
        model: model_id,
        messages: llm_messages,
        system: None,
        tools: vec![],
        temperature: None,
        top_p: None,
        max_tokens: None,
        extra_headers: std::collections::HashMap::new(),
        api_key,
        base_url,
    };

    let client = OpenAiClient::new();
    let mut stream = match client.stream(&stream_opts).await {
        Ok(s) => Box::pin(s),
        Err(e) => {
            let _ = tx.send(ProcessEvent::Error(e.to_string())).await;
            return Ok(());
        }
    };

    let text_part_id = super::schema::new_id();
    let mut assistant_message = Message::assistant(options.session_id.clone());
    assistant_message.parts.push(Part::Text(TextPart {
        id: text_part_id.clone(),
        text: String::new(),
    }));
    store.add_message(&assistant_message)?;
    let _ = tx
        .send(ProcessEvent::MessageCreated {
            message: assistant_message.clone(),
        })
        .await;

    let message_id = assistant_message.id.clone();
    let mut accumulated_text = String::new();
    while let Some(delta_result) = stream.next().await {
        match delta_result {
            Ok(delta) => {
                if let Some(chunk) = &delta.text {
                    accumulated_text.push_str(chunk);

                    let delta_part = Part::Text(TextPart {
                        id: text_part_id.clone(),
                        text: chunk.clone(),
                    });
                    let _ = tx
                        .send(ProcessEvent::PartUpdated {
                            message_id: message_id.clone(),
                            part: delta_part,
                        })
                        .await;
                }
            }
            Err(e) => {
                if let Part::Text(tp) = &mut assistant_message.parts[0] {
                    tp.text.clone_from(&accumulated_text);
                }
                assistant_message.time_updated = super::schema::now_ms();
                let _ = store.update_message(&assistant_message);
                let _ = tx.send(ProcessEvent::Error(e.to_string())).await;
                return Ok(());
            }
        }
    }

    if let Part::Text(tp) = &mut assistant_message.parts[0] {
        tp.text.clone_from(&accumulated_text);
    }
    assistant_message.time_updated = super::schema::now_ms();
    store.update_message(&assistant_message)?;

    let _ = tx.send(ProcessEvent::Done).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::schema::Session;
    use crate::session::store::SessionStore;

    #[tokio::test]
    async fn process_emits_message_created_then_terminal_event()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        store.create_session(&session)?;

        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let options = ProcessOptions {
            session_id: session.id.clone(),
            user_message: "test message".to_owned(),
            // Unqualified model: processor emits Error as terminal event.
            model: Some("claude-3-5-sonnet".to_owned()),
            agent: "default".to_owned(),
            max_turns: None,
        };

        process(&store, options, tx).await?;

        // First event must always be the user MessageCreated.
        let event1 = rx.recv().await.ok_or("event1")?;
        assert!(matches!(event1, ProcessEvent::MessageCreated { .. }));

        // Unqualified model "claude-3-5-sonnet" has no provider prefix, so parse_qualified_model
        // returns None and the processor always emits Error as the terminal event.
        let event2 = rx.recv().await.ok_or("event2")?;
        assert!(
            matches!(event2, ProcessEvent::Error(_)),
            "expected Error for unqualified model"
        );

        Ok(())
    }

    #[tokio::test]
    async fn process_stores_user_message() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        store.create_session(&session)?;

        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let options = ProcessOptions {
            session_id: session.id.clone(),
            user_message: "hello agent".to_owned(),
            model: Some("claude-3-5-sonnet".to_owned()),
            agent: "default".to_owned(),
            max_turns: Some(5),
        };

        process(&store, options, tx).await?;

        let messages = store.list_messages(&session.id)?;
        // At minimum the user message must be stored.
        assert!(!messages.is_empty());

        Ok(())
    }
}
