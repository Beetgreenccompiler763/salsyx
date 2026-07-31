//! AAHL — ArchiveHub Archive Layer.
//!
//! A lossless, chunked, content-addressed archive format for long-term
//! software preservation. See the crate README for the full design.
//!
//! # Architecture
//!
//! - [`encode`] turns a directory into a [`Manifest`] plus chunks written to
//!   a [`store::ChunkStore`].
//! - [`decode`] reconstructs a directory from a [`Manifest`] + [`store::ChunkStore`].
//! - [`chunking`] implements content-defined chunking (buzhash).
//! - [`manifest`] defines the versioned, checksummed, optionally signed manifest.
//!
//! The chunk store is the only external dependency: provide a
//! [`store::ChunkStore`] implementation backed by a filesystem, an S3/R2
//! bucket, or anything else.

pub mod chunking;
pub mod decode;
pub mod encode;
pub mod error;
pub mod manifest;
pub mod store;

pub use error::{AahlError, Result};
pub use manifest::{ChunkCompression, FileEntry, FileKind, Manifest, ManifestRef, SourceInfo};

use sha2::{Digest, Sha256};

/// Compute the lowercase hex SHA-256 of `data` — the content address used
/// throughout AAHL.
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// The on-disk format version written to every manifest.
pub const FORMAT_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_is_hex_sha256() {
        assert_eq!(sha256_hex(b"").len(), 64);
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
