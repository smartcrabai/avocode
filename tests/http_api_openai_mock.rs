//! HTTP API integration tests using the `openai-mokku-go` Docker container.
#![expect(clippy::expect_used)]
//!
//! Each test:
//!  1. Starts the mock container.
//!  2. Creates an isolated temp HOME + project directory.
//!  3. Writes `opencode.jsonc` with the container's base URL.
//!  4. Spawns `avocode serve` on a free local port.
//!  5. Exercises the REST API via `reqwest`.
//!  6. Asserts the response content and structure.
//!
//! None of these tests talk to real `OpenAI` or any other live service.

mod common;

use std::time::Duration;

use common::fs::TestEnv;
use common::openai_mock::OpenAiMock;
use common::process::{free_local_port, spawn_avocode_serve, wait_for_server};

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

/// `POST /session/{id}/message` with `"hello"` should return a response body
/// that contains `Echo: hello` once the processor calls the mock.
#[tokio::test]
async fn http_send_message_returns_echoed_assistant_text() {
    // Given: a running mock container
    let mock = OpenAiMock::start().await;

    // Given: an isolated project directory with config pointing at the mock
    let env = TestEnv::new();
    env.write_openai_config(&mock.base_url);

    // Given: avocode serve running on a free port
    let port = free_local_port();
    let mut server =
        spawn_avocode_serve("127.0.0.1", port, &env.env_overrides(), env.project_path());
    let server_url = format!("http://127.0.0.1:{port}");
    wait_for_server(&format!("{server_url}/provider")).await;

    let client = reqwest::Client::new();

    // When: create a session rooted at the project directory
    let create_resp = client
        .post(format!("{server_url}/session"))
        .json(&serde_json::json!({
            "title": "test session",
            "directory": env.project_path().display().to_string()
        }))
        .send()
        .await
        .expect("POST /session failed");
    assert_eq!(
        create_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 from POST /session"
    );
    let session: serde_json::Value = create_resp
        .json()
        .await
        .expect("failed to parse session response");
    let session_id = session["id"]
        .as_str()
        .expect("session response missing 'id' field");

    // When: send a message to the session
    let send_resp = client
        .post(format!("{server_url}/session/{session_id}/message"))
        .json(&serde_json::json!({
            "content": "hello",
            "model": "openai/gpt-4o"
        }))
        .send()
        .await
        .expect("POST /session/{id}/message failed");

    assert_eq!(
        send_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 from POST /session/{{id}}/message"
    );
    let body: serde_json::Value = send_resp
        .json()
        .await
        .expect("failed to parse send_message response");

    // Then: the response body contains the echoed assistant text
    let body_str = body.to_string();
    assert!(
        body_str.contains("Echo: hello"),
        "expected 'Echo: hello' in response body, got: {body_str}"
    );

    server.kill().await.ok();
}

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

/// `POST /session` should return a valid session id and `GET /session/{id}`
/// should return the session data (not 404) after creation.
#[tokio::test]
async fn http_get_session_returns_created_session() {
    // Given: avocode serve running (no mock needed for session lifecycle)
    let env = TestEnv::new();
    let port = free_local_port();
    let mut server =
        spawn_avocode_serve("127.0.0.1", port, &env.env_overrides(), env.project_path());
    let server_url = format!("http://127.0.0.1:{port}");
    wait_for_server(&format!("{server_url}/provider")).await;

    let client = reqwest::Client::new();

    // When: create a session
    let create_resp = client
        .post(format!("{server_url}/session"))
        .json(&serde_json::json!({
            "directory": env.project_path().display().to_string()
        }))
        .send()
        .await
        .expect("POST /session failed");
    assert_eq!(create_resp.status(), reqwest::StatusCode::OK);

    let session: serde_json::Value = create_resp.json().await.expect("parse error");
    let session_id = session["id"].as_str().expect("missing id");

    // Then: GET /session/{id} returns the session (not 404)
    let get_resp = client
        .get(format!("{server_url}/session/{session_id}"))
        .send()
        .await
        .expect("GET /session/{id} failed");

    assert_eq!(
        get_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 from GET /session/{{id}}, got {:?}",
        get_resp.status()
    );

    server.kill().await.ok();
}

/// `GET /session` should list sessions including one that was just created.
#[tokio::test]
async fn http_list_sessions_includes_created_session() {
    // Given: avocode serve running
    let env = TestEnv::new();
    let port = free_local_port();
    let mut server =
        spawn_avocode_serve("127.0.0.1", port, &env.env_overrides(), env.project_path());
    let server_url = format!("http://127.0.0.1:{port}");
    wait_for_server(&format!("{server_url}/provider")).await;

    let client = reqwest::Client::new();

    // When: create a session then list all sessions
    let create_resp = client
        .post(format!("{server_url}/session"))
        .json(&serde_json::json!({
            "directory": env.project_path().display().to_string()
        }))
        .send()
        .await
        .expect("POST /session failed");
    assert_eq!(
        create_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 from POST /session"
    );
    let created: serde_json::Value = create_resp.json().await.expect("parse error");
    let created_id = created["id"].as_str().expect("missing id");

    let list_resp = client
        .get(format!("{server_url}/session"))
        .send()
        .await
        .expect("GET /session failed");

    assert_eq!(list_resp.status(), reqwest::StatusCode::OK);

    let sessions: Vec<serde_json::Value> = list_resp.json().await.expect("parse error");

    // Then: the created session appears in the list
    assert!(
        sessions
            .iter()
            .any(|s| s["id"].as_str() == Some(created_id)),
        "expected session {created_id} in list, got: {sessions:?}"
    );

    server.kill().await.ok();
}

