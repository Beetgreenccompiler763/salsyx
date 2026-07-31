//! API error handling.
//!
//! Every handler returns `Result<T, AppError>`; `AppError` knows how to
//! render itself as a consistent JSON error envelope. A middleware-level
//! catch-all converts panics into 500 responses as well.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Standard JSON error envelope returned by every endpoint.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Machine-readable error code, e.g. `not_found`, `rate_limited`.
    pub code: &'static str,
    pub message: String,
    /// RFC 7807 style detail — the human-readable explanation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The application-wide error type.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("repository `{full_name}` was not found on GitHub and has no archive")]
    NotFound { full_name: String },

    #[error("repository `{full_name}` exists but has no archive yet")]
    NotArchived { full_name: String },

    #[error("archive `{id}` has been deleted")]
    Gone { id: String },

    #[error("upstream service unavailable: {0}")]
    Upstream(String),

    #[error("invalid input: {0}")]
    Validation(String),

    #[error("rate limit exceeded")]
    RateLimited,

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
            AppError::NotArchived { .. } => StatusCode::NOT_FOUND,
            AppError::Gone { .. } => StatusCode::GONE,
            AppError::Upstream(_) => StatusCode::BAD_GATEWAY,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Migration(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            AppError::NotFound { .. } => "not_found",
            AppError::NotArchived { .. } => "not_archived",
            AppError::Gone { .. } => "gone",
            AppError::Upstream(_) => "upstream_unavailable",
            AppError::Validation(_) => "invalid_input",
            AppError::RateLimited => "rate_limited",
            AppError::Database(_) => "internal_error",
            AppError::Migration(_) => "internal_error",
            AppError::Internal(_) => "internal_error",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let message = self.to_string();

        tracing::warn!(status = %status, error = %message, "request failed");

        let body = ErrorBody {
            code: self.code(),
            message: message.clone(),
            detail: None,
        };

        (status, Json(body)).into_response()
    }
}

impl From<salsyx_shared::error::SalsyxError> for AppError {
    fn from(err: salsyx_shared::error::SalsyxError) -> Self {
        match err {
            salsyx_shared::error::SalsyxError::NotFound(full_name) => {
                AppError::NotFound { full_name }
            }
            salsyx_shared::error::SalsyxError::NotArchived(full_name) => {
                AppError::NotArchived { full_name }
            }
            salsyx_shared::error::SalsyxError::UpstreamUnavailable(msg) => AppError::Upstream(msg),
            salsyx_shared::error::SalsyxError::Integrity(msg) => {
                AppError::Internal(anyhow::anyhow!("integrity: {msg}"))
            }
            salsyx_shared::error::SalsyxError::Validation(msg) => AppError::Validation(msg),
            salsyx_shared::error::SalsyxError::Database(e) => AppError::Database(e),
            salsyx_shared::error::SalsyxError::Internal(e) => AppError::Internal(e),
        }
    }
}
