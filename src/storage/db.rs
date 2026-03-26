use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::storage::StorageError;
use crate::storage::migrations;
use crate::storage::schema::{MessageRow, PartRow, PermissionRow, ProjectRow, SessionRow, TodoRow};

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Thread-safe wrapper around a `SQLite` connection.
pub struct Database {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl Database {
    fn lock(&self) -> Result<MutexGuard<'_, rusqlite::Connection>, StorageError> {
        self.conn
            .lock()
            .map_err(|_| StorageError::Sqlite(rusqlite::Error::InvalidQuery))
    }

    /// Open (or create) a database file at `path` and run migrations.
    ///
    /// # Errors
    /// Returns [`StorageError`] if the file cannot be opened or migrations fail.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let conn = rusqlite::Connection::open(path)?;
        migrations::run(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database and run migrations. Useful for tests.
    ///
    /// # Errors
    /// Returns [`StorageError`] if the connection cannot be opened or migrations fail.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        migrations::run(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    // -------------------------------------------------------------------------
    // Projects
    // -------------------------------------------------------------------------

    /// Insert a new project row.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or serialization failure.
    pub fn project_insert(&self, project: &ProjectRow) -> Result<(), StorageError> {
        let worktree = serde_json::to_string(&project.worktree)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO project (id, name, worktree, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                project.id,
                project.name,
                worktree,
                project.time_created,
                project.time_updated,
            ],
        )?;
        Ok(())
    }

    /// Retrieve a project by its primary key.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or deserialization failure.
    pub fn project_get(&self, id: &str) -> Result<Option<ProjectRow>, StorageError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, worktree, time_created, time_updated FROM project WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(row) = rows.next()? {
            let worktree_json: String = row.get(2)?;
            Ok(Some(ProjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                worktree: serde_json::from_str(&worktree_json)?,
                time_created: row.get(3)?,
                time_updated: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Find a project whose `worktree` JSON array contains `directory`.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or deserialization failure.
    pub fn project_get_by_directory(
        &self,
        directory: &str,
    ) -> Result<Option<ProjectRow>, StorageError> {
        let conn = self.lock()?;
        let mut stmt =
            conn.prepare("SELECT id, name, worktree, time_created, time_updated FROM project")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let worktree_json: String = row.get(2)?;
            let worktree: Vec<String> = serde_json::from_str(&worktree_json)?;
            if worktree.iter().any(|p| p == directory) {
                return Ok(Some(ProjectRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    worktree,
                    time_created: row.get(3)?,
                    time_updated: row.get(4)?,
                }));
            }
        }
        Ok(None)
    }

    // -------------------------------------------------------------------------
    // Sessions
    // -------------------------------------------------------------------------

    /// Insert a new session row.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or serialization failure.
    pub fn session_insert(&self, session: &SessionRow) -> Result<(), StorageError> {
        let summary_files = session
            .summary_files
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let summary_diffs = session
            .summary_diffs
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let permission = session
            .permission
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO session (
                id, project_id, slug, directory, title, version, share_url,
                summary_additions, summary_deletions, summary_files, summary_diffs,
                permission, parent_id,
                time_created, time_updated, time_compacting, time_archived
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11,
                ?12, ?13,
                ?14, ?15, ?16, ?17
             )",
            rusqlite::params![
                session.id,
                session.project_id,
                session.slug,
                session.directory,
                session.title,
                session.version,
                session.share_url,
                session.summary_additions,
                session.summary_deletions,
                summary_files,
                summary_diffs,
                permission,
                session.parent_id,
                session.time_created,
                session.time_updated,
                session.time_compacting,
                session.time_archived,
            ],
        )?;
        Ok(())
    }

