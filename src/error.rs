//! Error types.

use axum::response::{IntoResponse, Response};
use reqwest::StatusCode;

pub enum ApiError {
    Unauthorized,
    BadRequest(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("not found")]
    NotFound,

    #[error("invalid plugin name '{0}'")]
    InvalidName(String),

    #[error("invalid base url: {0}")]
    Url(#[from] url::ParseError),
}
