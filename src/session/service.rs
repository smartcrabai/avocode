use std::sync::Arc;
use tokio::sync::broadcast;

use crate::server::ServerEvent;
use crate::session::store::SessionStore;
use crate::session::{Message, SessionError};

/// Options for running a prompt against a session.
pub struct RunOptions {
    pub session_id: String,
    pub content: String,
    /// Optional `"provider/model"` override (e.g. `"openai/gpt-4o"`).
    pub model: Option<String>,
    /// Agent name to use (e.g. `"build"`, `"plan"`).
    /// Defaults to `"build"` when `None`.
    pub agent: Option<String>,
    /// When `true`, persist the user message but skip the LLM call entirely.
    /// No assistant message is created or returned.
    pub no_reply: bool,
}

/// Result of a successfully completed prompt run.
pub struct RunResult {
    /// Accumulated assistant response text.  Empty when `no_reply` is `true`.
    pub text: String,
    /// The persisted assistant [`Message`].  `None` when `no_reply` is `true`.
    pub message: Option<Message>,
}

/// Run a user prompt against an existing session.
///
/// This is the single canonical execution path shared by CLI, TUI, and HTTP.
/// It replaces the inline `tokio::spawn` / mpsc-drain pattern that was
/// previously duplicated in each caller.
///
/// # Errors
///
/// Returns [`SessionError::NotFound`] when the session does not exist.
/// LLM / config failures are surfaced as [`SessionError::Other`].
pub async fn run_prompt(
    store: &Arc<SessionStore>,
    event_tx: &broadcast::Sender<ServerEvent>,
    options: RunOptions,
) -> Result<RunResult, SessionError> {
    let _session = store
        .get_session(&options.session_id)?
        .ok_or_else(|| SessionError::NotFound(options.session_id.clone()))?;

    if options.no_reply {
        let user_message = Message::user(options.session_id.clone(), options.content);
        store.add_message(&user_message)?;
        let _ = event_tx.send(ServerEvent::MessageCreated {
            session_id: options.session_id.clone(),
            message_id: user_message.id.clone(),
        });
        return Ok(RunResult {
            text: String::new(),
            message: None,
        });
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let process_options = crate::session::processor::ProcessOptions {
        session_id: options.session_id.clone(),
        user_message: options.content,
        model: options.model,
        agent: options.agent.unwrap_or_else(|| "build".to_owned()),
    };

    let store_for_proc = store.clone();
    let session_id_for_proc = options.session_id.clone();
    let proc_handle = tokio::spawn(async move {
        crate::session::processor::process(&store_for_proc, process_options, tx).await
    });

    let mut assistant_text = String::new();
    let mut assistant_message: Option<Message> = None;

    while let Some(event) = rx.recv().await {
        match event {
            crate::session::processor::ProcessEvent::PartUpdated { part, .. } => {
                if let crate::session::Part::Text(t) = part {
                    assistant_text.push_str(&t.text);
                }
            }
            crate::session::processor::ProcessEvent::MessageCreated { message } => {
                let _ = event_tx.send(ServerEvent::MessageCreated {
                    session_id: session_id_for_proc.clone(),
                    message_id: message.id.clone(),
                });
                assistant_message = Some(message);
            }
            crate::session::processor::ProcessEvent::Done => break,
            crate::session::processor::ProcessEvent::Error(e) => {
                return Err(SessionError::Other(e));
            }
        }
    }

    // Await the processor task so panics and errors are not silently lost.
    proc_handle
        .await
        .map_err(|e| SessionError::Other(e.to_string()))?
        .map_err(|e| SessionError::Other(e.to_string()))?;

    Ok(RunResult {
        text: assistant_text,
        message: assistant_message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::AppState;
    use crate::session::store::SessionStore;

    /// Helper: build an Arc<SessionStore> backed by an in-memory `SQLite` database.
    fn in_memory_store() -> Result<Arc<SessionStore>, Box<dyn std::error::Error>> {
        Ok(Arc::new(SessionStore::open_in_memory()?))
    }

    // -----------------------------------------------------------------------
    // run_prompt – error cases (do not require an LLM)
    // -----------------------------------------------------------------------

    /// Calling `run_prompt` with a `session_id` that does not exist in the
    /// store must return an error.  The service must validate the session
    /// before touching the LLM.
    #[tokio::test]
    async fn run_prompt_returns_error_for_unknown_session_id()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: an empty store and a broadcast channel
        let store = in_memory_store()?;
        let event_tx = AppState::new().event_tx.clone();

        // When: run_prompt is called with a non-existent session_id
        let result = run_prompt(
            &store,
            &event_tx,
            RunOptions {
                session_id: "no-such-session".to_owned(),
                content: "hello".to_owned(),
                model: None,
                agent: None,
                no_reply: false,
            },
        )
        .await;

        // Then: result is an error
        assert!(result.is_err(), "expected error for unknown session_id");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // run_prompt – no_reply = true
    // -----------------------------------------------------------------------

    /// When `no_reply = true` the user message is persisted but no LLM call
    /// is made.  `RunResult::message` must be `None` and `RunResult::text`
    /// must be empty.
    #[tokio::test]
    async fn run_prompt_with_no_reply_skips_llm_and_returns_empty_result()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: a session in the store
        let store = in_memory_store()?;
        let session = crate::session::Session::new("proj-1".to_owned(), "/tmp/project".to_owned());
        let session_id = session.id.clone();
        store.create_session(&session)?;

        let event_tx = AppState::new().event_tx.clone();

        // When: run_prompt is called with no_reply=true
        let result = run_prompt(
            &store,
            &event_tx,
            RunOptions {
                session_id: session_id.clone(),
                content: "silent".to_owned(),
                model: None,
                agent: None,
                no_reply: true,
            },
        )
        .await?;

        // Then: no assistant message is returned
        assert!(
            result.message.is_none(),
            "no_reply=true must not produce an assistant message"
        );
        assert!(
            result.text.is_empty(),
            "no_reply=true must produce empty text"
        );

        // Then: exactly one message (user) is in the store
        let messages = store.list_messages(&session_id)?;
        assert_eq!(
            messages.len(),
            1,
            "only the user message should be persisted when no_reply=true"
        );
        assert!(
            matches!(messages[0].role, crate::session::MessageRole::User),
            "the persisted message should be the user message"
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // run_prompt – event broadcasting
    // -----------------------------------------------------------------------

    /// When a prompt runs successfully, at least a `MessageCreated` event must
    /// be broadcast so SSE clients can observe the new message.
    ///
    /// This test verifies the broadcast contract without checking LLM output.
    /// It relies on `no_reply=false` triggering the full code path, but only
    /// asserts on the event bus, not on the text content.
    ///
    /// NOTE: This test will remain `#[ignore]` until LLM mocking is wired up
    /// in the unit-test layer.  The integration counterpart
    /// (`http_event_stream_emits_message_events_after_send`) covers this
    /// behaviour end-to-end.
    #[tokio::test]
    #[ignore = "requires LLM mock — covered by integration test"]
    async fn run_prompt_broadcasts_message_created_event() -> Result<(), Box<dyn std::error::Error>>
    {
        // Given: a session and a subscribed receiver
        let store = in_memory_store()?;
        let session = crate::session::Session::new("proj-1".to_owned(), "/tmp/project".to_owned());
        let session_id = session.id.clone();
        store.create_session(&session)?;

        let state = AppState::new();
        let mut rx = state.subscribe();
        let event_tx = state.event_tx.clone();

        // When: a prompt is executed (LLM must be mocked at integration level)
        let _ = run_prompt(
            &store,
            &event_tx,
            RunOptions {
                session_id: session_id.clone(),
                content: "hello".to_owned(),
                model: Some("openai/gpt-4o".to_owned()),
                agent: None,
                no_reply: false,
            },
        )
        .await;

        // Then: a MessageCreated event is present in the channel
        let mut found = false;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, ServerEvent::MessageCreated { .. }) {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected a MessageCreated event on the broadcast channel"
        );

        Ok(())
    }
}
