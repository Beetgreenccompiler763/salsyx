//! Canonical error type for the Salsyx domain layer.

use thiserror::Error;

/// Errors that can occur across the platform's domain logic.
///
/// These map onto HTTP status codes in the API layer, but the error type
/// itself is transport-agnostic so the crawler and storage layers can reuse
/// it without depending on a web framework.
#[derive(Debug, Error)]
pub enum SalsyxError {
    /// The requested resource was not found on the source *and* we hold no
    /// archive — the "this repository has not been archived" case.
    #[error("repository `{0}` does not exist on GitHub and has not been archived")]
    NotFound(String),

    /// The resource exists locally but has no archive yet.
    #[error("repository `{0}` exists but has no archive yet")]
    NotArchived(String),

    /// Upstream (GitHub) API rate limited or transiently unavailable.
    #[error("upstream service unavailable: {0}")]
    UpstreamUnavailable(String),

    /// A record failed an integrity check (checksum mismatch, missing blob).
    #[error("integrity check failed for {0}")]
    Integrity(String),

    /// The caller supplied invalid input.
    #[error("invalid input: {0}")]
    Validation(String),

    /// A database error occurred.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Any other infrastructure failure.
    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, SalsyxError>;
