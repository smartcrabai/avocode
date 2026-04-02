use std::sync::Arc;
use tokio::sync::broadcast;

use crate::permission::PermissionManager;
use crate::provider::ProviderInfo;
use crate::session::store::SessionStore;

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

impl ServerEvent {
    /// Return the `PascalCase` variant name, used as the SSE `event:` field.
    ///
    /// Keeping this as a separate method preserves the `snake_case` JSON
    /// serialisation (via `#[serde(rename_all = "snake_case")]`) while still
    /// letting clients match on `event: MessageCreated` / `event: PartUpdated`.
    #[must_use]
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::SessionCreated { .. } => "SessionCreated",
            Self::SessionUpdated { .. } => "SessionUpdated",
            Self::MessageCreated { .. } => "MessageCreated",
            Self::PartUpdated { .. } => "PartUpdated",
            Self::PermissionAsked { .. } => "PermissionAsked",
            Self::PermissionReplied { .. } => "PermissionReplied",
        }
    }
}

/// Shared state passed to all axum handlers
#[derive(Clone)]
pub struct AppState {
    pub event_tx: broadcast::Sender<ServerEvent>,
    /// Dynamic provider/model catalog built from connected providers.
    pub provider_catalog: Arc<Vec<ProviderInfo>>,
    /// Session store used by handlers that need to read/write sessions.
    /// `None` when the server is started without a store (e.g. tests that only need routing).
    pub session_store: Option<Arc<SessionStore>>,
    /// Permission manager for handling runtime permission checks and approvals.
    pub permission_manager: Arc<PermissionManager>,
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            event_tx,
            provider_catalog: Arc::new(vec![]),
            session_store: None,
            permission_manager: Arc::new(PermissionManager::new()),
        }
    }

    /// Create an `AppState` pre-populated with a provider catalog.
    #[cfg(test)]
    #[must_use]
    pub fn with_catalog(catalog: Vec<ProviderInfo>) -> Self {
        Self {
            provider_catalog: Arc::new(catalog),
            ..Self::new()
        }
    }

    /// Create an `AppState` pre-populated with a session store.
    #[cfg(test)]
    #[must_use]
    pub fn with_store(store: SessionStore) -> Self {
        Self {
            session_store: Some(Arc::new(store)),
            ..Self::new()
        }
    }

    /// Create an `AppState` with both a session store and a provider catalog.
    #[must_use]
    pub fn with_store_and_catalog(store: SessionStore, catalog: Vec<ProviderInfo>) -> Self {
        Self {
            session_store: Some(Arc::new(store)),
            provider_catalog: Arc::new(catalog),
            ..Self::new()
        }
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
    fn server_event_serializes_as_tagged_json() -> Result<(), serde_json::Error> {
        let event = ServerEvent::SessionUpdated {
            session_id: "abc".to_owned(),
        };
        let json = serde_json::to_string(&event)?;
        assert!(json.contains("\"type\":\"session_updated\""));
        assert!(json.contains("\"session_id\":\"abc\""));
        Ok(())
    }
}
