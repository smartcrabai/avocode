#[must_use]
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[must_use]
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[must_use]
pub fn generate_slug() -> String {
    let ts = chrono::Utc::now().format("%Y-%m-%d");
    format!("session-{ts}")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub project_id: String,
    pub slug: String,
    pub directory: String,
    pub title: Option<String>,
    pub version: u32,
    pub share_url: Option<String>,
    pub summary: Option<SessionSummary>,
    pub permission: Vec<PermissionRule>,
    pub parent_id: Option<String>,
    pub time_created: i64,
    pub time_updated: i64,
    pub time_compacting: Option<i64>,
    pub time_archived: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSummary {
    pub additions: i64,
    pub deletions: i64,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionRule {
    pub permission: String,
    pub pattern: String,
    pub action: String,
}

impl Session {
    #[must_use]
    pub fn new(project_id: String, directory: String) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            project_id,
            slug: generate_slug(),
            directory,
            title: None,
            version: 0,
            share_url: None,
            summary: None,
            permission: Vec::new(),
            parent_id: None,
            time_created: now,
            time_updated: now,
            time_compacting: None,
            time_archived: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn new_id_generates_non_empty_unique_strings() {
        let id1 = new_id();
        let id2 = new_id();
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
        assert_ne!(id1, id2);
    }

    #[test]
    fn new_id_generates_many_unique_ids() {
        let ids: HashSet<String> = (0..100).map(|_| new_id()).collect();
        assert_eq!(ids.len(), 100);
    }

    #[test]
    fn generate_slug_has_expected_prefix() {
        let slug = generate_slug();
        assert!(
            slug.starts_with("session-"),
            "slug should start with 'session-': {slug}"
        );
    }

    #[test]
    fn session_new_sets_fields() {
        let s = Session::new("proj-1".to_owned(), "/home/user".to_owned());
        assert_eq!(s.project_id, "proj-1");
        assert_eq!(s.directory, "/home/user");
        assert!(s.title.is_none());
        assert_eq!(s.version, 0);
        assert!(s.time_created > 0);
        assert_eq!(s.time_created, s.time_updated);
    }
}
