//! Encoding: turn a directory tree into a [`Manifest`] + deduplicated chunks.
//!
//! The encoder walks the tree, chunks each file with the streaming
//! [`Chunker`], stores each unique chunk in the [`ChunkStore`] (compressed
//! with Zstandard when the `zstd` feature is enabled), and records blob
//! references in the manifest. Identical bytes are stored once no matter how
//! many files or snapshots reference them.

use std::io::Read;
use std::path::Path;

use tracing::instrument;

use crate::chunking::{Chunker, MAX_CHUNK};
use crate::error::Result;
use crate::manifest::{FileEntry, FileKind, Manifest, ManifestRef, SourceInfo};

/// The encoding of chunks written by this crate build (see `zstd` feature).
pub const DEFAULT_COMPRESSION: crate::manifest::ChunkCompression =
    crate::manifest::ChunkCompression::Zstd;

/// Encode the directory at `root` into a manifest, storing chunks in `store`.
///
/// - `root` is the directory whose contents become the archive root.
/// - `source` describes where the content came from.
/// - `parent` optionally links this snapshot to the previous one (incremental).
///
/// On success the manifest is validated and returned; its
/// [`Manifest::digest`] is the archive checksum to persist.
#[instrument(skip(root, store), fields(root = %root.display()))]
pub async fn encode_dir(
    root: &Path,
    store: &dyn crate::store::ChunkStore,
    source: SourceInfo,
    parent: Option<ManifestRef>,
) -> Result<Manifest> {
    let mut manifest = Manifest::new(source);
    manifest.parent = parent;
    manifest.compression = DEFAULT_COMPRESSION;

    // Deduplication map: chunk hash → index in manifest.blobs.
    let mut blob_index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for entry in walkdir::WalkDir::new(root).min_depth(1).sort_by_file_name() {
        let entry = entry.map_err(|e| {
            let path = e
                .path()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.to_path_buf());
            let msg = e.to_string();
            crate::AahlError::Io {
                path,
                source: e
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other(msg)),
            }
        })?;

        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .expect("walkdir yields entries under root");
        let rel_str = crate::store::normalize_path(rel);

        let file_type = entry.file_type();
        let metadata = entry.metadata();

        if file_type.is_symlink() {
            let target = std::fs::read_link(path).map_err(|e| crate::AahlError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
            manifest.entries.push(FileEntry {
                path: rel_str,
                kind: FileKind::Symlink,
                mode: mode_of(&metadata),
                size: 0,
                blobs: Vec::new(),
                symlink_target: Some(target.to_string_lossy().into_owned()),
                modified: metadata.as_ref().ok().and_then(modified_of),
            });
            continue;
        }

        if file_type.is_dir() {
            manifest.entries.push(FileEntry {
                path: rel_str,
                kind: FileKind::Dir,
                mode: mode_of(&metadata),
                size: 0,
                blobs: Vec::new(),
                symlink_target: None,
                modified: metadata.as_ref().ok().and_then(modified_of),
            });
            continue;
        }

        if !file_type.is_file() {
            // Sockets, fifos, devices: skip; the format has no lossless
            // representation for them and they don't survive a clone anyway.
            tracing::debug!(path = %rel_str, "skipping non-regular file");
            continue;
        }

        // Regular file: stream-chunk and store.
        let mut file = std::fs::File::open(path).map_err(|e| crate::AahlError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let mut chunker = Chunker::default();
        let mut blobs = Vec::new();
        let mut size = 0u64;

        let mut buf = vec![0u8; MAX_CHUNK];
        loop {
            let n = file.read(&mut buf).map_err(|e| crate::AahlError::Io {
                path: path.to_path_buf(),
                source: e,
            })?;
            if n == 0 {
                break;
            }
            size += n as u64;
            for chunk in chunker.push(&buf[..n]) {
                push_chunk(&mut manifest, &mut blob_index, store, chunk, &mut blobs).await?;
            }
        }
        for chunk in chunker.finish() {
            push_chunk(&mut manifest, &mut blob_index, store, chunk, &mut blobs).await?;
        }

        manifest.entries.push(FileEntry {
            path: rel_str,
            kind: FileKind::File,
            mode: mode_of(&metadata),
            size,
            blobs,
            symlink_target: None,
            modified: metadata.as_ref().ok().and_then(modified_of),
        });
    }

    manifest.entries.sort_by(|a, b| a.path.cmp(&b.path));
    manifest.validate()?;
    Ok(manifest)
}

/// Store a chunk (dedup via the store) and record its index.
async fn push_chunk(
    manifest: &mut Manifest,
    blob_index: &mut std::collections::HashMap<String, usize>,
    store: &dyn crate::store::ChunkStore,
    chunk: crate::chunking::Chunk,
    file_blobs: &mut Vec<usize>,
) -> Result<()> {
    let idx = match blob_index.get(&chunk.hash) {
        Some(&idx) => idx,
        None => {
            let encoded = encode_chunk(&chunk.data, manifest.compression)?;
            if !store.has(&chunk.hash).await? {
                store.put(&chunk.hash, &encoded).await?;
            }
            let idx = manifest.blobs.len();
            manifest.blobs.push(chunk.hash.clone());
            blob_index.insert(chunk.hash, idx);
            idx
        }
    };
    file_blobs.push(idx);
    Ok(())
}

