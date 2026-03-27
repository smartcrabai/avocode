use serde::{Deserialize, Serialize};
use std::fmt;

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Newtype wrappers for IDs to prevent accidental mixing
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolCallId(pub String);

impl SessionId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid_v4())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl MessageId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid_v4())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ProjectId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid_v4())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ToolCallId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid_v4())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for ToolCallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ToolCallId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unix timestamp in milliseconds
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(pub i64);

impl Timestamp {
    #[must_use]
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Self(millis)
    }

    #[must_use]
    pub fn as_millis(&self) -> i64 {
        self.0
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_unique() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_timestamp_now() {
        let ts = Timestamp::now();
        assert!(ts.as_millis() > 0);
    }

    #[test]
    fn test_id_display() {
        let id = SessionId(String::from("test-id"));
        assert_eq!(id.to_string(), "test-id");
    }

    #[test]
    fn test_id_serde() {
        let id = SessionId::new();
        let result = serde_json::to_string(&id);
        assert!(result.is_ok());
        let json = result.unwrap_or_default();
        let result2 = serde_json::from_str::<SessionId>(&json);
        assert!(result2.is_ok());
    }
}
