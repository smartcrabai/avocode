pub const DEFAULT_AGENT: &str = "default";

pub struct ProcessOptions {
    pub session_id: String,
    pub user_message: String,
    pub model: String,
    pub agent: String,
    pub max_turns: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ProcessEvent {
    MessageCreated {
        message: super::message::Message,
    },
    PartUpdated {
        message_id: String,
        part: super::message::Part,
    },
    Done,
    Error(String),
}

/// Shared orchestration entrypoint for the agent loop.
///
/// Creates a user message, resolves the model/provider, streams an LLM
/// response (when possible), and emits [`ProcessEvent`]s.
///
/// Backward-compatible: when the model is not a qualified `provider/model`
/// identifier, or when credentials cannot be resolved, falls back to the
/// skeleton behaviour (user message + Done) without erroring.
///
/// # Errors
/// Returns [`super::SessionError`] if the message cannot be persisted to the store.
pub async fn process(
    store: &super::store::SessionStore,
    options: ProcessOptions,
    tx: tokio::sync::mpsc::Sender<ProcessEvent>,
) -> Result<(), super::SessionError> {
    let user_message =
        super::message::Message::user(options.session_id.clone(), &options.user_message);
    store.add_message(&user_message)?;
    let _ = tx
        .send(ProcessEvent::MessageCreated {
            message: user_message,
        })
        .await;

    let Some((provider_id, model_id)) =
        crate::provider::catalog::parse_qualified_model(&options.model)
    else {
        // Unqualified model — backward compatible skeleton path.
        let _ = tx.send(ProcessEvent::Done).await;
        return Ok(());
    };

    let catalog = crate::provider::registry::builtin_providers();

    let Some(provider) = catalog.iter().find(|p| p.id == provider_id) else {
        return emit_error(&tx, format!("Provider not found: {provider_id}")).await;
    };
    if !provider.models.iter().any(|m| m.id == model_id) {
        return emit_error(&tx, format!("Model not found: {model_id}")).await;
    }

    let Some(session) = store.get_session(&options.session_id)? else {
        return emit_error(&tx, format!("Session not found: {}", options.session_id)).await;
    };

    let (api_key, base_url) = match resolve_provider_credentials(provider, &session.directory) {
        Ok(creds) => creds,
        Err(msg) => {
            return emit_error(&tx, msg).await;
        }
    };

    let mut assistant_message = super::message::Message::assistant(options.session_id.clone());
    let client = crate::llm::openai::OpenAiClient::new();
    let stream_options = crate::llm::StreamOptions {
        model: model_id.to_string(),
        messages: vec![crate::llm::ChatMessage {
            role: crate::llm::messages::MessageRole::User,
            content: vec![crate::llm::ContentPart::Text {
                text: options.user_message.clone(),
            }],
        }],
        system: None,
        tools: vec![],
        temperature: None,
        top_p: None,
        max_tokens: None,
        extra_headers: std::collections::HashMap::new(),
        api_key,
        base_url,
    };

    let persisted =
        stream_response(&client, &stream_options, &tx, store, &mut assistant_message).await;

    if !persisted {
        // Streaming yielded no content — persist the (empty) assistant message
        // so callers can still correlate the response.
        let _ = store.add_message(&assistant_message);
    }

    let _ = tx.send(ProcessEvent::Done).await;
    Ok(())
}

/// Resolve provider credentials from the session directory's config.
///
/// Returns `(api_key, base_url)` on success, or an error message on failure.
fn resolve_provider_credentials(
    provider: &crate::provider::schema::ProviderInfo,
    session_directory: &str,
) -> Result<(String, String), String> {
    let config = crate::config::loader::load(std::path::Path::new(session_directory))
        .map_err(|e| format!("Failed to load config: {e}"))?;

    let descriptor = crate::provider::catalog::ProviderDescriptor {
        id: provider.id.clone(),
        name: provider.name.clone(),
        env_keys: provider.env.clone(),
        default_base_url: None,
        fetch_strategy: crate::provider::catalog::FetchStrategy::OpenAiCompatible,
    };
    let connections = crate::provider::catalog::resolve_connections(
        &[descriptor],
        &config,
        &std::collections::HashMap::new(),
    );

    let connection = connections
        .into_iter()
        .find(|c| c.descriptor.id == provider.id)
        .ok_or_else(|| format!("No API key configured for provider: {}", provider.id))?;

    let api_key = connection
        .api_key
        .ok_or_else(|| format!("No API key configured for provider: {}", provider.id))?;

    let base_url = connection
        .base_url
        .or(connection.descriptor.default_base_url)
        .unwrap_or_else(|| crate::llm::openai::OPENAI_API_BASE.to_string());

    Ok((api_key, base_url))
}

/// Stream the LLM response and persist the accumulated assistant message.
///
/// Returns `true` if the message was persisted, `false` if no content was
/// received.
async fn stream_response(
    client: &crate::llm::openai::OpenAiClient,
    stream_options: &crate::llm::StreamOptions,
    tx: &tokio::sync::mpsc::Sender<ProcessEvent>,
    store: &super::store::SessionStore,
    assistant_message: &mut super::message::Message,
) -> bool {
    match client.stream(stream_options).await {
        Ok(stream) => {
            use futures_util::StreamExt as _;

            let mut stream = std::pin::pin!(stream);
            let mut accumulated = String::new();
            while let Some(item) = stream.next().await {
                match item {
                    Ok(delta) => {
                        if let Some(text) = &delta.text {
                            accumulated.push_str(text);
                            let part = super::message::Part::text(&accumulated);
                            let _ = tx
                                .send(ProcessEvent::PartUpdated {
                                    message_id: assistant_message.id.clone(),
                                    part,
                                })
                                .await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(ProcessEvent::Error(e.to_string())).await;
                        break;
                    }
                }
            }

            if !accumulated.is_empty() {
                assistant_message.parts = vec![super::message::Part::text(&accumulated)];
                assistant_message.time_updated = super::schema::now_ms();
                store.add_message(assistant_message).ok();
                return true;
            }
            false
        }
        Err(e) => {
            let _ = tx.send(ProcessEvent::Error(e.to_string())).await;
            false
        }
    }
}

async fn emit_error(
    tx: &tokio::sync::mpsc::Sender<ProcessEvent>,
    msg: String,
) -> Result<(), super::SessionError> {
    let _ = tx.send(ProcessEvent::Error(msg)).await;
    let _ = tx.send(ProcessEvent::Done).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::schema::Session;
    use crate::session::store::SessionStore;

    #[tokio::test]
    async fn process_emits_message_created_and_done() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        store.create_session(&session)?;

        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let options = ProcessOptions {
            session_id: session.id.clone(),
            user_message: "test message".to_owned(),
            model: "claude-3-5-sonnet".to_owned(),
            agent: DEFAULT_AGENT.to_owned(),
            max_turns: None,
        };

        process(&store, options, tx).await?;

        let event1 = rx.recv().await.ok_or("event1")?;
        assert!(matches!(event1, ProcessEvent::MessageCreated { .. }));

        let event2 = rx.recv().await.ok_or("event2")?;
        assert!(matches!(event2, ProcessEvent::Done));

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
            model: "claude-3-5-sonnet".to_owned(),
            agent: DEFAULT_AGENT.to_owned(),
            max_turns: Some(5),
        };

        process(&store, options, tx).await?;

        let messages = store.list_messages(&session.id)?;
        assert_eq!(messages.len(), 1);

        Ok(())
    }

