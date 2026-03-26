use tokio::sync::mpsc::Sender;

use crate::session::SessionError;
use crate::session::message::{Message, MessageRole, Part, TextPart, new_message_id, new_part_id};
use crate::session::schema::{Session, now_ms};
use crate::session::store::SessionStore;

/// Configuration for a single agent-loop run.
pub struct ProcessOptions {
    pub session_id: String,
    pub user_message: String,
    /// Provider name, e.g. `"anthropic"` or `"openai"`.
    pub model_provider: String,
    pub model_id: String,
    pub agent: String,
    pub max_turns: Option<u32>,
    pub system: Option<String>,
}

/// Events emitted by the agent loop as it makes progress.
pub enum ProcessEvent {
    PartCreated { message_id: String, part: Part },
    PartUpdated { message_id: String, part: Part },
    MessageCreated { message: Message },
    SessionUpdated { session: Session },
    Done,
    Error(String),
}

/// Drive the agent loop for one user message.
///
/// This is a skeleton implementation.  It stores the user message, emits the
/// corresponding events, and then immediately emits `Done`.  Actual LLM calls
/// will be wired in during integration.
///
/// # Errors
/// Returns a [`SessionError`] if the message cannot be stored or an event
/// cannot be sent to the caller.
pub async fn process(
    store: &SessionStore,
    options: ProcessOptions,
    events: Sender<ProcessEvent>,
) -> Result<(), SessionError> {
    // 1. Build and persist the user message.
    let text_part = Part::Text(TextPart {
        id: new_part_id(),
        text: options.user_message,
    });
    let msg = Message {
        id: new_message_id(),
        session_id: options.session_id,
        role: MessageRole::User,
        parts: vec![text_part.clone()],
        time_created: now_ms(),
        time_updated: now_ms(),
    };

    store.add_message(&msg)?;

    // Capture id before msg is moved into the event.
    let message_id = msg.id.clone();

    // 2. Emit MessageCreated.
    events
        .send(ProcessEvent::MessageCreated { message: msg })
        .await
        .map_err(|e| SessionError::Other(e.to_string()))?;

    // 3. Emit PartCreated for the text part.
    events
        .send(ProcessEvent::PartCreated {
            message_id,
            part: text_part,
        })
        .await
        .map_err(|e| SessionError::Other(e.to_string()))?;

    // TODO: Call LLM with accumulated messages and emit further events.

    events
        .send(ProcessEvent::Done)
        .await
        .map_err(|e| SessionError::Other(e.to_string()))?;

    Ok(())
}
