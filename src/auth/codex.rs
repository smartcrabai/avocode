use std::collections::HashMap;
use std::fmt::Write as _;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::header;

use crate::auth::{
    AuthError,
    oauth::{OAuthTokens, device_flow_poll, device_flow_start, pkce_challenge, pkce_verifier},
    store::{AuthInfo, AuthStore},
};

pub const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const CODEX_API_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
pub const CODEX_MODELS: &[&str] = &[
    "gpt-5.1-codex",
    "gpt-5.1-codex-max",
    "gpt-5.1-codex-mini",
    "gpt-5.2",
    "gpt-5.2-codex",
    "gpt-5.3-codex",
    "gpt-5.4",
    "gpt-5.4-mini",
];

const CODEX_DEVICE_URL: &str = "https://auth.openai.com/oauth/device_code";
const CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_REDIRECT_URI: &str = "http://localhost:1455/callback";
const CODEX_SCOPE: &str = "openid email profile offline_access";
/// Refresh if expiry is within 5 minutes
const REFRESH_MARGIN_SECS: i64 = 5 * 60;

pub enum CodexAuthMethod {
    /// Headless device code flow (no browser required)
    DeviceCode,
    /// Browser-based PKCE flow (opens browser, starts local callback server)
    Browser,
}

pub struct CodexDeviceFlowPending {
    pub user_code: String,
    pub verification_uri: String,
    session: crate::auth::DeviceFlowSession,
    client: reqwest::Client,
}

pub struct CodexBrowserPending {
    /// URL to open in browser
    pub auth_url: String,
    verifier: String,
    state: String,
}

/// Start Codex device code flow (headless).
///
/// # Errors
/// Returns `AuthError` if the device code request fails.
pub async fn start_device_auth() -> Result<CodexDeviceFlowPending, AuthError> {
    let client = reqwest::Client::new();
    let session =
        device_flow_start(&client, CODEX_DEVICE_URL, CODEX_CLIENT_ID, CODEX_SCOPE).await?;
    Ok(CodexDeviceFlowPending {
        user_code: session.user_code.clone(),
        verification_uri: session.verification_uri.clone(),
        session,
        client,
    })
}

/// Poll until user completes device code auth. Returns tokens.
///
/// # Errors
/// Returns `AuthError` on timeout, denial, or HTTP failure.
pub async fn complete_device_auth(
    pending: CodexDeviceFlowPending,
) -> Result<OAuthTokens, AuthError> {
    device_flow_poll(
        &pending.client,
        CODEX_TOKEN_URL,
        CODEX_CLIENT_ID,
        &pending.session,
    )
    .await
}

/// Start Codex browser PKCE flow. Returns the URL to open and server state.
///
/// # Errors
/// This function currently always succeeds; the `Result` allows future fallible steps.
pub fn start_browser_auth() -> Result<CodexBrowserPending, AuthError> {
    let verifier = pkce_verifier();
    let challenge = pkce_challenge(&verifier);

    let state = {
        let bytes: Vec<u8> = (0..16).map(|_| rand::random::<u8>()).collect();
        URL_SAFE_NO_PAD.encode(&bytes)
    };

    let params: Vec<(&str, &str)> = vec![
        ("response_type", "code"),
        ("client_id", CODEX_CLIENT_ID),
        ("redirect_uri", CODEX_REDIRECT_URI),
        ("scope", CODEX_SCOPE),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("state", &state),
    ];

    let query = params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding_simple(v)))
        .collect::<Vec<_>>()
        .join("&");

    let auth_url = format!("https://auth.openai.com/oauth/authorize?{query}");

    Ok(CodexBrowserPending {
        auth_url,
        verifier,
        state,
    })
}

/// Exchange PKCE authorization code for tokens.
///
/// # Errors
/// Returns `AuthError::Rejected` on state mismatch or HTTP failure during token exchange.
pub async fn complete_browser_auth(
    pending: CodexBrowserPending,
    code: &str,
    returned_state: &str,
) -> Result<OAuthTokens, AuthError> {
    if returned_state != pending.state {
        return Err(AuthError::Rejected(
            "State mismatch -- possible CSRF attack".to_string(),
        ));
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(CODEX_TOKEN_URL)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CODEX_CLIENT_ID),
            ("code", code),
            ("redirect_uri", CODEX_REDIRECT_URI),
            ("code_verifier", &pending.verifier),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<OAuthTokens>()
        .await?;

    Ok(resp)
}