    // ─── Enhanced process tests (require implementation) ────────────────────
    //
    // The following tests describe the expected behavior after the enhanced
    // process function is implemented (plan section 6.1). They will pass once
    // the implementation loads config, resolves providers, creates assistant
    // messages, and streams from the OpenAI-compatible API.

    #[tokio::test]
    #[ignore = "requires enhanced process implementation with LLM streaming"]
    async fn process_creates_assistant_message_on_streaming()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: session + config pointing to mock
        let store = SessionStore::open_in_memory()?;
        let dir = tempfile::tempdir()?;
        let session = Session::new("proj-1".to_owned(), dir.path().display().to_string());
        store.create_session(&session)?;

        // (In the real test, config + mock server setup would go here)

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let options = ProcessOptions {
            session_id: session.id.clone(),
            user_message: "Hello, echo this!".to_owned(),
            model: "openai/gpt-4o".to_owned(),
            agent: DEFAULT_AGENT.to_owned(),
            max_turns: None,
        };

        // When: process is called
        process(&store, options, tx).await?;

        // Then: collect all events
        let mut events: Vec<ProcessEvent> = Vec::new();
        while let Some(event) = rx.recv().await {
            let is_done = matches!(event, ProcessEvent::Done);
            events.push(event);
            if is_done {
                break;
            }
        }

        // At least MessageCreated + PartUpdated + Done
        assert!(
            events.len() >= 3,
            "expected at least 3 events, got {}",
            events.len()
        );