/// Encode a chunk for storage according to `compression`.
fn encode_chunk(data: &[u8], compression: crate::manifest::ChunkCompression) -> Result<Vec<u8>> {
    match compression {
        crate::manifest::ChunkCompression::None => Ok(data.to_vec()),
        crate::manifest::ChunkCompression::Zstd => compress_zstd(data),
    }
}

#[cfg(feature = "zstd")]
fn compress_zstd(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut out = zstd::stream::Encoder::new(Vec::new(), 3)
        .map_err(|e| crate::AahlError::Compression(e.to_string()))?;
    out.write_all(data)
        .map_err(|e| crate::AahlError::Compression(e.to_string()))?;
    out.finish()
        .map_err(|e| crate::AahlError::Compression(e.to_string()))
}

#[cfg(not(feature = "zstd"))]
fn compress_zstd(_data: &[u8]) -> Result<Vec<u8>> {
    Err(crate::AahlError::ZstdNotEnabled)
}

#[cfg(unix)]
fn mode_of(metadata: &std::result::Result<std::fs::Metadata, walkdir::Error>) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.as_ref().map(|m| m.mode()).unwrap_or(0)
}

#[cfg(not(unix))]
fn mode_of(_metadata: &std::result::Result<std::fs::Metadata, walkdir::Error>) -> u32 {
    0
}

#[cfg(unix)]
fn modified_of(metadata: &std::fs::Metadata) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;
    use std::os::unix::fs::MetadataExt;
    chrono::Utc
        .timestamp_opt(metadata.mtime(), metadata.mtime_nsec() as u32)
        .single()
}

#[cfg(not(unix))]
fn modified_of(_metadata: &std::fs::Metadata) -> Option<chrono::DateTime<chrono::Utc>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ChunkStore, MemoryStore};

    #[tokio::test]
    async fn encodes_a_small_tree() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("README.md"), "# hi\n").unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("README.md", dir.path().join("link")).unwrap();

        let store = MemoryStore::new();
        let manifest = encode_dir(
            dir.path(),
            &store,
            SourceInfo {
                kind: "test".into(),
                id: "demo".into(),
                reference: None,
                commit: None,
                captured_at: None,
            },
            None,
        )
        .await
        .unwrap();

        manifest.validate().unwrap();
        let files: Vec<&str> = manifest.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(files.contains(&"README.md"));
        assert!(files.contains(&"src"));
        assert!(files.contains(&"src/main.rs"));
        #[cfg(unix)]
        assert!(files.contains(&"link"));

        // Chunks were stored content-addressed.
        assert!(!manifest.blobs.is_empty());
        for blob in &manifest.blobs {
            assert!(store.has(blob).await.unwrap());
        }
    }

    #[tokio::test]
    async fn deduplicates_identical_files() {
        let dir = tempfile::tempdir().unwrap();
        let content = "x".repeat(100_000);
        std::fs::write(dir.path().join("a.bin"), &content).unwrap();
        std::fs::write(dir.path().join("b.bin"), &content).unwrap();

        let store = MemoryStore::new();
        let manifest = encode_dir(
            dir.path(),
            &store,
            SourceInfo {
                kind: "test".into(),
                id: "dup".into(),
                reference: None,
                commit: None,
                captured_at: None,
            },
            None,
        )
        .await
        .unwrap();

        // Two identical files → the second shares chunks, no new blobs.
        let a = manifest.entries.iter().find(|e| e.path == "a.bin").unwrap();
        let b = manifest.entries.iter().find(|e| e.path == "b.bin").unwrap();
        assert_eq!(a.blobs, b.blobs);
        assert!(manifest.blobs.len() < 100); // shared chunk count, not double
    }

    #[tokio::test]
    async fn incremental_parent_linked() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "content".repeat(10_000)).unwrap();

        let store = MemoryStore::new();
        let first = encode_dir(
            dir.path(),
            &store,
            SourceInfo {
                kind: "test".into(),
                id: "inc".into(),
                reference: None,
                commit: None,
                captured_at: None,
            },
            None,
        )
        .await
        .unwrap();
        let first_digest = first.digest().unwrap();
        let blob_count_after_first = store.used_blobs();

        std::fs::write(dir.path().join("g.txt"), "new file".repeat(5_000)).unwrap();
        let second = encode_dir(
            dir.path(),
            &store,
            SourceInfo {
                kind: "test".into(),
                id: "inc".into(),
                reference: None,
                commit: None,
                captured_at: None,
            },
            Some(crate::manifest::ManifestRef {
                digest: first_digest.clone(),
                created_at: first.created_at,
            }),
        )
        .await
        .unwrap();

        assert_eq!(second.parent.as_ref().unwrap().digest, first_digest);
        // Adding a small file adds few new chunks; shared content is reused.
        assert!(store.used_blobs() - blob_count_after_first < 20);
    }
}
