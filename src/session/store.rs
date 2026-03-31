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
             CREATE INDEX IF NOT EXISTS idx_msg_session ON messages(session_id);
             CREATE INDEX IF NOT EXISTS idx_sessions_directory
               ON sessions(json_extract(data, '$.directory'));",
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

    /// Return the most recent non-`None` `config_ref` among all sessions whose
    /// `directory` exactly matches the given value, or `None` if no such session exists.
    ///
    /// # Errors
    /// Returns [`super::SessionError`] on `SQLite` or deserialization failure.
    pub fn latest_config_for_directory(
        &self,
        directory: &str,
    ) -> Result<Option<String>, super::SessionError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT json_extract(data, '$.config_ref') \
                 FROM sessions \
                 WHERE json_extract(data, '$.directory') = ?1 \
                   AND json_extract(data, '$.config_ref') IS NOT NULL \
                 ORDER BY json_extract(data, '$.time_updated') DESC \
                 LIMIT 1",
            )
            .map_err(super::SessionError::Sqlite)?;
        let mut rows = stmt
            .query(params![directory])
            .map_err(super::SessionError::Sqlite)?;
        if let Some(row) = rows.next().map_err(super::SessionError::Sqlite)? {
            let config_ref: String = row.get(0).map_err(super::SessionError::Sqlite)?;
            return Ok(Some(config_ref));
        }
        Ok(None)
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
    fn create_and_get_session_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/home/user".to_owned());
        let id = session.id.clone();
        store.create_session(&session)?;
        let got = store.get_session(&id)?.ok_or("session not found")?;
        assert_eq!(got.id, id);
        assert_eq!(got.project_id, "proj-1");
        assert_eq!(got.directory, "/home/user");

        Ok(())
    }

    #[test]
    fn get_session_returns_none_for_unknown_id() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let result = store.get_session("nonexistent")?;
        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn list_sessions_filters_by_project() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let s1 = Session::new("proj-a".to_owned(), "/dir1".to_owned());
        let s2 = Session::new("proj-b".to_owned(), "/dir2".to_owned());
        store.create_session(&s1)?;
        store.create_session(&s2)?;

        let list_a = store.list_sessions("proj-a")?;
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_a[0].project_id, "proj-a");

        Ok(())
    }

    #[test]
    fn update_session_title_persists() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        let id = session.id.clone();
        store.create_session(&session)?;
        store.update_session_title(&id, "My Session")?;
        let got = store.get_session(&id)?.ok_or("session not found")?;
        assert_eq!(got.title, Some("My Session".to_owned()));

        Ok(())
    }

    #[test]
    fn archive_session_sets_time_archived() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        let id = session.id.clone();
        store.create_session(&session)?;
        store.archive_session(&id)?;
        let got = store.get_session(&id)?.ok_or("session not found")?;
        assert!(got.time_archived.is_some());

        Ok(())
    }

    #[test]
    fn add_and_list_messages_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        store.create_session(&session)?;

        let msg1 = Message::user(session.id.clone(), "hello");
        let msg2 = Message::user(session.id.clone(), "world");
        store.add_message(&msg1)?;
        store.add_message(&msg2)?;

        let messages = store.list_messages(&session.id)?;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, msg1.id);
        assert_eq!(messages[1].id, msg2.id);

        Ok(())
    }

    #[test]
    fn config_ref_persists_and_restores() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let mut session = Session::new("proj-1".to_owned(), "/home/user".to_owned());
        session.config_ref = Some("~/.config/avocode/myconfig.toml".to_owned());
        let id = session.id.clone();
        store.create_session(&session)?;
        let got = store.get_session(&id)?.ok_or("session not found")?;
        assert_eq!(
            got.config_ref,
            Some("~/.config/avocode/myconfig.toml".to_owned())
        );
        Ok(())
    }

    #[test]
    fn latest_config_for_directory_returns_most_recent_config_ref()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let mut s1 = Session::new("proj-1".to_owned(), "/workspace/proj".to_owned());
        s1.config_ref = Some("config-v1".to_owned());
        s1.time_created = 1_000;
        s1.time_updated = 1_000;
        store.create_session(&s1)?;

        let mut s2 = Session::new("proj-1".to_owned(), "/workspace/proj".to_owned());
        s2.config_ref = Some("config-v2".to_owned());
        s2.time_created = 2_000;
        s2.time_updated = 2_000;
        store.create_session(&s2)?;

        let result = store.latest_config_for_directory("/workspace/proj")?;
        assert_eq!(result, Some("config-v2".to_owned()));
        Ok(())
    }

    #[test]
    fn latest_config_for_directory_does_not_pick_other_directories()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let mut s = Session::new("proj-1".to_owned(), "/workspace/other".to_owned());
        s.config_ref = Some("other-config".to_owned());
        store.create_session(&s)?;

        let result = store.latest_config_for_directory("/workspace/proj")?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn latest_config_for_directory_returns_none_for_unknown_directory()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let result = store.latest_config_for_directory("/nonexistent")?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn latest_config_for_directory_returns_none_when_all_sessions_have_no_config_ref()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let s = Session::new("proj-1".to_owned(), "/workspace/proj".to_owned());
        store.create_session(&s)?;
        let result = store.latest_config_for_directory("/workspace/proj")?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn latest_config_for_directory_skips_sessions_with_no_config_ref()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let mut s1 = Session::new("proj-1".to_owned(), "/workspace/proj".to_owned());
        s1.config_ref = Some("config-v1".to_owned());
        s1.time_created = 1_000;
        s1.time_updated = 1_000;
        store.create_session(&s1)?;

        let mut s2 = Session::new("proj-1".to_owned(), "/workspace/proj".to_owned());
        s2.time_created = 2_000;
        s2.time_updated = 2_000;
        store.create_session(&s2)?;

        let result = store.latest_config_for_directory("/workspace/proj")?;
        assert_eq!(result, Some("config-v1".to_owned()));
        Ok(())
    }

    #[test]
    fn update_message_persists_changes() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::open_in_memory()?;
        let session = Session::new("proj-1".to_owned(), "/dir".to_owned());
        store.create_session(&session)?;

        let mut msg = Message::assistant(session.id.clone());
        store.add_message(&msg)?;

        msg.parts
            .push(crate::session::message::Part::text("response text"));
        msg.time_updated = crate::session::schema::now_ms();
        store.update_message(&msg)?;

        let messages = store.list_messages(&session.id)?;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].parts.len(), 1);

        Ok(())
    }
}
