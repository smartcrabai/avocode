pub struct ProcessOptions {
    pub session_id: String,
    pub user_message: String,
    pub model: String,
    pub agent: String,
    pub max_turns: Option<u32>,
}

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

/// Skeleton agent loop. Creates user message and emits `Done`.
///
/// # Errors
/// Returns [`super::SessionError`] if the message cannot be persisted to the store.
pub async fn process(
    store: &super::store::SessionStore,
    options: ProcessOptions,
    tx: tokio::sync::mpsc::Sender<ProcessEvent>,
) -> Result<(), super::SessionError> {
    let message = super::message::Message::user(options.session_id.clone(), options.user_message);
    store.add_message(&message)?;
    let _ = tx.send(ProcessEvent::MessageCreated { message }).await;
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
            agent: "default".to_owned(),
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
            agent: "default".to_owned(),
            max_turns: Some(5),
        };

        process(&store, options, tx).await?;

        let messages = store.list_messages(&session.id)?;
        assert_eq!(messages.len(), 1);

        Ok(())
    }
}
