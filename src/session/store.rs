use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::session::SessionError;
use crate::session::message::{Message, Part};
use crate::session::schema::{Session, now_ms};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    id   TEXT PRIMARY KEY,
    data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    data       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
";

/// Persistent storage for sessions and messages backed by `SQLite`.
pub struct SessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SessionStore {
    /// Open (or create) a `SQLite` database at `path`.
    ///
    /// # Errors
    /// Returns [`SessionError::Sqlite`] if the database cannot be opened or
    /// the schema cannot be initialised.
    pub fn open(path: &Path) -> Result<Self, SessionError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database (useful for testing).
    ///
    /// # Errors
    /// Returns [`SessionError::Sqlite`] if the database cannot be initialised.
    pub fn open_in_memory() -> Result<Self, SessionError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    // -----------------------------------------------------------------------
    // Sessions
    // -----------------------------------------------------------------------

    /// Persist a new session.
    ///
    /// # Errors
    /// Returns a [`SessionError`] on serialisation or database failure.
    pub fn create_session(&self, session: &Session) -> Result<(), SessionError> {
        let data = serde_json::to_string(session)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| SessionError::Other(e.to_string()))?;
        conn.execute(
            "INSERT INTO sessions (id, data) VALUES (?1, ?2)",
            rusqlite::params![session.id, data],
        )?;
        Ok(())
    }

    /// Retrieve a session by its ID.  Returns `None` if not found.
    ///
    /// # Errors
    /// Returns a [`SessionError`] on database or deserialisation failure.
    pub fn get_session(&self, id: &str) -> Result<Option<Session>, SessionError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SessionError::Other(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT data FROM sessions WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => {
                let data: String = row.get(0)?;
                let session = serde_json::from_str(&data)?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// List all sessions belonging to a project.
    ///
    /// # Errors
    /// Returns a [`SessionError`] on database or deserialisation failure.
    pub fn list_sessions(&self, project_id: &str) -> Result<Vec<Session>, SessionError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SessionError::Other(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT data FROM sessions")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut sessions = Vec::new();
        for row in rows {
            let data = row?;
            let session: Session = serde_json::from_str(&data)?;
            if session.project_id == project_id {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }

    /// Update the title of an existing session.
    ///
    /// # Errors
    /// Returns [`SessionError::NotFound`] if the session does not exist, or a
    /// [`SessionError`] on database or serialisation failure.
    pub fn update_session_title(&self, id: &str, title: &str) -> Result<(), SessionError> {
        let session = self
            .get_session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        let updated = Session {
            title: Some(title.to_string()),
            time_updated: now_ms(),
            ..session
        };
        let data = serde_json::to_string(&updated)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| SessionError::Other(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET data = ?1 WHERE id = ?2",
            rusqlite::params![data, id],
        )?;
        Ok(())
    }

    /// Mark a session as archived by setting `time_archived` to now.
    ///
    /// # Errors
    /// Returns [`SessionError::NotFound`] if the session does not exist, or a
    /// [`SessionError`] on database or serialisation failure.
    pub fn archive_session(&self, id: &str) -> Result<(), SessionError> {
        let session = self
            .get_session(id)?
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        let updated = Session {
            time_archived: Some(now_ms()),
            time_updated: now_ms(),
            ..session
        };
        let data = serde_json::to_string(&updated)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| SessionError::Other(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET data = ?1 WHERE id = ?2",
            rusqlite::params![data, id],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Messages
    // -----------------------------------------------------------------------

    /// Persist a new message.
    ///
    /// # Errors
    /// Returns a [`SessionError`] on serialisation or database failure.
    pub fn add_message(&self, message: &Message) -> Result<(), SessionError> {
        let data = serde_json::to_string(message)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| SessionError::Other(e.to_string()))?;
        conn.execute(
            "INSERT INTO messages (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![message.id, message.session_id, data],
        )?;
        Ok(())
    }

    /// Retrieve all messages for a session, ordered by creation time.
    ///
    /// # Errors
    /// Returns a [`SessionError`] on database or deserialisation failure.
    pub fn list_messages(&self, session_id: &str) -> Result<Vec<Message>, SessionError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SessionError::Other(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT data FROM messages WHERE session_id = ?1")?;
        let rows = stmt.query_map(rusqlite::params![session_id], |row| row.get::<_, String>(0))?;
        let mut messages = Vec::new();
        for row in rows {
            let data = row?;
            let msg: Message = serde_json::from_str(&data)?;
            messages.push(msg);
        }
        messages.sort_by_key(|m| m.time_created);
        Ok(messages)
    }

    /// Replace the parts of an existing message.
    ///
    /// # Errors
    /// Returns [`SessionError::NotFound`] if the message does not exist, or a
    /// [`SessionError`] on database or serialisation failure.
    pub fn update_message_parts(
        &self,
        message_id: &str,
        parts: &[Part],
    ) -> Result<(), SessionError> {
        let existing = {
            let conn = self
                .conn
                .lock()
                .map_err(|e| SessionError::Other(e.to_string()))?;
            let mut stmt = conn.prepare("SELECT data FROM messages WHERE id = ?1")?;
            let mut rows = stmt.query(rusqlite::params![message_id])?;
            match rows.next()? {
                Some(row) => {
                    let data: String = row.get(0)?;
                    serde_json::from_str::<Message>(&data)?
                }
                None => return Err(SessionError::NotFound(message_id.to_string())),
            }
        };
        let updated = Message {
            parts: parts.to_vec(),
            time_updated: now_ms(),
            ..existing
        };
        let data = serde_json::to_string(&updated)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| SessionError::Other(e.to_string()))?;
        conn.execute(
            "UPDATE messages SET data = ?1 WHERE id = ?2",
            rusqlite::params![data, message_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::message::{MessageRole, TextPart};
    use crate::session::schema::{new_session_id, now_ms};

    fn make_session(project_id: &str) -> Session {
        Session {
            id: new_session_id(),
            project_id: project_id.to_string(),
            slug: "test-session".into(),
            directory: "/tmp".into(),
            title: Some("Test".into()),
            version: 1,
            share_url: None,
            summary: None,
            permission: vec![],
            parent_id: None,
            time_created: now_ms(),
            time_updated: now_ms(),
            time_compacting: None,
            time_archived: None,
        }
    }

    fn make_message(session_id: &str) -> Message {
        use crate::session::message::new_message_id;
        Message {
            id: new_message_id(),
            session_id: session_id.to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text(TextPart {
                id: "p1".into(),
                text: "hello".into(),
            })],
            time_created: now_ms(),
            time_updated: now_ms(),
        }
    }

    #[test]
    fn create_and_get_session() {
        let store = SessionStore::open_in_memory().expect("open");
        let session = make_session("proj-1");
        store.create_session(&session).expect("create");
        let fetched = store
            .get_session(&session.id)
            .expect("get")
            .expect("present");
        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.project_id, "proj-1");
    }

    #[test]
    fn add_and_list_messages() {
        let store = SessionStore::open_in_memory().expect("open");
        let session = make_session("proj-2");
        store.create_session(&session).expect("create");
        let msg = make_message(&session.id);
        store.add_message(&msg).expect("add");
        let msgs = store.list_messages(&session.id).expect("list");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, msg.id);
    }
}
