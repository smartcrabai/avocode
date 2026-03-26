use crate::storage::StorageError;

const MIGRATIONS: &[&str] = &[
    // Migration 1: initial schema
    r"
    CREATE TABLE IF NOT EXISTS project (
        id           TEXT PRIMARY KEY NOT NULL,
        name         TEXT NOT NULL,
        worktree     TEXT NOT NULL DEFAULT '[]',
        time_created INTEGER NOT NULL,
        time_updated INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS session (
        id                  TEXT PRIMARY KEY NOT NULL,
        project_id          TEXT NOT NULL,
        slug                TEXT NOT NULL,
        directory           TEXT NOT NULL,
        title               TEXT,
        version             INTEGER NOT NULL DEFAULT 0,
        share_url           TEXT,
        summary_additions   INTEGER,
        summary_deletions   INTEGER,
        summary_files       TEXT,
        summary_diffs       TEXT,
        permission          TEXT,
        parent_id           TEXT,
        time_created        INTEGER NOT NULL,
        time_updated        INTEGER NOT NULL,
        time_compacting     INTEGER,
        time_archived       INTEGER
    );

    CREATE TABLE IF NOT EXISTS message (
        id           TEXT PRIMARY KEY NOT NULL,
        session_id   TEXT NOT NULL,
        role         TEXT NOT NULL,
        time_created INTEGER NOT NULL,
        time_updated INTEGER NOT NULL,
        metadata     TEXT
    );

    CREATE TABLE IF NOT EXISTS part (
        id           TEXT PRIMARY KEY NOT NULL,
        message_id   TEXT NOT NULL,
        session_id   TEXT NOT NULL,
        type         TEXT NOT NULL,
        data         TEXT,
        time_created INTEGER NOT NULL,
        time_updated INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS todo (
        session_id   TEXT NOT NULL,
        content      TEXT NOT NULL,
        status       TEXT NOT NULL,
        priority     INTEGER NOT NULL,
        position     INTEGER NOT NULL,
        time_created INTEGER NOT NULL,
        time_updated INTEGER NOT NULL,
        UNIQUE(session_id, position)
    );

    CREATE TABLE IF NOT EXISTS permission (
        id           TEXT PRIMARY KEY NOT NULL,
        project_id   TEXT NOT NULL,
        data         TEXT,
        time_created INTEGER NOT NULL,
        time_updated INTEGER NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_session_project ON session(project_id);
    CREATE INDEX IF NOT EXISTS idx_message_session ON message(session_id);
    CREATE INDEX IF NOT EXISTS idx_part_message    ON part(message_id);
    CREATE INDEX IF NOT EXISTS idx_part_session    ON part(session_id);
    ",
];

/// Run all pending migrations against the given connection.
///
/// # Errors
/// Returns [`StorageError`] if the database cannot be queried or a migration fails.
pub fn run(conn: &rusqlite::Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            applied_at  INTEGER NOT NULL
        );",
    )?;

    let applied: i64 = conn.query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))?;

    for (i, migration) in MIGRATIONS.iter().enumerate() {
        if i64::try_from(i).is_ok_and(|n| n < applied) {
            continue;
        }
        conn.execute_batch(migration)?;
        conn.execute(
            "INSERT INTO _migrations (applied_at) VALUES (?1)",
            rusqlite::params![chrono::Utc::now().timestamp_millis()],
        )?;
    }

    Ok(())
}
