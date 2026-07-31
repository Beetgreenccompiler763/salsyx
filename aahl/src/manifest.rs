//! The AAHL manifest: a versioned, checksummed, optionally signed index of
//! an archive.
//!
//! The manifest is the "header" of an archive. It is small (it references
//! chunks by digest rather than containing file data) and is meant to be
//! stored beside the chunk store — in the backend this is the row in the
//! `archives` table plus the manifest blob itself.
//!
//! # Integrity model
//!
//! - Every chunk is content-addressed (SHA-256), so referencing a blob by
//!   hash both identifies it and pins its contents.
//! - [`Manifest::digest`] returns the SHA-256 of the canonical JSON bytes,
//!   which the backend stores as the archive checksum.
//! - With the `sign` feature, [`Manifest::sign`]/[`Manifest::verify`] add an
//!   Ed25519 signature over the canonical bytes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{AahlError, Result};
use crate::FORMAT_VERSION;

/// Where the archived content came from (free-form, human- and machine-readable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// e.g. `github`, `software-heritage`, `local`.
    pub kind: String,
    /// Human-readable identifier, e.g. `owner/repo` or a git remote URL.
    pub id: String,
    /// Branch / ref captured, if known.
    pub reference: Option<String>,
    /// Git commit captured, if known.
    pub commit: Option<String>,
    /// RFC 3339 timestamp of the snapshot at the source.
    pub captured_at: Option<DateTime<Utc>>,
}

/// A link to the previous snapshot of the same repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestRef {
    /// Digest of the parent manifest (see [`Manifest::digest`]).
    pub digest: String,
    /// When the parent snapshot was created.
    pub created_at: DateTime<Utc>,
}

/// How chunks are encoded inside the store.
///
/// The chunk *digest* is always over the uncompressed bytes, so deduplication
/// is unaffected by the encoding choice. This field tells the decoder how to
/// decode each stored blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChunkCompression {
    /// Chunks stored verbatim.
    #[default]
    None,
    /// Chunks compressed with Zstandard before storage.
    Zstd,
}

/// Directory / file / symlink kind of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    File,
    Dir,
    Symlink,
}

/// One entry in the archived tree.
///
/// File data is stored as a list of chunk indexes into `Manifest::blobs`.
/// Splitting a file across chunks lets the decoder stream it and lets the
/// encoder deduplicate common sub-ranges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Slash-separated path relative to the archive root (no leading `/`).
    pub path: String,
    pub kind: FileKind,
    /// Unix permission bits (e.g. `0o100644`); `0` when unknown.
    pub mode: u32,
    /// Uncompressed size in bytes (files only).
    pub size: u64,
    /// Chunk indexes into `Manifest::blobs` (files only, in order).
    pub blobs: Vec<usize>,
    /// Symlink target (symlinks only).
    pub symlink_target: Option<String>,
    /// Last-modified time if known.
    pub modified: Option<DateTime<Utc>>,
}

/// The full AAHL archive index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub created_at: DateTime<Utc>,
    pub source: SourceInfo,
    /// Reference to the previous snapshot (incremental archives).
    pub parent: Option<ManifestRef>,
    /// Unique chunk digests referenced by entries (deduplicated).
    pub blobs: Vec<String>,
    /// Encoding used for the chunks in `blobs`.
    #[serde(default)]
    pub compression: ChunkCompression,
    /// Tree entries (directories, files, symlinks).
    pub entries: Vec<FileEntry>,
    /// Optional Ed25519 signature over the canonical manifest bytes.
    /// Present only when signed at encode time.
    pub signature: Option<String>,
}

