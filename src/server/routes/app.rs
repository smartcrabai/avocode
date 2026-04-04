use axum::Json;
use serde::Serialize;

/// Summary of a single agent as returned by `GET /app/agents`.
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
pub struct AgentSummary {
    pub name: String,
    pub description: String,
    pub mode: crate::agent::schema::AgentMode,
    pub hidden: bool,
}

/// GET /app/agents — list all available built-in agents.
pub async fn list_agents() -> Json<Vec<AgentSummary>> {
    let agents = crate::agent::builtin::builtin_agents();
    let summaries: Vec<AgentSummary> = agents
        .into_values()
        .map(|info| AgentSummary {
            name: info.name,
            description: info.description,
            mode: info.mode,
            hidden: info.hidden,
        })
        .collect();
    Json(summaries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::AppState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::{Router, routing::get};
    use tower::ServiceExt as _;

    fn make_router() -> Router {
        let state = AppState::new();
        Router::new()
            .route("/app/agents", get(list_agents))
            .with_state(state)
    }

    // -----------------------------------------------------------------------
    // GET /app/agents — happy path
    // -----------------------------------------------------------------------

    /// Endpoint must return HTTP 200.
    #[tokio::test]
    async fn list_agents_returns_200() -> Result<(), Box<dyn std::error::Error>> {
        // Given: /app/agents router
        let app = make_router();

        // When: GET /app/agents
        let response = app
            .oneshot(Request::builder().uri("/app/agents").body(Body::empty())?)
            .await?;

        // Then: 200 OK
        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }

    /// Response must contain exactly the 4 built-in agents: build, plan,
    /// general, explore.
    #[tokio::test]
    async fn list_agents_returns_four_builtin_agents() -> Result<(), Box<dyn std::error::Error>> {
        // Given: /app/agents router
        let app = make_router();

        // When: GET /app/agents
        let response = app
            .oneshot(Request::builder().uri("/app/agents").body(Body::empty())?)
            .await?;

        // Then: array length is 4 and all expected names are present
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 4096).await?;
        let agents: Vec<AgentSummary> = serde_json::from_slice(&bytes)?;
        assert_eq!(agents.len(), 4, "expected exactly 4 builtin agents");

        let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"build"), "missing 'build' agent");
        assert!(names.contains(&"plan"), "missing 'plan' agent");
        assert!(names.contains(&"general"), "missing 'general' agent");
        assert!(names.contains(&"explore"), "missing 'explore' agent");

        Ok(())
    }

    /// Every agent must have a non-empty name and description.
    #[tokio::test]
    async fn list_agents_each_has_nonempty_name_and_description()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: /app/agents router
        let app = make_router();

        // When: GET /app/agents
        let response = app
            .oneshot(Request::builder().uri("/app/agents").body(Body::empty())?)
            .await?;

        // Then: each entry has name and description
        let bytes = axum::body::to_bytes(response.into_body(), 4096).await?;
        let agents: Vec<AgentSummary> = serde_json::from_slice(&bytes)?;
        for agent in &agents {
            assert!(!agent.name.is_empty(), "agent has empty name");
            assert!(
                !agent.description.is_empty(),
                "agent '{}' has empty description",
                agent.name
            );
        }
        Ok(())
    }

    /// The `mode` field must be one of the known values (`primary`, `subagent`,
    /// `all`).
    #[tokio::test]
    async fn list_agents_mode_field_is_a_known_value() -> Result<(), Box<dyn std::error::Error>> {
        // Given: /app/agents router
        let app = make_router();

        // When: GET /app/agents
        let response = app
            .oneshot(Request::builder().uri("/app/agents").body(Body::empty())?)
            .await?;

        // Then: mode is one of the expected enum variants
        let bytes = axum::body::to_bytes(response.into_body(), 4096).await?;
        let agents: Vec<AgentSummary> = serde_json::from_slice(&bytes)?;
        let valid_modes = [
            crate::agent::schema::AgentMode::Primary,
            crate::agent::schema::AgentMode::Subagent,
            crate::agent::schema::AgentMode::All,
        ];
        for agent in &agents {
            assert!(
                valid_modes.contains(&agent.mode),
                "agent '{}' has unexpected mode {:?}",
                agent.name,
                agent.mode
            );
        }
        Ok(())
    }

    /// `build` and `plan` agents have `hidden = false` (they appear in the
    /// agent picker); `general` and `explore` are subagents and may be hidden.
    #[tokio::test]
    async fn list_agents_primary_agents_are_not_hidden() -> Result<(), Box<dyn std::error::Error>> {
        // Given: /app/agents router
        let app = make_router();

        // When: GET /app/agents
        let response = app
            .oneshot(Request::builder().uri("/app/agents").body(Body::empty())?)
            .await?;

        // Then: build and plan are not hidden
        let bytes = axum::body::to_bytes(response.into_body(), 4096).await?;
        let agents: Vec<AgentSummary> = serde_json::from_slice(&bytes)?;
        for agent in agents
            .iter()
            .filter(|a| matches!(a.mode, crate::agent::schema::AgentMode::Primary))
        {
            assert!(
                !agent.hidden,
                "primary agent '{}' should not be hidden",
                agent.name
            );
        }
        Ok(())
    }
}
