/// A trait for errors that carry a name and can be serialized to a JSON object.
pub trait NamedError: std::error::Error + Send + Sync {
    fn name(&self) -> &str;
    fn to_object(&self) -> serde_json::Value;
}

/// The primary application error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Provider error: {0}")]
    Provider(String),
    /// Catch-all variant — prefer a specific variant when possible.
    #[error("{0}")]
    Other(String),
}

impl NamedError for AppError {
    fn name(&self) -> &str {
        match self {
            Self::Io(_) => "IoError",
            Self::Json(_) => "JsonError",
            Self::Http(_) => "HttpError",
            Self::Database(_) => "DatabaseError",
            Self::Auth(_) => "AuthError",
            Self::Config(_) => "ConfigError",
            Self::Provider(_) => "ProviderError",
            Self::Other(_) => "Error",
        }
    }

    fn to_object(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name(),
            "message": self.to_string(),
        })
    }
}

/// Serialisable representation of an [`AppError`] suitable for wire transport
/// (e.g. JSON-RPC error responses).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorObject {
    pub name: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl From<AppError> for ErrorObject {
    fn from(err: AppError) -> Self {
        Self {
            name: err.name().to_owned(),
            message: err.to_string(),
            data: None,
        }
    }
}

/// Convenience alias — all fallible operations in avocode return this type.
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::{AppError, ErrorObject, NamedError};

    #[test]
    fn display_http_error() {
        let err = AppError::Http("503 Service Unavailable".to_owned());
        assert_eq!(err.to_string(), "HTTP error: 503 Service Unavailable");
    }

    #[test]
    fn display_auth_error() {
        let err = AppError::Auth("invalid token".to_owned());
        assert_eq!(err.to_string(), "Authentication error: invalid token");
    }

    #[test]
    fn display_other_error() {
        let err = AppError::Other("something went wrong".to_owned());
        assert_eq!(err.to_string(), "something went wrong");
    }

    #[test]
    fn name_returns_correct_variant_label() {
        assert_eq!(
            AppError::Config("bad value".to_owned()).name(),
            "ConfigError"
        );
        assert_eq!(
            AppError::Provider("rate limit".to_owned()).name(),
            "ProviderError"
        );
        assert_eq!(
            AppError::Database("conn failed".to_owned()).name(),
            "DatabaseError"
        );
    }

    #[test]
    fn to_object_contains_name_and_message() {
        let err = AppError::Http("404".to_owned());
        let obj = err.to_object();
        assert_eq!(obj["name"], "HttpError");
        assert_eq!(obj["message"], "HTTP error: 404");
    }

    #[test]
    fn from_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let app_err = AppError::from(io_err);
        assert!(matches!(app_err, AppError::Io(_)));
        assert_eq!(app_err.name(), "IoError");
    }

    #[test]
    fn from_serde_json_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json")
            .expect_err("should fail to parse");
        let app_err = AppError::from(json_err);
        assert!(matches!(app_err, AppError::Json(_)));
        assert_eq!(app_err.name(), "JsonError");
    }

    #[test]
    fn error_object_serialises_without_data_field() {
        let obj = ErrorObject::from(AppError::Auth("denied".to_owned()));
        let json = serde_json::to_string(&obj).expect("serialisation must succeed");
        assert!(json.contains("\"name\":\"AuthError\""));
        assert!(json.contains("\"message\":\"Authentication error: denied\""));
        // skip_serializing_if = "Option::is_none" must suppress the field when absent
        assert!(!json.contains("\"data\""));
    }

    #[test]
    fn error_object_serialises_with_data_field() {
        let obj = ErrorObject {
            name: "TestError".to_owned(),
            message: "test".to_owned(),
            data: Some(serde_json::json!({"key": "value"})),
        };
        let json = serde_json::to_string(&obj).expect("serialisation must succeed");
        assert!(json.contains("\"data\""));
        assert!(json.contains("\"key\":\"value\""));
    }

    #[test]
    fn error_object_roundtrips_through_json() {
        let original = ErrorObject::from(AppError::Config("missing key".to_owned()));
        let serialised = serde_json::to_string(&original).expect("serialise");
        let deserialised: ErrorObject = serde_json::from_str(&serialised).expect("deserialise");
        assert_eq!(deserialised.name, original.name);
        assert_eq!(deserialised.message, original.message);
        assert!(deserialised.data.is_none());
    }
}