impl Manifest {
    /// Create a new manifest with the current format version and no parent.
    pub fn new(source: SourceInfo) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            created_at: Utc::now(),
            source,
            parent: None,
            blobs: Vec::new(),
            compression: ChunkCompression::None,
            entries: Vec::new(),
            signature: None,
        }
    }

    /// Canonical JSON bytes for this manifest (with the signature field
    /// zeroed so the digest is stable regardless of signing).
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        let mut clone = self.clone();
        clone.signature = None;
        Ok(serde_json::to_vec(&clone)?)
    }

    /// SHA-256 digest of the canonical manifest bytes — the archive checksum.
    pub fn digest(&self) -> Result<String> {
        Ok(crate::sha256_hex(&self.canonical_bytes()?))
    }

    /// Validate structure invariants. Called by both encode and decode.
    pub fn validate(&self) -> Result<()> {
        if self.format_version != FORMAT_VERSION {
            return Err(AahlError::UnsupportedVersion(
                self.format_version,
                FORMAT_VERSION,
            ));
        }
        if self.signature.is_some() && cfg!(not(feature = "sign")) {
            return Err(AahlError::SignatureInvalid);
        }
        for entry in &self.entries {
            if entry.kind == FileKind::File {
                let referenced = entry.blobs.iter().cloned();
                for idx in referenced {
                    if idx >= self.blobs.len() {
                        return Err(AahlError::BlobIndexOutOfRange(idx));
                    }
                }
            }
            if entry.path.is_empty()
                || entry.path.contains("\\")
                || entry.path.starts_with('/')
                || entry.path.split('/').any(|s| s == ".." || s == ".")
            {
                return Err(AahlError::PathTraversal(entry.path.clone()));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Optional Ed25519 signing (feature `sign`)
// ---------------------------------------------------------------------------

/// Result of verifying a manifest signature: success carries the signer's
/// public key bytes so callers can decide whether they trust that key.
#[cfg(feature = "sign")]
pub fn verify_manifest(
    manifest: &Manifest,
    expected_public_key: Option<&ed25519_dalek::VerifyingKey>,
) -> Result<()> {
    let sig_hex = manifest
        .signature
        .as_ref()
        .ok_or(AahlError::SignatureInvalid)?;
    let sig_bytes = hex::decode(sig_hex).map_err(|_| AahlError::SignatureInvalid)?;
    let signature = ed25519_dalek::Signature::from_slice(&sig_bytes)
        .map_err(|_| AahlError::SignatureInvalid)?;

    let public_key = match expected_public_key {
        Some(key) => *key,
        // Without an expected key we can only check that some valid key
        // verifies — so we can't do anything useful here; callers MUST pass
        // the key they trust.
        None => return Err(AahlError::SignatureInvalid),
    };

    let canonical = manifest.canonical_bytes()?;
    public_key
        .verify_strict(&canonical, &signature)
        .map_err(|_| AahlError::SignatureInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Manifest {
        Manifest {
            format_version: FORMAT_VERSION,
            created_at: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            source: SourceInfo {
                kind: "github".into(),
                id: "lmdelm-dev/salsyx".into(),
                reference: Some("main".into()),
                commit: Some("abc123".into()),
                captured_at: None,
            },
            parent: None,
            blobs: vec!["deadbeef".into()],
            compression: crate::ChunkCompression::None,
            entries: vec![FileEntry {
                path: "README.md".into(),
                kind: FileKind::File,
                mode: 0o100644,
                size: 12,
                blobs: vec![0],
                symlink_target: None,
                modified: None,
            }],
            signature: None,
        }
    }

    #[test]
    fn digest_is_stable_and_hex() {
        let a = sample();
        let b = sample();
        assert_eq!(a.digest().unwrap(), b.digest().unwrap());
        assert_eq!(a.digest().unwrap().len(), 64);
    }

    #[test]
    fn signature_field_does_not_change_digest() {
        let mut a = sample();
        a.signature = Some("x".into());
        assert_eq!(a.digest().unwrap(), sample().digest().unwrap());
    }

    #[test]
    fn validate_rejects_bad_format_version() {
        let mut m = sample();
        m.format_version = 99;
        assert!(m.validate().is_err());
    }

    #[test]
    fn validate_rejects_traversal() {
        let mut m = sample();
        m.entries.push(FileEntry {
            path: "../evil".into(),
            kind: FileKind::File,
            mode: 0,
            size: 0,
            blobs: vec![],
            symlink_target: None,
            modified: None,
        });
        assert!(m.validate().is_err());
    }
}
