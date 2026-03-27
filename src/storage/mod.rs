pub mod db;
pub mod migrations;
pub mod schema;

pub use db::Database;
pub use schema::{MessageRow, PartRow, PermissionRow, ProjectRow, SessionRow, TodoRow};

/// Errors that can occur during storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_project() -> ProjectRow {
        ProjectRow {
            id: "proj-1".to_string(),
            name: "Test Project".to_string(),
            worktree: vec!["/home/user/project".to_string()],
            time_created: 1_000_000,
            time_updated: 1_000_000,
        }
    }

    fn make_session(project_id: &str) -> SessionRow {
        SessionRow {
            id: "sess-1".to_string(),
            project_id: project_id.to_string(),
            slug: "test-session".to_string(),
            directory: "/home/user/project".to_string(),
            title: Some("My Session".to_string()),
            version: 0,
            share_url: None,
            summary_additions: None,
            summary_deletions: None,
            summary_files: None,
            summary_diffs: None,
            permission: None,
            parent_id: None,
            time_created: 2_000_000,
            time_updated: 2_000_000,
            time_compacting: None,
            time_archived: None,
        }
    }

    fn make_message(session_id: &str) -> MessageRow {
        MessageRow {
            id: "msg-1".to_string(),
            session_id: session_id.to_string(),
            role: "user".to_string(),
            time_created: 3_000_000,
            time_updated: 3_000_000,
            metadata: Some(serde_json::json!({"tokens": 42})),
        }
    }

    fn make_part(message_id: &str, session_id: &str) -> PartRow {
        PartRow {
            id: "part-1".to_string(),
            message_id: message_id.to_string(),
            session_id: session_id.to_string(),
            r#type: "text".to_string(),
            data: Some(serde_json::json!({"content": "Hello, world!"})),
            time_created: 4_000_000,
            time_updated: 4_000_000,
        }
    }

    /// Opening an in-memory database and running migrations must not fail.
    #[test]
    fn test_open_in_memory_runs_migrations() -> Result<(), Box<dyn std::error::Error>> {
        Database::open_in_memory()?;
        Ok(())
    }

    /// Insert a session and retrieve it by ID.
    #[test]
    fn test_session_insert_and_get() -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::open_in_memory()?;
        db.project_insert(&make_project())?;
        let session = make_session("proj-1");
        db.session_insert(&session)?;

        let got = db.session_get("sess-1")?;
        assert!(got.is_some());
        let got = got.ok_or("session not found")?;
        assert_eq!(got.id, "sess-1");
        assert_eq!(got.title, Some("My Session".to_string()));
        assert_eq!(got.project_id, "proj-1");
        Ok(())
    }

    /// Insert messages and list them for the session.
    #[test]
    fn test_message_insert_and_list() -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::open_in_memory()?;
        db.project_insert(&make_project())?;
        db.session_insert(&make_session("proj-1"))?;
        db.message_insert(&make_message("sess-1"))?;

        let msgs = db.message_list("sess-1")?;
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, "msg-1");
        assert_eq!(msgs[0].role, "user");
        // metadata round-trips correctly
        let meta = msgs[0].metadata.as_ref().ok_or("metadata missing")?;
        assert_eq!(meta["tokens"], serde_json::json!(42));
        Ok(())
    }

    /// Insert parts and list them for the session.
    #[test]
    fn test_part_insert_and_list() -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::open_in_memory()?;
        db.project_insert(&make_project())?;
        db.session_insert(&make_session("proj-1"))?;
        db.message_insert(&make_message("sess-1"))?;
        db.part_insert(&make_part("msg-1", "sess-1"))?;

        let parts = db.part_list("sess-1")?;
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].id, "part-1");
        assert_eq!(parts[0].r#type, "text");
        let data = parts[0].data.as_ref().ok_or("part data missing")?;
        assert_eq!(data["content"], serde_json::json!("Hello, world!"));
        Ok(())
    }

    /// Listing parts for a session that has no parts returns an empty vec.
    #[test]
    fn test_part_list_nonexistent_session_returns_empty() -> Result<(), Box<dyn std::error::Error>>
    {
        let db = Database::open_in_memory()?;
        let parts = db.part_list("does-not-exist")?;
        assert!(parts.is_empty());
        Ok(())
    }

    /// Todo upsert inserts on first call and updates on second call for same position.
    #[test]
    fn test_todo_upsert() -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::open_in_memory()?;
        db.project_insert(&make_project())?;
        db.session_insert(&make_session("proj-1"))?;

        let todo = TodoRow {
            session_id: "sess-1".to_string(),
            content: "Write tests".to_string(),
            status: "pending".to_string(),
            priority: 1,
            position: 0,
            time_created: 5_000_000,
            time_updated: 5_000_000,
        };
        db.todo_upsert(&todo)?;

        // Upsert again with a different status at the same position.
        let updated = TodoRow {
            status: "done".to_string(),
            time_updated: 6_000_000,
            ..todo
        };
        db.todo_upsert(&updated)?;

        let todos = db.todo_list("sess-1")?;
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].status, "done");
        Ok(())
    }

    /// Session list returns only sessions for the given project.
    #[test]
    fn test_session_list_by_project() -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::open_in_memory()?;
        db.project_insert(&make_project())?;
        db.project_insert(&ProjectRow {
            id: "proj-2".to_string(),
            name: "Other".to_string(),
            worktree: vec![],
            time_created: 1_000_000,
            time_updated: 1_000_000,
        })?;

        db.session_insert(&make_session("proj-1"))?;
        db.session_insert(&SessionRow {
            id: "sess-2".to_string(),
            project_id: "proj-2".to_string(),
            slug: "other-session".to_string(),
            ..make_session("proj-2")
        })?;

        let list = db.session_list("proj-1")?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "sess-1");
        Ok(())
    }
}
