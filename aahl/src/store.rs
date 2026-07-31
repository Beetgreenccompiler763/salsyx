//! Content-addressed chunk storage.
//!
//! AAHL is storage-agnostic: [`ChunkStore`] is the only thing the format
//! talks to. A store is simply a key→bytes map where the key is the SHA-256
//! of the bytes (the content address). This makes deduplication automatic:
//! writing an already-present chunk is a no-op, and any two archives (or any
//! two repositories) that share a store share their identical chunks.
//!
//! Implementations provided here: [`MemoryStore`] (tests, ephemeral) and
//! [`FsStore`] (local disk, one file per chunk under a configurable root).
//! The backend wires R2/S3 via a thin adapter over its own [`Storage`] trait.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::{AahlError, Result};

/// Async content-addressed store for archive chunks.
///
/// Keys are the SHA-256 of the *uncompressed* chunk — the same digest that
/// appears in [`Manifest::blobs`] — so deduplication and integrity checks are
/// independent of how the chunk is encoded at rest. `put` stores the *encoded*
/// bytes under the caller-provided content address.
#[async_trait]
pub trait ChunkStore: Send + Sync {
    /// Persist the encoded form of a chunk under its content address `hash`.
    /// Must be idempotent.
    async fn put(&self, hash: &str, bytes: &[u8]) -> Result<()>;

    /// Read the encoded chunk stored under digest `hash`.
    async fn get(&self, hash: &str) -> Result<Vec<u8>>;

    /// Return true if a chunk with this digest already exists.
    async fn has(&self, hash: &str) -> Result<bool>;

    /// Human-readable name of the store (for diagnostics).
    fn name(&self) -> &'static str;
}

/// In-memory chunk store (tests, caches, single-process use).
#[derive(Debug, Default)]
pub struct MemoryStore {
    chunks: Mutex<HashMap<String, Vec<u8>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of unique chunks currently stored (for tests/diagnostics).
    pub fn used_blobs(&self) -> usize {
        self.chunks.lock().unwrap().len()
    }
}

#[async_trait]
impl ChunkStore for MemoryStore {
    async fn put(&self, hash: &str, bytes: &[u8]) -> Result<()> {
        self.chunks
            .lock()
            .unwrap()
            .entry(hash.to_string())
            .or_insert_with(|| bytes.to_vec());
        Ok(())
    }

    async fn get(&self, hash: &str) -> Result<Vec<u8>> {
        self.chunks
            .lock()
            .unwrap()
            .get(hash)
            .cloned()
            .ok_or_else(|| AahlError::ChunkNotFound(hash.to_string()))
    }

    async fn has(&self, hash: &str) -> Result<bool> {
        Ok(self.chunks.lock().unwrap().contains_key(hash))
    }

    fn name(&self) -> &'static str {
        "memory"
    }
}

/// Filesystem chunk store: one file per chunk under `root`, named by digest.
///
/// Layout is flat: `{root}/{hash}` (optionally sharded, e.g. `{root}/{hash[0..2]}/{hash}`,
/// to keep directory sizes small — controlled by [`FsStore::sharded`]).
#[derive(Debug)]
pub struct FsStore {
    root: PathBuf,
    sharded: bool,
}

impl FsStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            sharded: true,
        }
    }

    pub fn unsharded(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            sharded: false,
        }
    }

    fn path_for(&self, hash: &str) -> PathBuf {
        if self.sharded && hash.len() >= 4 {
            self.root.join(&hash[0..2]).join(&hash[2..4]).join(hash)
        } else {
            self.root.join(hash)
        }
    }
}

#[async_trait]
impl ChunkStore for FsStore {
    async fn put(&self, hash: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path_for(hash);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AahlError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        std::fs::write(&path, bytes).map_err(|e| AahlError::Io {
            path: path.clone(),
            source: e,
        })?;
        Ok(())
    }

    async fn get(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.path_for(hash);
        std::fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AahlError::ChunkNotFound(hash.to_string()),
            _ => AahlError::Io { path, source: e },
        })
    }

    async fn has(&self, hash: &str) -> Result<bool> {
        Ok(self.path_for(hash).exists())
    }

    fn name(&self) -> &'static str {
        "fs"
    }
}

/// Convert `path` to a normalized, slash-separated archive key with no
/// leading slash or `.`/`..` segments.
pub fn normalize_path(path: &Path) -> String {
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    parts.join("/")
}
