//! API error type and JSON envelope.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{json, Value};

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    pub details: Value,
}

impl ApiError {
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            details: Value::Null,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "A valid session token is required.",
        )
    }

    pub fn forbidden_origin() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "forbidden_origin",
            "This origin is not allowed to control the daemon.",
        )
    }

    pub fn conflict(revision: u64) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "revision_conflict",
            "The pipeline changed since this request was prepared.",
        )
        .with_details(json!({ "revision": revision }))
    }

    pub fn invalid_param(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_param", message)
    }

    pub fn invalid_preset(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_preset", message)
    }

    pub fn missing_asset(message: impl Into<String>, details: Value) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "missing_asset", message).with_details(details)
    }

    pub fn pipeline_error(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "pipeline_error", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "code": self.code,
                "message": self.message,
                "details": self.details,
            }
        });
        (self.status, axum::Json(body)).into_response()
    }
}

impl From<std::io::Error> for ApiError {
    fn from(value: std::io::Error) -> Self {
        ApiError::internal(value.to_string())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Wrap a value in the success envelope.
pub fn ok<T: serde::Serialize>(data: T) -> axum::Json<Value> {
    axum::Json(json!({ "data": data }))
}