    /// Retrieve a session by its primary key.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or deserialization failure.
    pub fn session_get(&self, id: &str) -> Result<Option<SessionRow>, StorageError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, project_id, slug, directory, title, version, share_url,
                    summary_additions, summary_deletions, summary_files, summary_diffs,
                    permission, parent_id,
                    time_created, time_updated, time_compacting, time_archived
             FROM session WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(session_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// List all sessions belonging to a project.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or deserialization failure.
    pub fn session_list(&self, project_id: &str) -> Result<Vec<SessionRow>, StorageError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, project_id, slug, directory, title, version, share_url,
                    summary_additions, summary_deletions, summary_files, summary_diffs,
                    permission, parent_id,
                    time_created, time_updated, time_compacting, time_archived
             FROM session WHERE project_id = ?1 ORDER BY time_created ASC",
        )?;
        let mut rows = stmt.query(rusqlite::params![project_id])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(session_from_row(row)?);
        }
        Ok(result)
    }

    /// Update the title of a session.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database failure.
    pub fn session_update_title(&self, id: &str, title: &str) -> Result<(), StorageError> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE session SET title = ?1, time_updated = ?2 WHERE id = ?3",
            rusqlite::params![title, now_ms(), id],
        )?;
        Ok(())
    }

    /// Set `time_archived` to now for a session.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database failure.
    pub fn session_archive(&self, id: &str) -> Result<(), StorageError> {
        let ts = now_ms();
        let conn = self.lock()?;
        conn.execute(
            "UPDATE session SET time_archived = ?1, time_updated = ?2 WHERE id = ?3",
            rusqlite::params![ts, ts, id],
        )?;
        Ok(())
    }

    /// Delete a session and all its related messages, parts, and todos atomically.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database failure.
    pub fn session_delete(&self, id: &str) -> Result<(), StorageError> {
        let conn = self.lock()?;
        conn.execute_batch("BEGIN;")?;
        let result = (|| -> Result<(), StorageError> {
            conn.execute(
                "DELETE FROM todo    WHERE session_id = ?1",
                rusqlite::params![id],
            )?;
            conn.execute(
                "DELETE FROM part    WHERE session_id = ?1",
                rusqlite::params![id],
            )?;
            conn.execute(
                "DELETE FROM message WHERE session_id = ?1",
                rusqlite::params![id],
            )?;
            conn.execute("DELETE FROM session WHERE id = ?1", rusqlite::params![id])?;
            Ok(())
        })();
        if result.is_ok() {
            conn.execute_batch("COMMIT;")?;
        } else {
            conn.execute_batch("ROLLBACK;").ok();
        }
        result
    }

    // -------------------------------------------------------------------------
    // Messages
    // -------------------------------------------------------------------------

    /// Insert a new message row.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or serialization failure.
    pub fn message_insert(&self, message: &MessageRow) -> Result<(), StorageError> {
        let metadata = message
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO message (id, session_id, role, time_created, time_updated, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                message.id,
                message.session_id,
                message.role,
                message.time_created,
                message.time_updated,
                metadata,
            ],
        )?;
        Ok(())
    }

    /// List all messages for a session, ordered by creation time.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or deserialization failure.
    pub fn message_list(&self, session_id: &str) -> Result<Vec<MessageRow>, StorageError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, time_created, time_updated, metadata
             FROM message WHERE session_id = ?1 ORDER BY time_created ASC",
        )?;
        let mut rows = stmt.query(rusqlite::params![session_id])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let metadata_json: Option<String> = row.get(5)?;
            result.push(MessageRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                time_created: row.get(3)?,
                time_updated: row.get(4)?,
                metadata: metadata_json
                    .map(|s| serde_json::from_str(&s))
                    .transpose()?,
            });
        }
        Ok(result)
    }

    // -------------------------------------------------------------------------
    // Parts
    // -------------------------------------------------------------------------

    /// Insert a new part row.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or serialization failure.
    pub fn part_insert(&self, part: &PartRow) -> Result<(), StorageError> {
        let data = part.data.as_ref().map(serde_json::to_string).transpose()?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, type, data, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                part.id,
                part.message_id,
                part.session_id,
                part.r#type,
                data,
                part.time_created,
                part.time_updated,
            ],
        )?;
        Ok(())
    }

    /// Update an existing part row (replaces data and updates `time_updated`).
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or serialization failure.
    pub fn part_update(&self, part: &PartRow) -> Result<(), StorageError> {
        let data = part.data.as_ref().map(serde_json::to_string).transpose()?;
        let conn = self.lock()?;
        conn.execute(
            "UPDATE part SET data = ?1, time_updated = ?2 WHERE id = ?3",
            rusqlite::params![data, now_ms(), part.id],
        )?;
        Ok(())
    }

    /// List all parts for a session, ordered by creation time.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or deserialization failure.
    pub fn part_list(&self, session_id: &str) -> Result<Vec<PartRow>, StorageError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, message_id, session_id, type, data, time_created, time_updated
             FROM part WHERE session_id = ?1 ORDER BY time_created ASC",
        )?;
        let mut rows = stmt.query(rusqlite::params![session_id])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let data_json: Option<String> = row.get(4)?;
            result.push(PartRow {
                id: row.get(0)?,
                message_id: row.get(1)?,
                session_id: row.get(2)?,
                r#type: row.get(3)?,
                data: data_json.map(|s| serde_json::from_str(&s)).transpose()?,
                time_created: row.get(5)?,
                time_updated: row.get(6)?,
            });
        }
        Ok(result)
    }

    // -------------------------------------------------------------------------
    // Todos
    // -------------------------------------------------------------------------

    /// Insert or replace a todo row (upsert by `session_id` + position).
    ///
    /// # Errors
    /// Returns [`StorageError`] on database failure.
    pub fn todo_upsert(&self, todo: &TodoRow) -> Result<(), StorageError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO todo (session_id, content, status, priority, position, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(session_id, position) DO UPDATE SET
                content      = excluded.content,
                status       = excluded.status,
                priority     = excluded.priority,
                time_updated = excluded.time_updated",
            rusqlite::params![
                todo.session_id,
                todo.content,
                todo.status,
                todo.priority,
                todo.position,
                todo.time_created,
                todo.time_updated,
            ],
        )?;
        Ok(())
    }

    /// List all todos for a session, ordered by position.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database failure.
    pub fn todo_list(&self, session_id: &str) -> Result<Vec<TodoRow>, StorageError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, content, status, priority, position, time_created, time_updated
             FROM todo WHERE session_id = ?1 ORDER BY position ASC",
        )?;
        let mut rows = stmt.query(rusqlite::params![session_id])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(TodoRow {
                session_id: row.get(0)?,
                content: row.get(1)?,
                status: row.get(2)?,
                priority: row.get(3)?,
                position: row.get(4)?,
                time_created: row.get(5)?,
                time_updated: row.get(6)?,
            });
        }
        Ok(result)
    }

    // -------------------------------------------------------------------------
    // Permissions
    // -------------------------------------------------------------------------

    /// Insert a new permission row.
    ///
    /// # Errors
    /// Returns [`StorageError`] on database or serialization failure.
    pub fn permission_insert(&self, perm: &PermissionRow) -> Result<(), StorageError> {
        let data = perm.data.as_ref().map(serde_json::to_string).transpose()?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO permission (id, project_id, data, time_created, time_updated)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                perm.id,
                perm.project_id,
                data,
                perm.time_created,
                perm.time_updated,
            ],
        )?;
        Ok(())
    }
}

// -------------------------------------------------------------------------
// Helper: deserialise a session from a rusqlite Row
// -------------------------------------------------------------------------

fn session_from_row(row: &rusqlite::Row<'_>) -> Result<SessionRow, StorageError> {
    let summary_files_json: Option<String> = row.get(9)?;
    let summary_diffs_json: Option<String> = row.get(10)?;
    let permission_json: Option<String> = row.get(11)?;

    Ok(SessionRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        slug: row.get(2)?,
        directory: row.get(3)?,
        title: row.get(4)?,
        version: row.get(5)?,
        share_url: row.get(6)?,
        summary_additions: row.get(7)?,
        summary_deletions: row.get(8)?,
        summary_files: summary_files_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        summary_diffs: summary_diffs_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        permission: permission_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        parent_id: row.get(12)?,
        time_created: row.get(13)?,
        time_updated: row.get(14)?,
        time_compacting: row.get(15)?,
        time_archived: row.get(16)?,
    })
}
