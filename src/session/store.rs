use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};

use super::{message::Message, schema::Session};

pub struct SessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SessionStore {
    /// # Errors
    /// Returns [`super::SessionError::Sqlite`] if the database cannot be opened or migrated.
    pub fn open(path: &std::path::Path) -> Result<Self, super::SessionError> {
        let conn = Connection::open(path).map_err(super::SessionError::Sqlite)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    /// # Errors
    /// Returns [`super::SessionError::Sqlite`] if the in-memory database cannot be created or migrated.
    pub fn open_in_memory() -> Result<Self, super::SessionError> {
        let conn = Connection::open_in_memory().map_err(super::SessionError::Sqlite)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), super::SessionError> {
        let conn = self.lock()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS messages (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 data TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_msg_session ON messages(session_id);",
        )
        .map_err(super::SessionError::Sqlite)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, super::SessionError> {
        self.conn
            .lock()
            .map_err(|_| super::SessionError::Other("mutex poisoned".into()))
    }

    /// # Errors
    /// Returns [`super::SessionError`] on serialization or `SQLite` failure.
    pub fn create_session(&self, session: &Session) -> Result<(), super::SessionError> {
        let data = serde_json::to_string(session).map_err(super::SessionError::Serde)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO sessions (id, data) VALUES (?1, ?2)",
            params![session.id, data],
        )
        .map_err(super::SessionError::Sqlite)?;
        Ok(())
    }

    /// # Errors
    /// Returns [`super::SessionError`] on `SQLite` or deserialization failure.
    pub fn get_session(&self, id: &str) -> Result<Option<Session>, super::SessionError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT data FROM sessions WHERE id = ?1")
            .map_err(super::SessionError::Sqlite)?;
        let mut rows = stmt
            .query(params![id])
            .map_err(super::SessionError::Sqlite)?;
        if let Some(row) = rows.next().map_err(super::SessionError::Sqlite)? {
            let data: String = row.get(0).map_err(super::SessionError::Sqlite)?;
            let session = serde_json::from_str(&data).map_err(super::SessionError::Serde)?;
            return Ok(Some(session));
        }
        Ok(None)
    }

