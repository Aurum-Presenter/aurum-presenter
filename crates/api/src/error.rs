//! A failure that is a normal outcome of a request rather than a defect.
//!
//! These carry a stable machine-readable code alongside the status, so the client can branch on
//! the reason without parsing prose — the sync engine's retry logic classifies by both.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub details: Option<Value>,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError {
            status,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> ApiError {
        self.details = Some(details);
        self
    }

    pub fn with_code(mut self, code: &'static str) -> ApiError {
        self.code = code;
        self
    }

    pub fn bad_request(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn unauthorized(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    pub fn forbidden(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn not_found(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn conflict(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::CONFLICT, "conflict", message)
    }

    pub fn unprocessable(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "unprocessable", message)
    }

    pub fn validation(field: &str, message: impl Into<String>) -> ApiError {
        ApiError::unprocessable(message)
            .with_code("validation_failed")
            .with_details(json!({ "field": field }))
    }

    pub fn too_many_requests(message: impl Into<String>, retry_after: i64) -> ApiError {
        ApiError::new(StatusCode::TOO_MANY_REQUESTS, "too_many_requests", message)
            .with_details(json!({ "retry_after": retry_after }))
    }

    /// A defect, not an outcome. Logged with its cause; the client is told nothing about it,
    /// because anything specific here is a detail of how the server is built.
    pub fn internal(context: &str, cause: impl std::fmt::Display) -> ApiError {
        tracing::error!("{context}: {cause}");

        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Something went wrong.",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut error = json!({ "code": self.code, "message": self.message });

        if let Some(details) = self.details {
            error["details"] = details;
        }

        (self.status, Json(json!({ "error": error }))).into_response()
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for ApiError {}

impl From<rusqlite::Error> for ApiError {
    fn from(error: rusqlite::Error) -> ApiError {
        ApiError::internal("database", error)
    }
}
