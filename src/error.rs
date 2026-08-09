use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn bad(message: impl Into<String>) -> Self { Self::BadRequest(message.into()) }
    pub fn conflict(message: impl Into<String>) -> Self { Self::Conflict(message.into()) }
    pub fn unauthorized(message: impl Into<String>) -> Self { Self::Unauthorized(message.into()) }

    pub fn category(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "configuration_or_api",
            Self::Conflict(_) => "safety_conflict",
            Self::Unauthorized(_) => "permission",
            Self::Internal(_) => "network_or_internal",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(json!({"ok": false, "error": self.to_string()}));
        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self { Self::Internal(value.into()) }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self { Self::Internal(value.into()) }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self { Self::Internal(value.into()) }
}
