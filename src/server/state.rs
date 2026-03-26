use tokio::sync::broadcast;

/// Events that can be sent via SSE to clients
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    SessionCreated {
        session_id: String,
    },
    SessionUpdated {
        session_id: String,
    },
    MessageCreated {
        session_id: String,
        message_id: String,
    },
    PartUpdated {
        session_id: String,
        message_id: String,
        part_id: String,
    },
    PermissionAsked {
        request_id: String,
        session_id: String,
        permission: String,
        pattern: String,
        metadata: serde_json::Value,
    },
    PermissionReplied {
        request_id: String,
        reply: String,
    },
}

/// Shared state passed to all axum handlers
#[derive(Clone)]
pub struct AppState {
    pub event_tx: broadcast::Sender<ServerEvent>,
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self { event_tx }
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.event_tx.subscribe()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_new_creates_working_broadcast_channel() {
        let state = AppState::new();
        let mut rx = state.subscribe();
        let event = ServerEvent::SessionCreated {
            session_id: "test-id".to_owned(),
        };
        state.event_tx.send(event).ok();
        let received = rx.try_recv();
        assert!(received.is_ok());
        if let Ok(ServerEvent::SessionCreated { session_id }) = received {
            assert_eq!(session_id, "test-id");
        } else {
            panic!("unexpected event variant");
        }
    }

    #[test]
    fn server_event_serializes_as_tagged_json() {
        let event = ServerEvent::SessionUpdated {
            session_id: "abc".to_owned(),
        };
        let json = serde_json::to_string(&event).unwrap_or_default();
        assert!(json.contains("\"type\":\"session_updated\""));
        assert!(json.contains("\"session_id\":\"abc\""));
    }
}
