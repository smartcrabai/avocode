use tokio::sync::broadcast;

use crate::event::AppEvent;

const BUS_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
}

impl EventBus {
    #[must_use]
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        Self { tx }
    }

    /// Publish an event to all subscribers. Send errors are silently ignored (no subscribers is fine).
    pub fn publish(&self, event: AppEvent) {
        let _ = self.tx.send(event);
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }

    #[must_use]
    pub fn sender(&self) -> broadcast::Sender<AppEvent> {
        self.tx.clone()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SessionId;

    #[tokio::test]
    async fn test_publish_subscribe() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        let session_id = SessionId::new();
        bus.publish(AppEvent::SessionCreated {
            session_id: session_id.clone(),
        });

        let received = rx.try_recv();
        assert!(received.is_ok());
    }

    #[test]
    fn test_no_subscribers_ok() {
        let bus = EventBus::new();
        // Publishing with no subscribers should not panic
        bus.publish(AppEvent::Done {
            session_id: SessionId::new(),
        });
    }
}