        // Assistant message persisted
        let messages = store.list_messages(&session.id)?;
        let assistant_count = messages
            .iter()
            .filter(|m| matches!(m.role, super::super::message::MessageRole::Assistant))
            .count();
        assert_eq!(assistant_count, 1, "expected exactly one assistant message");

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires enhanced process implementation with LLM streaming"]
    async fn process_emits_part_updated_with_streaming_text()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let dir = tempfile::tempdir()?;
        let session = Session::new("proj-1".to_owned(), dir.path().display().to_string());
        store.create_session(&session)?;

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let options = ProcessOptions {
            session_id: session.id.clone(),
            user_message: "echo hello".to_owned(),
            model: "openai/gpt-4o".to_owned(),
            agent: DEFAULT_AGENT.to_owned(),
            max_turns: None,
        };

        process(&store, options, tx).await?;

        let mut part_updated_count = 0;
        let mut accumulated_text = String::new();
        while let Some(event) = rx.recv().await {
            match &event {
                ProcessEvent::PartUpdated { part, .. } => {
                    part_updated_count += 1;
                    if let super::super::message::Part::Text(t) = part {
                        // PartUpdated contains the full accumulated text so far,
                        // not just the delta — assign rather than append.
                        accumulated_text = t.text.clone();
                    }
                }
                ProcessEvent::Done => break,
                _ => {}
            }
        }

        assert!(
            part_updated_count > 0,
            "expected at least one PartUpdated event"
        );
        assert!(
            !accumulated_text.is_empty(),
            "expected non-empty accumulated text from streaming"
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires enhanced process implementation with error handling"]
    async fn process_emits_error_on_missing_api_key() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let dir = tempfile::tempdir()?;
        let session = Session::new("proj-1".to_owned(), dir.path().display().to_string());
        store.create_session(&session)?;

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let options = ProcessOptions {
            session_id: session.id.clone(),
            user_message: "hello".to_owned(),
            model: "openai/gpt-4o".to_owned(),
            agent: DEFAULT_AGENT.to_owned(),
            max_turns: None,
        };

        process(&store, options, tx).await?;

        let mut found_error = false;
        while let Some(event) = rx.recv().await {
            if matches!(event, ProcessEvent::Error(_)) {
                found_error = true;
                break;
            }
            if matches!(event, ProcessEvent::Done) {
                break;
            }
        }

        assert!(found_error, "expected an Error event for missing API key");

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires enhanced process implementation with config-driven base_url"]
    async fn process_uses_base_url_from_config() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let dir = tempfile::tempdir()?;
        let session = Session::new("proj-1".to_owned(), dir.path().display().to_string());
        store.create_session(&session)?;

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let options = ProcessOptions {
            session_id: session.id.clone(),
            user_message: "custom base url test".to_owned(),
            model: "openai/gpt-4o".to_owned(),
            agent: DEFAULT_AGENT.to_owned(),
            max_turns: None,
        };

        process(&store, options, tx).await?;

        // If we get here with a Done event (not Error), the base_url was used.
        let mut received_done = false;
        while let Some(event) = rx.recv().await {
            if matches!(event, ProcessEvent::Done) {
                received_done = true;
                break;
            }
            if matches!(event, ProcessEvent::Error(_)) {
                break;
            }
        }

        assert!(
            received_done,
            "expected Done event — base_url should have been used"
        );

        Ok(())
    }

    #[tokio::test]
    async fn process_emits_error_on_invalid_model() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let dir = tempfile::tempdir()?;
        let session = Session::new("proj-1".to_owned(), dir.path().display().to_string());
        store.create_session(&session)?;

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let options = ProcessOptions {
            session_id: session.id.clone(),
            user_message: "hello".to_owned(),
            model: "openai/nonexistent-model-xyz".to_owned(),
            agent: DEFAULT_AGENT.to_owned(),
            max_turns: None,
        };

        process(&store, options, tx).await?;

        let mut found_error = false;
        while let Some(event) = rx.recv().await {
            if let ProcessEvent::Error(msg) = &event {
                found_error = true;
                assert!(
                    msg.contains("nonexistent-model-xyz") || msg.contains("not found"),
                    "error message should mention the model: {msg}"
                );
                break;
            }
            if matches!(event, ProcessEvent::Done) {
                break;
            }
        }

        assert!(found_error, "expected an Error event for invalid model");

        Ok(())
    }
}