    /// # Errors
    /// Returns [`super::SessionError`] on `SQLite` failure.
    pub fn list_sessions(&self, project_id: &str) -> Result<Vec<Session>, super::SessionError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT data FROM sessions")
            .map_err(super::SessionError::Sqlite)?;
        let sessions = stmt
            .query_map([], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })
            .map_err(super::SessionError::Sqlite)?
            .filter_map(Result::ok)
            .filter_map(|data| serde_json::from_str::<Session>(&data).ok())
            .filter(|s| s.project_id == project_id)
            .collect();
        Ok(sessions)
    }

    /// # Errors
    /// Returns [`super::SessionError::NotFound`] if no session has that id, or [`super::SessionError`] on `SQLite`/serde failure.
    pub fn update_session_title(&self, id: &str, title: &str) -> Result<(), super::SessionError> {
        let mut session = self
            .get_session(id)?
            .ok_or_else(|| super::SessionError::NotFound(id.to_owned()))?;
        session.title = Some(title.to_owned());
        session.time_updated = super::schema::now_ms();
        let data = serde_json::to_string(&session).map_err(super::SessionError::Serde)?;
        let conn = self.lock()?;
        conn.execute(
            "UPDATE sessions SET data = ?1 WHERE id = ?2",
            params![data, id],
        )
        .map_err(super::SessionError::Sqlite)?;
        Ok(())
    }

    /// # Errors
    /// Returns [`super::SessionError::NotFound`] if no session has that id, or [`super::SessionError`] on `SQLite`/serde failure.
    pub fn archive_session(&self, id: &str) -> Result<(), super::SessionError> {
        let mut session = self
            .get_session(id)?
            .ok_or_else(|| super::SessionError::NotFound(id.to_owned()))?;
        session.time_archived = Some(super::schema::now_ms());
        let data = serde_json::to_string(&session).map_err(super::SessionError::Serde)?;
        let conn = self.lock()?;
        conn.execute(
            "UPDATE sessions SET data = ?1 WHERE id = ?2",
            params![data, id],
        )
        .map_err(super::SessionError::Sqlite)?;
        Ok(())
    }

    /// # Errors
    /// Returns [`super::SessionError`] on serialization or `SQLite` failure.
    pub fn add_message(&self, message: &Message) -> Result<(), super::SessionError> {
        let data = serde_json::to_string(message).map_err(super::SessionError::Serde)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR REPLACE INTO messages (id, session_id, data) VALUES (?1, ?2, ?3)",
            params![message.id, message.session_id, data],
        )
        .map_err(super::SessionError::Sqlite)?;
        Ok(())
    }

    /// # Errors
    /// Returns [`super::SessionError`] on `SQLite` failure.
    pub fn list_messages(&self, session_id: &str) -> Result<Vec<Message>, super::SessionError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT data FROM messages WHERE session_id = ?1 ORDER BY rowid")
            .map_err(super::SessionError::Sqlite)?;
        let messages = stmt
            .query_map(params![session_id], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })
            .map_err(super::SessionError::Sqlite)?
            .filter_map(Result::ok)
            .filter_map(|data| serde_json::from_str::<Message>(&data).ok())
            .collect();
        Ok(messages)
    }

    /// # Errors
    /// Returns [`super::SessionError`] on serialization or `SQLite` failure.
    pub fn update_message(&self, message: &Message) -> Result<(), super::SessionError> {
        let data = serde_json::to_string(message).map_err(super::SessionError::Serde)?;
        let conn = self.lock()?;
        conn.execute(
            "UPDATE messages SET data = ?1 WHERE id = ?2",
            params![data, message.id],
        )
        .map_err(super::SessionError::Sqlite)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{message::Message, schema::Session};

    #[test]
    fn open_in_memory_succeeds() {
        let store = SessionStore::open_in_memory();
        assert!(store.is_ok());
    }

    #[test]
    fn create_and_get_session_roundtrip() {
        let store = SessionStore::open_in_memory().expect("store");
        let session = Session::new("proj-1".to_owned(), "/home/user".to_owned());
        let id = session.id.clone();
        store.create_session(&session).expect("create");
        let got = store.get_session(&id).expect("get").expect("some");
        assert_eq!(got.id, id);
        assert_eq!(got.project_id, "proj-1");
        assert_eq!(got.directory, "/home/user");
    }

    #[test]
    fn get_session_returns_none_for_unknown_id() {
        let store = SessionStore::open_in_memory().expect("store");
        let result = store.get_session("nonexistent").expect("get");
        assert!(result.is_none());
    }

    #[test]
    fn list_sessions_filters_by_project() {
        let store = SessionStore::open_in_memory().expect("store");
        let s1 = Session::new("proj-a".to_owned(), "/dir1".to_owned());
        let s2 = Session::new("proj-b".to_owned(), "/dir2".to_owned());
        store.create_session(&s1).expect("create s1");
        store.create_session(&s2).expect("create s2");

        let list_a = store.list_sessions("proj-a").expect("list a");
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_a[0].project_id, "proj-a");
    }

    #[test]
    fn update_session_title_persists() {
        let store = SessionStore::open_in_memory().expect("store");
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        let id = session.id.clone();
        store.create_session(&session).expect("create");
        store
            .update_session_title(&id, "My Session")
            .expect("update title");
        let got = store.get_session(&id).expect("get").expect("some");
        assert_eq!(got.title, Some("My Session".to_owned()));
    }

    #[test]
    fn archive_session_sets_time_archived() {
        let store = SessionStore::open_in_memory().expect("store");
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        let id = session.id.clone();
        store.create_session(&session).expect("create");
        store.archive_session(&id).expect("archive");
        let got = store.get_session(&id).expect("get").expect("some");
        assert!(got.time_archived.is_some());
    }

    #[test]
    fn add_and_list_messages_roundtrip() {
        let store = SessionStore::open_in_memory().expect("store");
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        store.create_session(&session).expect("create session");

        let msg1 = Message::user(session.id.clone(), "hello");
        let msg2 = Message::user(session.id.clone(), "world");
        store.add_message(&msg1).expect("add msg1");
        store.add_message(&msg2).expect("add msg2");

        let messages = store.list_messages(&session.id).expect("list");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, msg1.id);
        assert_eq!(messages[1].id, msg2.id);
    }

    #[test]
    fn update_message_persists_changes() {
        let store = SessionStore::open_in_memory().expect("store");
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        store.create_session(&session).expect("create session");

        let mut msg = Message::assistant(session.id.clone());
        store.add_message(&msg).expect("add");

        msg.parts
            .push(crate::session::message::Part::text("response text"));
        msg.time_updated = crate::session::schema::now_ms();
        store.update_message(&msg).expect("update");

        let messages = store.list_messages(&session.id).expect("list");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].parts.len(), 1);
    }
}
