use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng as _;
use sha2::{Digest, Sha256};
use tokio::time::sleep;

use crate::auth::AuthError;

#[derive(Debug)]
pub struct DeviceFlowSession {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, serde::Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub token_type: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: Option<String>,
    // GitHub uses "verification_uri", some providers use "verification_url"
    verification_url: Option<String>,
    verification_uri_complete: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct PollResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    token_type: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Initiates device flow. Returns session with `user_code` and `verification_uri`.
pub async fn device_flow_start(
    client: &reqwest::Client,
    device_url: &str,
    client_id: &str,
    scope: &str,
) -> Result<DeviceFlowSession, AuthError> {
    let resp = client
        .post(device_url)
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("scope", scope)])
        .send()
        .await?
        .error_for_status()?
        .json::<DeviceCodeResponse>()
        .await?;

    let verification_uri = resp
        .verification_uri
        .or(resp.verification_url)
        .ok_or_else(|| {
            AuthError::Other("No verification_uri in device code response".to_string())
        })?;

    Ok(DeviceFlowSession {
        device_code: resp.device_code,
        user_code: resp.user_code,
        verification_uri,
        verification_uri_complete: resp.verification_uri_complete,
        expires_in: resp.expires_in.unwrap_or(900),
        interval: resp.interval.unwrap_or(5),
    })
}

/// Polls for token completion. Blocks until success or timeout.
/// Respects the `interval` from `DeviceFlowSession`.
pub async fn device_flow_poll(
    client: &reqwest::Client,
    token_url: &str,
    client_id: &str,
    session: &DeviceFlowSession,
) -> Result<OAuthTokens, AuthError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(session.expires_in);
    let mut interval_secs = session.interval;

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(AuthError::Timeout);
        }

        sleep(Duration::from_secs(interval_secs)).await;

        let resp = client
            .post(token_url)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", &session.device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await?
            .json::<PollResponse>()
            .await?;

        if let Some(token) = resp.access_token {
            return Ok(OAuthTokens {
                access_token: token,
                refresh_token: resp.refresh_token,
                expires_in: resp.expires_in,
                token_type: resp.token_type,
            });
        }

        match resp.error.as_deref() {
            Some("authorization_pending") | None => {
                // Keep polling
            }
            Some("slow_down") => {
                interval_secs += 5;
            }
            Some("access_denied") => {
                return Err(AuthError::Rejected(
                    resp.error_description
                        .unwrap_or_else(|| "access_denied".to_string()),
                ));
            }
            Some("expired_token") => {
                return Err(AuthError::Timeout);
            }
            Some(other) => {
                return Err(AuthError::Rejected(other.to_string()));
            }
        }
    }
}

/// Generates PKCE code verifier (43-128 random bytes, base64url encoded).
#[must_use]
pub fn pkce_verifier() -> String {
    let mut bytes = [0u8; 64];
    rand::thread_rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Generates PKCE code challenge (S256: SHA256(verifier), base64url encoded).
#[must_use]
pub fn pkce_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_verifier_valid_base64url() {
        let v = pkce_verifier();
        // Must be 43-128 characters
        assert!(
            (43..=128).contains(&v.len()),
            "verifier length {} out of range",
            v.len()
        );
        // Must only contain URL-safe base64 characters (no padding)
        assert!(
            v.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "verifier contains invalid chars: {v}"
        );
    }

    #[test]
    fn test_pkce_challenge_valid() {
        let v = pkce_verifier();
        let c = pkce_challenge(&v);
        // SHA256 of any input is 32 bytes → 43 base64url chars (no padding)
        assert_eq!(
            c.len(),
            43,
            "challenge length should be 43, got {}",
            c.len()
        );
        assert!(
            c.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "challenge contains invalid chars: {c}"
        );
    }

    #[test]
    fn test_pkce_challenge_deterministic() {
        let v = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let c1 = pkce_challenge(v);
        let c2 = pkce_challenge(v);
        assert_eq!(c1, c2);
    }
}
