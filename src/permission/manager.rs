use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use super::PermissionError;
use super::evaluate::evaluate;
use super::schema::{PermissionAction, PermissionReply, PermissionRequest, PermissionRule};

struct PendingRequest {
    request: PermissionRequest,
    /// Copies of the fields needed to build a session rule on `Always` reply.
    permission: String,
    pattern: String,
    reply_tx: oneshot::Sender<PermissionReply>,
}

/// Manages runtime permission checks, including session-scoped rules and
/// pending user-approval requests.
pub struct PermissionManager {
    session_rules: Arc<Mutex<Vec<PermissionRule>>>,
    pending: Arc<Mutex<HashMap<String, PendingRequest>>>,
}

impl PermissionManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            session_rules: Arc::new(Mutex::new(Vec::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check permission for `request` against the provided rule stacks.
    ///
    /// Rule evaluation order (last-match-wins): config -> agent -> session.
    /// Session rules have the highest priority (applied last).
    /// Blocks asynchronously when action is `Ask` until a reply is submitted.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Internal`] if an internal mutex is poisoned.
    /// Returns [`PermissionError::ChannelClosed`] if the pending reply channel is dropped.
    pub async fn check(
        &self,
        request: PermissionRequest,
        config_rules: &[PermissionRule],
        agent_rules: &[PermissionRule],
    ) -> Result<bool, PermissionError> {
        let action = {
            let session_rules = self
                .session_rules
                .lock()
                .map_err(|_| PermissionError::Internal("mutex poisoned".into()))?;
            evaluate(
                &request.permission,
                &request.pattern,
                &[config_rules, agent_rules, &session_rules],
            )
        };

        match action {
            PermissionAction::Allow => Ok(true),
            PermissionAction::Deny => Ok(false),
            PermissionAction::Ask => {
                let (tx, rx) = oneshot::channel();
                {
                    let mut pending = self
                        .pending
                        .lock()
                        .map_err(|_| PermissionError::Internal("mutex poisoned".into()))?;
                    pending.insert(
                        request.id.clone(),
                        PendingRequest {
                            permission: request.permission.clone(),
                            pattern: request.pattern.clone(),
                            request,
                            reply_tx: tx,
                        },
                    );
                }
                let reply = rx.await.map_err(|_| PermissionError::ChannelClosed)?;
                match reply {
                    PermissionReply::Once => Ok(true),
                    PermissionReply::Always => {
                        // The reply is delivered via reply(), which has the PendingRequest.
                        // Session rule insertion is handled there for the Always case.
                        Ok(true)
                    }
                    PermissionReply::Deny => Ok(false),
                }
            }
        }
    }

    /// Submit a reply for a pending permission request identified by `request_id`.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Internal`] if an internal mutex is poisoned.
    /// Returns [`PermissionError::NotFound`] if no pending request matches `request_id`.
    pub fn reply(&self, request_id: &str, reply: PermissionReply) -> Result<(), PermissionError> {
        let pr = {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| PermissionError::Internal("mutex poisoned".into()))?;
            pending
                .remove(request_id)
                .ok_or_else(|| PermissionError::NotFound(request_id.to_owned()))?
        };
        if reply == PermissionReply::Always {
            self.add_session_rule(PermissionRule {
                permission: pr.permission.clone(),
                pattern: pr.pattern.clone(),
                action: PermissionAction::Allow,
            });
        }
        let _ = pr.reply_tx.send(reply);
        Ok(())
    }

    /// Returns all currently pending permission requests.
    #[must_use]
    pub fn pending_requests(&self) -> Vec<PermissionRequest> {
        self.pending
            .lock()
            .map(|p| p.values().map(|pr| pr.request.clone()).collect())
            .unwrap_or_default()
    }

    /// Add a rule that applies for the lifetime of this session.
    pub fn add_session_rule(&self, rule: PermissionRule) {
        if let Ok(mut rules) = self.session_rules.lock() {
            rules.push(rule);
        }
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::permission::schema::PermissionAction;

    fn make_request(permission: &str, pattern: &str) -> PermissionRequest {
        PermissionRequest {
            id: uuid::Uuid::now_v7().to_string(),
            session_id: "test-session".into(),
            permission: permission.into(),
            pattern: pattern.into(),
            metadata: Value::Null,
            always_patterns: vec![],
        }
    }

    #[tokio::test]
    async fn allow_rule_returns_true() {
        let manager = PermissionManager::new();
        let rules = vec![PermissionRule {
            permission: "read".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        }];
        let req = make_request("read", "foo.txt");
        let result = manager.check(req, &rules, &[]).await;
        assert!(matches!(result, Ok(true)));
    }

    #[tokio::test]
    async fn deny_rule_returns_false() {
        let manager = PermissionManager::new();
        let rules = vec![PermissionRule {
            permission: "write".into(),
            pattern: "*".into(),
            action: PermissionAction::Deny,
        }];
        let req = make_request("write", "foo.txt");
        let result = manager.check(req, &rules, &[]).await;
        assert!(matches!(result, Ok(false)));
    }

    #[tokio::test]
    async fn ask_and_reply_once_returns_true() -> Result<(), Box<dyn std::error::Error>> {
        let manager = Arc::new(PermissionManager::new());
        let rules = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Ask,
        }];
        let req = make_request("bash", "*");
        let req_id = req.id.clone();

        let manager_clone = Arc::clone(&manager);
        tokio::spawn(async move {
            // Small yield to let the check() call register the pending entry
            tokio::task::yield_now().await;
            manager_clone.reply(&req_id, PermissionReply::Once)?;
            Ok::<(), PermissionError>(())
        });

        let result = manager.check(req, &rules, &[]).await;
        assert!(matches!(result, Ok(true)));
        Ok(())
    }

    #[tokio::test]
    async fn ask_and_reply_deny_returns_false() -> Result<(), Box<dyn std::error::Error>> {
        let manager = Arc::new(PermissionManager::new());
        let rules = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Ask,
        }];
        let req = make_request("bash", "*");
        let req_id = req.id.clone();

        let manager_clone = Arc::clone(&manager);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            manager_clone.reply(&req_id, PermissionReply::Deny)?;
            Ok::<(), PermissionError>(())
        });

        let result = manager.check(req, &rules, &[]).await;
        assert!(matches!(result, Ok(false)));
        Ok(())
    }

    #[tokio::test]
    async fn reply_not_found_returns_error() {
        let manager = PermissionManager::new();
        let result = manager.reply("nonexistent-id", PermissionReply::Once);
        assert!(matches!(result, Err(PermissionError::NotFound(_))));
    }

    #[tokio::test]
    async fn ask_and_reply_always_adds_session_rule() -> Result<(), Box<dyn std::error::Error>> {
        let manager = Arc::new(PermissionManager::new());
        let ask_rules = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Ask,
        }];
        let req = make_request("bash", "*");
        let req_id = req.id.clone();

        let manager_clone = Arc::clone(&manager);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            manager_clone.reply(&req_id, PermissionReply::Always)?;
            Ok::<(), PermissionError>(())
        });

        let result = manager.check(req, &ask_rules, &[]).await;
        assert!(matches!(result, Ok(true)));

        // The session rule added by Always should now allow without asking
        let req2 = make_request("bash", "some-script");
        let result2 = manager.check(req2, &ask_rules, &[]).await;
        assert!(matches!(result2, Ok(true)));
        Ok(())
    }

    #[tokio::test]
    async fn session_rule_overrides_config_rule() {
        let manager = PermissionManager::new();
        manager.add_session_rule(PermissionRule {
            permission: "write".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        });
        let config_rules = vec![PermissionRule {
            permission: "write".into(),
            pattern: "*".into(),
            action: PermissionAction::Deny,
        }];
        let req = make_request("write", "foo.txt");
        let result = manager.check(req, &config_rules, &[]).await;
        assert!(matches!(result, Ok(true)));
    }
}
