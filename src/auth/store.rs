use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use crate::auth::AuthError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthInfo {
    Api {
        key: String,
    },
    Oauth {
        refresh: Option<String>,
        access: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        account_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        enterprise_url: Option<String>,
    },
    WellKnown {
        key: String,
        token: String,
    },
}

pub struct AuthStore {
    path: PathBuf,
}

impl AuthStore {
    /// # Errors
    /// Returns `AuthError` if the data directory cannot be determined or created.
    pub fn new() -> Result<Self, AuthError> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| AuthError::Other("Cannot determine data directory".to_string()))?;
        let auth_dir = data_dir.join("avocode");
        fs::create_dir_all(&auth_dir)?;
        Ok(Self {
            path: auth_dir.join("auth.json"),
        })
    }

    /// # Errors
    /// Returns `AuthError` if the auth file exists but cannot be read or parsed.
    pub fn get(&self, provider_id: &str) -> Result<Option<AuthInfo>, AuthError> {
        let all = self.read_all()?;
        Ok(all.get(provider_id).cloned())
    }

    /// # Errors
    /// Returns `AuthError` if the auth file cannot be read or written.
    pub fn set(&self, provider_id: &str, auth: AuthInfo) -> Result<(), AuthError> {
        let norm = provider_id.trim_end_matches('/');
        let mut data = self.read_all()?;
        data.remove(&format!("{norm}/"));
        if norm != provider_id {
            data.remove(provider_id);
        }
        data.insert(norm.to_string(), auth);
        self.write_all(&data)
    }

    /// # Errors
    /// Returns `AuthError` if the auth file cannot be read or written.
    pub fn remove(&self, provider_id: &str) -> Result<(), AuthError> {
        let norm = provider_id.trim_end_matches('/');
        let mut data = self.read_all()?;
        data.remove(provider_id);
        data.remove(norm);
        self.write_all(&data)
    }

    /// # Errors
    /// Returns `AuthError` if the auth file cannot be read or parsed.
    pub fn list(&self) -> Result<HashMap<String, AuthInfo>, AuthError> {
        self.read_all()
    }

    fn read_all(&self) -> Result<HashMap<String, AuthInfo>, AuthError> {
        let content = match fs::read_to_string(&self.path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(e) => return Err(AuthError::Io(e)),
        };
        let map: HashMap<String, serde_json::Value> = serde_json::from_str(&content)?;
        let mut result = HashMap::new();
        for (k, v) in map {
            if let Ok(info) = serde_json::from_value::<AuthInfo>(v) {
                result.insert(k, info);
            }
        }
        Ok(result)
    }

    fn write_all(&self, data: &HashMap<String, AuthInfo>) -> Result<(), AuthError> {
        let json = serde_json::to_string_pretty(data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true).mode(0o600);
            let mut file = opts.open(&self.path)?;
            file.write_all(json.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            fs::write(&self.path, json)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Build a store pointing at a temp file for testing.
    fn temp_store(dir: &Path) -> AuthStore {
        let path = dir.join("auth.json");
        AuthStore { path }
    }

    #[test]
    fn test_auth_info_api_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let info = AuthInfo::Api {
            key: "sk-test".to_string(),
        };
        let json = serde_json::to_string(&info)?;
        let decoded: AuthInfo = serde_json::from_str(&json)?;
        match decoded {
            AuthInfo::Api { key } => assert_eq!(key, "sk-test"),
            _ => panic!("wrong variant"),
        }
        Ok(())
    }

    #[test]
    fn test_auth_info_oauth_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let info = AuthInfo::Oauth {
            refresh: Some("refresh_tok".to_string()),
            access: "access_tok".to_string(),
            expires: Some(9_999_999_000),
            account_id: Some("acct_1".to_string()),
            enterprise_url: None,
        };
        let json = serde_json::to_string(&info)?;
        let decoded: AuthInfo = serde_json::from_str(&json)?;
        match decoded {
            AuthInfo::Oauth {
                access,
                expires,
                account_id,
                ..
            } => {
                assert_eq!(access, "access_tok");
                assert_eq!(expires, Some(9_999_999_000));
                assert_eq!(account_id, Some("acct_1".to_string()));
            }
            _ => panic!("wrong variant"),
        }
        Ok(())
    }

    #[test]
    fn test_auth_info_wellknown_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let info = AuthInfo::WellKnown {
            key: "wk-key".to_string(),
            token: "wk-token".to_string(),
        };
        let json = serde_json::to_string(&info)?;
        let decoded: AuthInfo = serde_json::from_str(&json)?;
        match decoded {
            AuthInfo::WellKnown { key, token } => {
                assert_eq!(key, "wk-key");
                assert_eq!(token, "wk-token");
            }
            _ => panic!("wrong variant"),
        }
        Ok(())
    }

    #[test]
    fn test_store_read_write_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = temp_store(dir.path());

        store.set(
            "test-provider",
            AuthInfo::Api {
                key: "hello".to_string(),
            },
        )?;

        let got = store.get("test-provider")?;
        match got {
            Some(AuthInfo::Api { key }) => assert_eq!(key, "hello"),
            _ => panic!("expected Api variant"),
        }
        Ok(())
    }

    #[test]
    fn test_store_remove() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = temp_store(dir.path());

        store.set(
            "prov",
            AuthInfo::Api {
                key: "x".to_string(),
            },
        )?;
        store.remove("prov")?;
        assert!(store.get("prov")?.is_none());
        Ok(())
    }

    #[test]
    fn test_store_list_multiple() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = temp_store(dir.path());

        store.set(
            "a",
            AuthInfo::Api {
                key: "1".to_string(),
            },
        )?;
        store.set(
            "b",
            AuthInfo::Api {
                key: "2".to_string(),
            },
        )?;

        let list = store.list()?;
        assert_eq!(list.len(), 2);
        Ok(())
    }
}
