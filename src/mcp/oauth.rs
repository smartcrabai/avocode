use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

/// Errors from the OAuth device flow.
#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("authorization denied")]
    Denied,

    #[error("device code request failed: {0}")]
    DeviceCode(String),

    #[error("token polling timed out")]
    Timeout,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

const MAX_POLLS: u32 = 60;

/// Perform the OAuth 2.0 device authorization flow against `server_url`.
///
/// 1. POST to `{server_url}/oauth/device/code` to obtain a device code.
/// 2. Poll `{server_url}/oauth/token` until the user authorizes.
/// 3. Return the resulting access token.
///
/// The token is stored only in memory for the lifetime of the calling scope.
///
/// # Errors
///
/// Returns an error if the HTTP requests fail, the authorization is denied, or
/// the polling times out.
pub async fn fetch_oauth_token(server_url: &str, client_id: &str) -> Result<String, OAuthError> {
    let client = Client::new();

    // Step 1 — request a device code.
    let device_resp: DeviceCodeResponse = client
        .post(format!("{server_url}/oauth/device/code"))
        .json(&serde_json::json!({ "client_id": client_id }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let interval = std::time::Duration::from_secs(device_resp.interval.unwrap_or(5));

    // Step 2 — poll for the token.
    for _ in 0..MAX_POLLS {
        tokio::time::sleep(interval).await;

        let token_resp: TokenResponse = client
            .post(format!("{server_url}/oauth/token"))
            .json(&serde_json::json!({
                "client_id": client_id,
                "device_code": device_resp.device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
            }))
            .send()
            .await?
            .json()
            .await?;

        match token_resp.error.as_deref() {
            None => {
                return token_resp
                    .access_token
                    .ok_or(OAuthError::DeviceCode("missing access_token".to_owned()));
            }
            Some("authorization_pending" | "slow_down") => {}
            Some("access_denied") => return Err(OAuthError::Denied),
            Some(other) => return Err(OAuthError::DeviceCode(other.to_owned())),
        }
    }

    Err(OAuthError::Timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_error_display() {
        let err = OAuthError::Timeout;
        assert_eq!(err.to_string(), "token polling timed out");

        let err = OAuthError::Denied;
        assert_eq!(err.to_string(), "authorization denied");

        let err = OAuthError::DeviceCode("bad_request".to_owned());
        assert!(err.to_string().contains("bad_request"));
    }

    #[test]
    fn test_device_code_response_deserialize() {
        let json = r#"{"device_code":"abc123","interval":5}"#;
        let resp: DeviceCodeResponse = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.device_code, "abc123");
        assert_eq!(resp.interval, Some(5));
    }

    #[test]
    fn test_token_response_deserialize_success() {
        let json = r#"{"access_token":"tok_xyz"}"#;
        let resp: TokenResponse = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.access_token.as_deref(), Some("tok_xyz"));
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_token_response_deserialize_pending() {
        let json = r#"{"error":"authorization_pending"}"#;
        let resp: TokenResponse = serde_json::from_str(json).expect("deserialize");
        assert!(resp.access_token.is_none());
        assert_eq!(resp.error.as_deref(), Some("authorization_pending"));
    }
}
