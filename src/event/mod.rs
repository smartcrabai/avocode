pub mod bus;

use serde::{Deserialize, Serialize};

use crate::types::{MessageId, SessionId, ToolCallId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    SessionCreated {
        session_id: SessionId,
    },
    /// Title changed or metadata updated
    SessionUpdated {
        session_id: SessionId,
    },
    SessionDeleted {
        session_id: SessionId,
    },

    MessageAdded {
        session_id: SessionId,
        message_id: MessageId,
    },
    /// Incremental streaming chunk
    MessageStreamed {
        session_id: SessionId,
        message_id: MessageId,
        delta: String,
    },
    MessageCompleted {
        session_id: SessionId,
        message_id: MessageId,
    },

    ToolStarted {
        session_id: SessionId,
        tool_call_id: ToolCallId,
        tool_name: String,
    },
    ToolCompleted {
        session_id: SessionId,
        tool_call_id: ToolCallId,
        success: bool,
    },

    /// Requires UI prompt before the tool may proceed
    PermissionRequested {
        session_id: SessionId,
        tool_name: String,
        request_id: String,
    },
    PermissionResponded {
        request_id: String,
        granted: bool,
    },

    Error {
        session_id: Option<SessionId>,
        message: String,
    },

    Thinking {
        session_id: SessionId,
    },
    Done {
        session_id: SessionId,
    },
}
