/// Error type for HTTP responses
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Unauthorized")]
    Unauthorized,
}

#[derive(serde::Serialize)]
pub struct ErrorBody {
    error: String,
    message: String,
}

impl axum::response::IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode;
        let (status, error_str) = match &self {
            ServerError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ServerError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ServerError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
            ServerError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let body = ErrorBody {
            error: error_str.to_owned(),
            message: self.to_string(),
        };
        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_serializes_correctly() {
        let body = ErrorBody {
            error: "not_found".to_owned(),
            message: "Not found: session-1".to_owned(),
        };
        let json = serde_json::to_string(&body).unwrap_or_default();
        assert!(json.contains("\"error\":\"not_found\""));
        assert!(json.contains("\"message\":\"Not found: session-1\""));
    }

    #[tokio::test]
    async fn server_error_not_found_converts_to_404() {
        use axum::response::IntoResponse;
        let err = ServerError::NotFound("session-1".to_owned());
        let response = err.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    }
}