/// Refresh Codex access token using a refresh token.
///
/// # Errors
/// Returns `AuthError` if the token refresh HTTP request fails.
pub async fn refresh_token(refresh_token: &str) -> Result<OAuthTokens, AuthError> {
    let client = reqwest::Client::new();
    let resp = client
        .post(CODEX_TOKEN_URL)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", CODEX_CLIENT_ID),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<OAuthTokens>()
        .await?;

    Ok(resp)
}

/// Get valid Codex access token, refreshing if needed.
///
/// # Errors
/// Returns `AuthError` if no Codex auth is stored, the auth type is wrong, or token refresh fails.
pub async fn get_valid_token(store: &AuthStore) -> Result<String, AuthError> {
    let auth = store
        .get("codex")?
        .ok_or_else(|| AuthError::Other("No Codex auth found".to_string()))?;

    match auth {
        AuthInfo::Oauth {
            access,
            refresh,
            expires,
            ..
        } => {
            let now_secs = chrono::Utc::now().timestamp();
            let expires_secs = expires.unwrap_or(0) / 1_000;

            if expires_secs - now_secs > REFRESH_MARGIN_SECS {
                return Ok(access);
            }

            let rt = refresh
                .ok_or_else(|| AuthError::Other("No refresh token for Codex".to_string()))?;
            let tokens = refresh_token(&rt).await?;

            let new_expires_ms = tokens
                .expires_in
                .map(|secs| (chrono::Utc::now().timestamp() + secs.cast_signed()) * 1_000);

            let account_id = extract_jwt_sub(&tokens.access_token);

            store.set(
                "codex",
                AuthInfo::Oauth {
                    refresh: tokens.refresh_token,
                    access: tokens.access_token.clone(),
                    expires: new_expires_ms,
                    account_id,
                    enterprise_url: None,
                },
            )?;

            Ok(tokens.access_token)
        }
        _ => Err(AuthError::Other(
            "Expected OAuth auth info for Codex".to_string(),
        )),
    }
}

/// Minimal JWT sub extraction (no verification -- only for reading account ID).
fn extract_jwt_sub(jwt: &str) -> Option<String> {
    let parts: Vec<&str> = jwt.split('.').collect();
    let payload = parts.get(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: HashMap<String, serde_json::Value> = serde_json::from_slice(&decoded).ok()?;
    claims
        .get("sub")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Minimal percent-encoding for query parameter values.
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                let _ = write!(out, "{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codex_models_non_empty() {
        assert!(!CODEX_MODELS.is_empty(), "CODEX_MODELS must not be empty");
    }

    #[test]
    fn test_codex_models_contain_known_model() {
        assert!(
            CODEX_MODELS.contains(&"gpt-5.1-codex"),
            "Expected gpt-5.1-codex in CODEX_MODELS"
        );
    }

    #[test]
    fn test_codex_api_url_is_expected() {
        assert_eq!(
            CODEX_API_URL,
            "https://chatgpt.com/backend-api/codex/responses"
        );
    }

    #[test]
    fn test_urlencoding_simple_space() {
        let encoded = urlencoding_simple("hello world");
        assert_eq!(encoded, "hello%20world");
    }

    #[test]
    fn test_urlencoding_simple_alphanumeric_unchanged() {
        let input = "abc123-_.~";
        let encoded = urlencoding_simple(input);
        assert_eq!(encoded, input);
    }

    #[test]
    fn test_extract_jwt_sub_valid() {
        // Craft a minimal JWT with sub claim (no real signing needed -- we don't verify)
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(r#"{"sub":"user_123","email":"test@example.com"}"#);
        let jwt = format!("{header}.{payload}.sig");
        let sub = extract_jwt_sub(&jwt);
        assert_eq!(sub, Some("user_123".to_string()));
    }
}
