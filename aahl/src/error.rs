//! Error types for the AAHL crate.

use std::path::PathBuf;

/// Errors produced by AAHL.
#[derive(Debug, thiserror::Error)]
pub enum AahlError {
    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("chunk not found in store: {0}")]
    ChunkNotFound(String),
    #[error("store transport error: {0}")]
    Store(String),
    #[error("chunk checksum mismatch for {hash}: expected {expected}, got {actual}")]
    ChunkChecksumMismatch {
        hash: String,
        expected: String,
        actual: String,
    },
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("unsupported format version {0} (supported: {1})")]
    UnsupportedVersion(u32, u32),
    #[error("manifest signature invalid or missing")]
    SignatureInvalid,
    #[error("path traversal detected in manifest entry: {0}")]
    PathTraversal(String),
    #[error("entry blob index out of range: {0}")]
    BlobIndexOutOfRange(usize),
    #[error("compression error: {0}")]
    Compression(String),
    #[error("decompression error: {0}")]
    Decompression(String),
    #[error("zstd support not compiled in (build with the `zstd` feature)")]
    ZstdNotEnabled,
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, AahlError>;
