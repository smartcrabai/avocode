/// Row type for the `project` table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    /// Stored as a JSON array in the database.
    pub worktree: Vec<String>,
    pub time_created: i64,
    pub time_updated: i64,
}

/// Row type for the `session` table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionRow {
    pub id: String,
    pub project_id: String,
    pub slug: String,
    pub directory: String,
    pub title: Option<String>,
    pub version: i64,
    pub share_url: Option<String>,
    pub summary_additions: Option<i64>,
    pub summary_deletions: Option<i64>,
    /// Stored as a JSON array in the database.
    pub summary_files: Option<Vec<String>>,
    /// Stored as a JSON object in the database.
    pub summary_diffs: Option<serde_json::Value>,
    /// Stored as a JSON object in the database.
    pub permission: Option<serde_json::Value>,
    pub parent_id: Option<String>,
    pub time_created: i64,
    pub time_updated: i64,
    pub time_compacting: Option<i64>,
    pub time_archived: Option<i64>,
}

/// Row type for the `message` table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub time_created: i64,
    pub time_updated: i64,
    /// Stored as a JSON object in the database.
    pub metadata: Option<serde_json::Value>,
}

/// Row type for the `part` table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PartRow {
    pub id: String,
    pub message_id: String,
    pub session_id: String,
    pub r#type: String,
    /// Stored as a JSON object in the database.
    pub data: Option<serde_json::Value>,
    pub time_created: i64,
    pub time_updated: i64,
}

/// Row type for the `todo` table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoRow {
    pub session_id: String,
    pub content: String,
    pub status: String,
    pub priority: i64,
    pub position: i64,
    pub time_created: i64,
    pub time_updated: i64,
}

/// Row type for the `permission` table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionRow {
    pub id: String,
    pub project_id: String,
    /// Stored as a JSON object in the database.
    pub data: Option<serde_json::Value>,
    pub time_created: i64,
    pub time_updated: i64,
}
