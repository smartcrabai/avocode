/// A chat session within a project.
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
    /// Unix timestamp in milliseconds.
    pub time_created: i64,
    /// Unix timestamp in milliseconds.
    pub time_updated: i64,
    pub time_compacting: Option<i64>,
    pub time_archived: Option<i64>,
}

/// Summarises the diff statistics for a session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSummary {
    pub additions: i64,
    pub deletions: i64,
    pub files: Vec<String>,
}

/// A single permission rule applied to a session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionRule {
    pub permission: String,
    pub pattern: String,
    /// One of `"allow"`, `"deny"`, or `"ask"`.
    pub action: String,
}

/// Generate a time-ordered session ID using UUID v7.
#[must_use]
pub fn new_session_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Return the current wall-clock time as Unix milliseconds.
#[must_use]
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Generate a human-readable slug from an optional title.
///
/// Non-alphanumeric runs become a single hyphen; leading/trailing hyphens are
/// trimmed; the result is truncated to 40 characters.  Falls back to
/// `"session-<date>"` when no title is supplied or the title is all
/// non-alphanumeric.
#[must_use]
pub fn generate_slug(title: Option<&str>) -> String {
    let Some(t) = title.filter(|t| !t.trim().is_empty()) else {
        return date_slug();
    };

    // Collapse runs of non-alphanumeric chars into a single '-', then
    // strip any leading/trailing '-', and take at most 40 chars — all in
    // one pass without intermediate allocations.
    let mut slug = String::with_capacity(t.len().min(40));
    let mut prev_hyphen = true; // treat start as if preceded by '-' to skip leading hyphens
    for c in t.to_lowercase().chars().take(80) {
        if c.is_alphanumeric() {
            slug.push(c);
            prev_hyphen = false;
            if slug.len() == 40 {
                break;
            }
        } else if !prev_hyphen {
            slug.push('-');
            prev_hyphen = true;
        }
    }
    // Trim trailing hyphen that may result from truncation.
    let slug = slug.trim_end_matches('-');

    if slug.is_empty() {
        date_slug()
    } else {
        slug.to_string()
    }
}

fn date_slug() -> String {
    let date = chrono::Utc::now().format("%Y-%m-%d");
    format!("session-{date}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_id_generates_unique_ids() {
        let a = new_session_id();
        let b = new_session_id();
        assert_ne!(a, b, "consecutive IDs must be unique");
        assert!(!a.is_empty());
        assert!(!b.is_empty());
    }

    #[test]
    fn generate_slug_non_empty_from_title() {
        let slug = generate_slug(Some("My Cool Session"));
        assert!(!slug.is_empty());
        assert_eq!(slug, "my-cool-session");
    }

    #[test]
    fn generate_slug_non_empty_without_title() {
        let slug = generate_slug(None);
        assert!(!slug.is_empty());
        assert!(slug.starts_with("session-"));
    }
}