// ---------------------------------------------------------------------------
// Negative path -- credit-error model
// ---------------------------------------------------------------------------

/// Using the `credit-error` model should result in an error being surfaced
/// in the response rather than an echoed assistant message.
#[tokio::test]
async fn http_credit_error_model_surfaces_error_in_response() {
    // Given: mock container + isolated env with credit-error config
    let mock = OpenAiMock::start().await;
    let env = TestEnv::new();
    env.write_credit_error_config(&mock.base_url);

    let port = free_local_port();
    let mut server =
        spawn_avocode_serve("127.0.0.1", port, &env.env_overrides(), env.project_path());
    let server_url = format!("http://127.0.0.1:{port}");
    wait_for_server(&format!("{server_url}/provider")).await;

    let client = reqwest::Client::new();

    // When: create a session and send a message with credit-error model
    let create_resp = client
        .post(format!("{server_url}/session"))
        .json(&serde_json::json!({
            "directory": env.project_path().display().to_string()
        }))
        .send()
        .await
        .expect("POST /session failed");
    assert_eq!(
        create_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 from POST /session"
    );
    let session: serde_json::Value = create_resp.json().await.expect("parse error");
    let session_id = session["id"].as_str().expect("missing id");

    let send_resp = client
        .post(format!("{server_url}/session/{session_id}/message"))
        .json(&serde_json::json!({
            "content": "trigger error",
            "model": "openai/credit-error"
        }))
        .send()
        .await
        .expect("POST /session/{id}/message failed");

    // Then: either a non-2xx status is returned or the body describes an error
    let status = send_resp.status();
    let body: serde_json::Value = send_resp.json().await.unwrap_or(serde_json::Value::Null);
    let body_str = body.to_string();

    let indicates_error =
        !status.is_success() || body_str.contains("error") || body_str.contains("Error");

    assert!(
        indicates_error,
        "expected error indication for credit-error model, got status={status} body={body_str}"
    );

    server.kill().await.ok();
}

// ---------------------------------------------------------------------------
// Server-Sent Events
// ---------------------------------------------------------------------------

/// After sending a message, the `/event` SSE stream should emit at least one
/// event related to the message (e.g. `MessageCreated` or `PartUpdated`).
#[tokio::test]
async fn http_event_stream_emits_message_events_after_send() {
    use futures_util::StreamExt;

    // Given: mock container + isolated env
    let mock = OpenAiMock::start().await;
    let env = TestEnv::new();
    env.write_openai_config(&mock.base_url);

    let port = free_local_port();
    let mut server =
        spawn_avocode_serve("127.0.0.1", port, &env.env_overrides(), env.project_path());
    let server_url = format!("http://127.0.0.1:{port}");
    wait_for_server(&format!("{server_url}/provider")).await;

    let client = reqwest::Client::new();

    // Subscribe to the event stream before sending the message
    let event_resp = client
        .get(format!("{server_url}/event"))
        .send()
        .await
        .expect("GET /event failed");
    assert_eq!(event_resp.status(), reqwest::StatusCode::OK);
    let mut event_stream = event_resp.bytes_stream();

    // When: create a session and send a message
    let create_resp = client
        .post(format!("{server_url}/session"))
        .json(&serde_json::json!({
            "directory": env.project_path().display().to_string()
        }))
        .send()
        .await
        .expect("POST /session failed");
    let session: serde_json::Value = create_resp.json().await.expect("parse error");
    let session_id = session["id"].as_str().expect("missing id");

    client
        .post(format!("{server_url}/session/{session_id}/message"))
        .json(&serde_json::json!({
            "content": "hello",
            "model": "openai/gpt-4o"
        }))
        .send()
        .await
        .expect("send message failed");

    // Then: collect SSE events for up to 10 seconds and find a message-related one
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut found_message_event = false;

    while std::time::Instant::now() < deadline {
        let chunk = tokio::time::timeout(Duration::from_secs(2), event_stream.next()).await;
        match chunk {
            Ok(Some(Ok(bytes))) => {
                let text = String::from_utf8_lossy(&bytes);
                if text.contains("MessageCreated")
                    || text.contains("PartUpdated")
                    || text.contains("SessionUpdated")
                {
                    found_message_event = true;
                    break;
                }
            }
            _ => break,
        }
    }

    assert!(
        found_message_event,
        "expected a message-related SSE event (MessageCreated/PartUpdated/SessionUpdated) \
         on the /event stream after sending a message"
    );

    server.kill().await.ok();
}
