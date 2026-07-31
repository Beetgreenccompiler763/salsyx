//! Archive-centric domain types.
//!
//! An *archive* is a point-in-time snapshot of a repository's contents
//! (typically a bare git clone plus metadata) stored in the object store.
//! A repository can have many archives over time; each archive is immutable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Lifecycle state of an archived repository.
///
/// Serialized as snake_case for a stable wire contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ArchiveStatus {
    /// The archive job has been queued but not started.
    #[default]
    Pending,
    /// The repository contents are being fetched from the source.
    Fetching,
    /// Contents have been fetched and are being compressed/deduped.
    Processing,
    /// The archive is complete and available for download/browse.
    Archived,
    /// The archive finished but its integrity checksum could not be verified.
    VerificationFailed,
    /// Something went wrong; inspect `error_message` on the record.
    Failed,
}

/// How the archived bytes were compressed.
///
/// The pipeline currently produces git bundles (native git compression) and
/// is designed so a future custom long-term format can be added here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionMethod {
    /// Standard ZIP (`.zip`) produced from the checkout working tree.
    Zip,
    /// Native git bundle (`.bundle`) — a complete ref + object pack.
    GitBundle,
    /// Plain uncompressed tar (for experimentation / direct rsync-style use).
    Tar,
    /// Future custom format optimized for long-term archival storage.
    Custom,
}

impl CompressionMethod {
    /// Canonical file extension for this compression method.
    pub fn extension(&self) -> &'static str {
        match self {
            CompressionMethod::Zip => "zip",
            CompressionMethod::GitBundle => "bundle",
            CompressionMethod::Tar => "tar",
            CompressionMethod::Custom => "arc",
        }
    }
}

/// Where the archived blob lives in the object store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageLocation {
    /// Storage provider/bucket namespace, e.g. `r2:archivehub`.
    pub provider: String,
    /// Object key inside the provider's bucket.
    pub key: String,
}

/// A single immutable point-in-time archive of a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Archive {
    pub id: Uuid,
    pub repository_id: Uuid,
    /// Git commit / ref that was captured, if known.
    pub commit_ref: Option<String>,
    /// Git commit count at archive time.
    pub commit_count: Option<i64>,
    /// SHA-256 hex digest of the stored object. Enables integrity checks.
    pub checksum: String,
    /// Number of bytes stored.
    pub size_bytes: i64,
    pub storage: StorageLocation,
    pub compression: CompressionMethod,
    pub status: ArchiveStatus,
    /// When the repository was observed deleted on the source, if applicable.
    pub deleted_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub archived_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
