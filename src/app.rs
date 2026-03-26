use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::event::bus::EventBus;
use crate::types::{ProjectId, SessionId};

/// Global application context shared across components
#[derive(Clone)]
pub struct AppContext {
    inner: Arc<AppContextInner>,
}

struct AppContextInner {
    project_id: ProjectId,
    project_root: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    event_bus: EventBus,
    state: RwLock<AppState>,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub active_session_id: Option<SessionId>,
    pub is_processing: bool,
    pub current_model: Option<String>,
}

impl AppContext {
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        let config_dir = dirs_next::config_dir()
            .unwrap_or_else(|| project_root.join(".config"))
            .join("avocode");
        let data_dir = dirs_next::data_dir()
            .unwrap_or_else(|| project_root.join(".local/share"))
            .join("avocode");
        Self {
            inner: Arc::new(AppContextInner {
                project_id: ProjectId::new(),
                project_root,
                config_dir,
                data_dir,
                event_bus: EventBus::new(),
                state: RwLock::new(AppState::default()),
            }),
        }
    }

    #[must_use]
    pub fn project_root(&self) -> &PathBuf {
        &self.inner.project_root
    }

    #[must_use]
    pub fn config_dir(&self) -> &PathBuf {
        &self.inner.config_dir
    }

    #[must_use]
    pub fn data_dir(&self) -> &PathBuf {
        &self.inner.data_dir
    }

    #[must_use]
    pub fn event_bus(&self) -> &EventBus {
        &self.inner.event_bus
    }

    #[must_use]
    pub fn project_id(&self) -> &ProjectId {
        &self.inner.project_id
    }

    pub async fn set_active_session(&self, session_id: SessionId) {
        let mut state = self.inner.state.write().await;
        state.active_session_id = Some(session_id);
    }

    pub async fn active_session(&self) -> Option<SessionId> {
        self.inner.state.read().await.active_session_id.clone()
    }

    pub async fn set_processing(&self, processing: bool) {
        let mut state = self.inner.state.write().await;
        state.is_processing = processing;
    }

    pub async fn is_processing(&self) -> bool {
        self.inner.state.read().await.is_processing
    }

    pub async fn set_model(&self, model: String) {
        let mut state = self.inner.state.write().await;
        state.current_model = Some(model);
    }

    pub async fn current_model(&self) -> Option<String> {
        self.inner.state.read().await.current_model.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_app_context_creation() {
        let ctx = AppContext::new(PathBuf::from("/tmp"));
        assert!(ctx.project_root() == &PathBuf::from("/tmp"));
        assert!(!ctx.is_processing().await);
    }

    #[tokio::test]
    async fn test_set_processing() {
        let ctx = AppContext::new(PathBuf::from("/tmp"));
        ctx.set_processing(true).await;
        assert!(ctx.is_processing().await);
        ctx.set_processing(false).await;
        assert!(!ctx.is_processing().await);
    }
}
