use reqwest::header::{self, HeaderMap, HeaderValue};

use crate::auth::{
    AuthError,
    oauth::{DeviceFlowSession, OAuthTokens, device_flow_poll, device_flow_start},
    store::{AuthInfo, AuthStore},
};

pub const COPILOT_CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";
pub const COPILOT_API_BASE: &str = "https://api.githubcopilot.com";

const GITHUB_DEVICE_URL: &str = "https://github.com/login/device/code";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const SCOPE: &str = "read:user";

const REFRESH_MARGIN_MS: i64 = 5 * 60 * 1_000;
const SESSION_KEY_NAME: &str = "copilot_session";

pub struct DeviceFlowPending {
    pub user_code: String,
    pub verification_uri: String,
    pub(crate) session: DeviceFlowSession,
    pub(crate) client: reqwest::Client,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CopilotSession {
    pub token: String,
    pub expires_at: i64,
    pub endpoints: Option<CopilotEndpoints>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CopilotEndpoints {
    pub api: Option<String>,
}

/// Start GitHub device flow. Returns `user_code` and `verification_uri` for the user.
///
/// # Errors
/// Returns `AuthError` if the GitHub device code request fails.
pub async fn start_auth() -> Result<DeviceFlowPending, AuthError> {
    let client = reqwest::Client::new();
    let session = device_flow_start(&client, GITHUB_DEVICE_URL, COPILOT_CLIENT_ID, SCOPE).await?;
    Ok(DeviceFlowPending {
        user_code: session.user_code.clone(),
        verification_uri: session.verification_uri.clone(),
        session,
        client,
    })
}

/// Poll until user completes auth. Returns GitHub OAuth access token.
///
/// # Errors
/// Returns `AuthError` on timeout, user denial, or HTTP failure.
pub async fn complete_auth(pending: DeviceFlowPending) -> Result<String, AuthError> {
    let tokens: OAuthTokens = device_flow_poll(
        &pending.client,
        GITHUB_TOKEN_URL,
        COPILOT_CLIENT_ID,
        &pending.session,
    )
    .await?;
    Ok(tokens.access_token)
}

#[derive(serde::Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: Option<i64>,
    endpoints: Option<CopilotEndpointsRaw>,
}

#[derive(serde::Deserialize)]
struct CopilotEndpointsRaw {
    api: Option<String>,
}

/// Exchange a GitHub OAuth token for a short-lived Copilot session token.
///
/// # Errors
/// Returns `AuthError` if the Copilot token exchange HTTP request fails.
pub async fn copilot_token_exchange(github_token: &str) -> Result<CopilotSession, AuthError> {
    let client = reqwest::Client::new();

    let resp = client
        .get(COPILOT_TOKEN_URL)
        .header(header::AUTHORIZATION, format!("token {github_token}"))
        .header(header::USER_AGENT, "avocode/0.1.0")
        .send()
        .await?
        .error_for_status()?
        .json::<CopilotTokenResponse>()
        .await?;

    Ok(CopilotSession {
        token: resp.token,
        expires_at: resp.expires_at.unwrap_or(0),
        endpoints: resp.endpoints.map(|e| CopilotEndpoints { api: e.api }),
    })
}

/// Get a valid Copilot token, refreshing if expired (within 5 min of expiry).
///
/// # Errors
/// Returns `AuthError` if no auth is stored, the stored auth is wrong type, or token exchange fails.
pub async fn get_valid_token(
    store: &AuthStore,
    provider_id: &str,
) -> Result<CopilotSession, AuthError> {
    let auth = store
        .get(provider_id)?
        .ok_or_else(|| AuthError::Other(format!("No auth found for provider: {provider_id}")))?;

    let AuthInfo::Oauth {
        access: github_token,
        ..
    } = auth
    else {
        return Err(AuthError::Other(
            "Expected OAuth auth info for Copilot".to_string(),
        ));
    };

    let session_key = format!("{provider_id}__session");
    if let Ok(Some(AuthInfo::WellKnown { key: _, token })) = store.get(&session_key)
        && let Ok(session) = serde_json::from_str::<CopilotSession>(&token)
    {
        let now_ms = chrono::Utc::now().timestamp_millis();
        if session.expires_at - now_ms > REFRESH_MARGIN_MS {
            return Ok(session);
        }
    }

    let session = copilot_token_exchange(&github_token).await?;

    let session_json = serde_json::to_string(&session)?;
    store.set(
        &session_key,
        AuthInfo::WellKnown {
            key: SESSION_KEY_NAME.to_string(),
            token: session_json,
        },
    )?;

    Ok(session)
}

/// Build reqwest headers for Copilot API requests.
pub fn copilot_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let auth_val = HeaderValue::from_str(&format!("Bearer {token}"))
        .unwrap_or_else(|_| HeaderValue::from_static("Bearer invalid"));
    headers.insert(header::AUTHORIZATION, auth_val);
    headers.insert(
        "openai-intent",
        HeaderValue::from_static("conversation-edits"),
    );
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static("avocode/0.1.0"),
    );
    headers.insert("x-initiator", HeaderValue::from_static("agent"));
    headers.insert("editor-version", HeaderValue::from_static("avocode/0.1.0"));
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copilot_headers_contain_expected_keys() {
        let headers = copilot_headers("test_token");
        assert!(headers.contains_key(header::AUTHORIZATION));
        assert!(headers.contains_key(header::USER_AGENT));
        assert!(headers.contains_key("openai-intent"));
        assert!(headers.contains_key("x-initiator"));
        assert!(headers.contains_key("editor-version"));
    }

    #[test]
    fn test_copilot_headers_authorization_value() -> Result<(), Box<dyn std::error::Error>> {
        let headers = copilot_headers("my_secret_token");
        let auth = headers
            .get(header::AUTHORIZATION)
            .ok_or("missing Authorization header")?
            .to_str()?;
        assert_eq!(auth, "Bearer my_secret_token");
        Ok(())
    }
}
