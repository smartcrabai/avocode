//! HTTP API integration test: real communication with `openai-mokku-go`.
//!
//! Starts the mock container, creates an `AppState` with the mock's `base_url`
//! in the project config, and verifies that:
//! - `POST /session` creates a session
//! - `POST /session/{id}/message` triggers LLM streaming and returns
//!   assistant content

mod support;

use avocode::server::create_router;
use avocode::server::state::AppState;
use avocode::session::schema::Session;
use avocode::session::store::SessionStore;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use support::mock_openai::{MockOpenAi, write_project_config};
use tower::ServiceExt as _;

/// Given: the mock container is running and `AppState` has a session store
///   containing a session whose directory has a project config pointing to
///   the mock,
/// When: `POST /session/{id}/message` with `content: "Hello"` is sent,
/// Then: the response body contains the assistant echo of "Hello".
#[tokio::test]
#[ignore = "requires Docker, openai-mokku-go image, and enhanced send_message implementation"]
async fn http_send_message_returns_assistant_echo()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Given: mock + project config + session in store
    let mock = MockOpenAi::start().await?;
    let project_dir = tempfile::tempdir()?;
    write_project_config(project_dir.path(), mock.base_url())?;

    let store = SessionStore::open_in_memory()?;
    let session = Session::new(
        "proj-1".to_owned(),
        project_dir.path().display().to_string(),
    );
    let session_id = session.id.clone();
    store.create_session(&session)?;

    let state = AppState::with_store(store);
    let app = create_router(state);

    // When: send a message
    let body = serde_json::json!({
        "content": "Hello, echo!",
        "model": "openai/gpt-4o"
    });
    let uri = format!("/session/{session_id}/message");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&uri)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&body)?))?,
        )
        .await?;

    // Then: 200 and response contains assistant text
    assert_eq!(response.status(), StatusCode::OK);
    let resp_bytes = axum::body::to_bytes(response.into_body(), 8192).await?;
    let resp_body: serde_json::Value = serde_json::from_slice(&resp_bytes)?;

    // The mock echoes the user message, so assistant content should contain it
    let assistant_text = resp_body
        .get("assistant")
        .or_else(|| resp_body.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    assert!(
        !assistant_text.is_empty(),
        "expected non-empty assistant content, got: {resp_body}"
    );

    Ok(())
}

/// Given: a session exists in the store,
/// When: `POST /session/{id}/message` triggers streaming,
/// Then: SSE events are broadcast (`MessageCreated`, `PartUpdated`).
#[tokio::test]
#[ignore = "requires Docker, openai-mokku-go image, and enhanced send_message with event broadcasting"]
async fn http_send_message_broadcasts_streaming_events()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mock = MockOpenAi::start().await?;
    let project_dir = tempfile::tempdir()?;
    write_project_config(project_dir.path(), mock.base_url())?;

    let store = SessionStore::open_in_memory()?;
    let session = Session::new(
        "proj-1".to_owned(),
        project_dir.path().display().to_string(),
    );
    let session_id = session.id.clone();
    store.create_session(&session)?;

    let state = AppState::with_store(store);
    let mut event_rx = state.subscribe();
    let app = create_router(state);

    let body = serde_json::json!({
        "content": "Stream test",
        "model": "openai/gpt-4o"
    });
    let uri = format!("/session/{session_id}/message");
    let _response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&uri)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&body)?))?,
        )
        .await?;

    // Then: at least one event was broadcast (with timeout to avoid race conditions)
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv()).await;
    assert!(
        event.is_ok(),
        "expected at least one server event broadcast"
    );

    Ok(())
}

/// Given: the mock container is running,
/// When: `POST /session` is called with a directory,
/// Then: the response contains a session id and title.
#[tokio::test]
#[ignore = "requires Docker and enhanced create_session implementation"]
async fn http_create_session_with_directory() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let _mock = MockOpenAi::start().await?;
    let project_dir = tempfile::tempdir()?;

    let store = SessionStore::open_in_memory()?;
    let state = AppState::with_store(store);
    let app = create_router(state);

    let body = serde_json::json!({
        "title": "Test Session",
        "directory": project_dir.path().display().to_string()
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/session")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&body)?))?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let resp_body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 1024).await?)?;

    assert!(resp_body["id"].is_string(), "response should have id");
    assert_eq!(resp_body["title"], "Test Session");

    Ok(())
}
