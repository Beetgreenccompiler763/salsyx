//! Internal event types used for the future queue / event bus.
//!
//! The crawler workers and the API server communicate through a queue of
//! these events. Today the queue is an in-memory channel; the type is what
//! makes it possible to swap in Redis/Postgres LISTEN-NOTIFY later without
//! touching producers or consumers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unit of work emitted into the archive pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    /// Schedule a repository (identified by full name) for refresh.
    /// The crawler decides whether a refresh/archive is actually needed
    /// (avoids duplicate archives via the repo's `last_checked_at`).
    CheckRepository { full_name: String },
    /// Archive the repository with the given id at its current state.
    ArchiveRepository { repository_id: Uuid },
    /// Re-verify the integrity (checksum) of an existing archive.
    VerifyArchive { archive_id: Uuid },
}
