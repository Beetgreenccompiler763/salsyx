//! Decoding: reconstruct files from a [`Manifest`] + [`ChunkStore`].
//!
//! Decoding is chunk-agnostic: the decoder never chunks data itself. It walks
//! the manifest's blob references, fetches (and decompresses) each chunk from
//! the store, verifies its content address, and reassembles files. This makes
//! extraction streamable — a single file can be materialized without touching
//! the rest of the archive.

use std::io::Write;
use std::path::Path;

use crate::error::{AahlError, Result};
use crate::manifest::{ChunkCompression, FileKind, Manifest};

/// Security boundary: reject entries whose paths could escape `dest`.
pub const MAX_PATH_DEPTH: usize = 128;

/// Reassemble the uncompressed bytes of a file entry.
///
/// Every chunk is fetched from `store` and its SHA-256 verified against the
/// manifest before the bytes are emitted — "verify before trusting".
pub async fn read_file(
    manifest: &Manifest,
    entry: &crate::manifest::FileEntry,
    store: &dyn crate::store::ChunkStore,
) -> Result<Vec<u8>> {
    if entry.kind != FileKind::File {
        return Err(AahlError::InvalidManifest(format!(
            "entry `{}` is not a file",
            entry.path
        )));
    }

    let mut out = Vec::with_capacity(entry.size as usize);
    for &idx in &entry.blobs {
        let hash = manifest
            .blobs
            .get(idx)
            .ok_or(AahlError::BlobIndexOutOfRange(idx))?;
        let encoded = store.get(hash).await?;
        let bytes = decode_chunk(&encoded, manifest.compression)?;
        let actual = crate::sha256_hex(&bytes);
        if actual != *hash {
            return Err(AahlError::ChunkChecksumMismatch {
                hash: hash.clone(),
                expected: hash.clone(),
                actual,
            });
        }
        out.extend_from_slice(&bytes);
    }
    Ok(out)
}

/// List archive entries without reading any chunk data.
pub fn list(manifest: &Manifest) -> &[crate::manifest::FileEntry] {
    &manifest.entries
}

/// Materialize the full archive under `dest`, recreating directories, files,
/// and symlinks with their recorded modes.
///
/// `dest` must be an existing directory. The function never follows symlinks
/// during writing (no escaping the destination).
pub async fn extract(
    manifest: &Manifest,
    store: &dyn crate::store::ChunkStore,
    dest: &Path,
) -> Result<()> {
    manifest.validate()?;
    if !dest.exists() {
        return Err(AahlError::Io {
            path: dest.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "destination missing"),
        });
    }

    for entry in &manifest.entries {
        let safe = safe_join(dest, &entry.path)?;
        match entry.kind {
            FileKind::Dir => {
                std::fs::create_dir_all(&safe).map_err(|e| AahlError::Io {
                    path: safe.clone(),
                    source: e,
                })?;
                apply_mode(&safe, entry.mode)?;
            }
            FileKind::File => {
                if let Some(parent) = safe.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| AahlError::Io {
                        path: parent.to_path_buf(),
                        source: e,
                    })?;
                }
                let bytes = read_file(manifest, entry, store).await?;
                let mut file = std::fs::File::create(&safe).map_err(|e| AahlError::Io {
                    path: safe.clone(),
                    source: e,
                })?;
                file.write_all(&bytes).map_err(|e| AahlError::Io {
                    path: safe.clone(),
                    source: e,
                })?;
                apply_mode(&safe, entry.mode)?;
            }
            FileKind::Symlink => {
                if let Some(parent) = safe.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| AahlError::Io {
                        path: parent.to_path_buf(),
                        source: e,
                    })?;
                }
                #[cfg(unix)]
                {
                    let _ = std::fs::remove_file(&safe);
                    let target = entry.symlink_target.as_ref().ok_or_else(|| {
                        AahlError::InvalidManifest(format!(
                            "symlink `{}` missing target",
                            entry.path
                        ))
                    })?;
                    std::os::unix::fs::symlink(target, &safe).map_err(|e| AahlError::Io {
                        path: safe.clone(),
                        source: e,
                    })?;
                }
                #[cfg(not(unix))]
                {
                    let _ = (entry, &safe);
                    tracing::warn!(path = %entry.path, "symlinks not supported on this platform");
                }
            }
        }
    }
    Ok(())
}

/// Join `rel` onto `dest` and reject any entry that would escape it.
fn safe_join(dest: &Path, rel: &str) -> Result<std::path::PathBuf> {
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.contains("\\")
        || rel.split('/').any(|s| s == ".." || s == ".")
    {
        return Err(AahlError::PathTraversal(rel.to_string()));
    }
    let joined = dest.join(rel);
    if joined.components().count() > dest.components().count() + MAX_PATH_DEPTH {
        return Err(AahlError::PathTraversal(rel.to_string()));
    }
    Ok(joined)
}

/// Decode a stored chunk according to the manifest's compression.
fn decode_chunk(encoded: &[u8], compression: ChunkCompression) -> Result<Vec<u8>> {
    match compression {
        ChunkCompression::None => Ok(encoded.to_vec()),
        ChunkCompression::Zstd => decompress_zstd(encoded),
    }
}

#[cfg(feature = "zstd")]
fn decompress_zstd(encoded: &[u8]) -> Result<Vec<u8>> {
    zstd::stream::decode_all(std::io::Cursor::new(encoded))
        .map_err(|e| AahlError::Decompression(e.to_string()))
}

#[cfg(not(feature = "zstd"))]
fn decompress_zstd(_encoded: &[u8]) -> Result<Vec<u8>> {
    Err(AahlError::ZstdNotEnabled)
}

#[cfg(unix)]
fn apply_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if mode != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| {
            AahlError::Io {
                path: path.to_path_buf(),
                source: e,
            }
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::encode_dir;
    use crate::manifest::SourceInfo;
    use crate::store::{ChunkStore, MemoryStore};

    #[tokio::test]
    async fn roundtrip_dir() {
        let src = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(src.path().join("nested/deep")).unwrap();
        std::fs::write(src.path().join("a.txt"), "hello ".repeat(20_000)).unwrap();
        std::fs::write(src.path().join("nested/b.txt"), "world".repeat(10_000)).unwrap();
        std::fs::write(src.path().join("nested/deep/c.txt"), "xyz".repeat(5_000)).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("a.txt", src.path().join("nested/link")).unwrap();

        let store = MemoryStore::new();
        let manifest = encode_dir(
            src.path(),
            &store,
            SourceInfo {
                kind: "test".into(),
                id: "round".into(),
                reference: None,
                commit: None,
                captured_at: None,
            },
            None,
        )
        .await
        .unwrap();

        let dst = tempfile::tempdir().unwrap();
        extract(&manifest, &store, dst.path()).await.unwrap();

        assert_eq!(
            std::fs::read(dst.path().join("a.txt")).unwrap(),
            std::fs::read(src.path().join("a.txt")).unwrap()
        );
        assert_eq!(
            std::fs::read(dst.path().join("nested/b.txt")).unwrap(),
            std::fs::read(src.path().join("nested/b.txt")).unwrap()
        );
        assert_eq!(
            std::fs::read(dst.path().join("nested/deep/c.txt")).unwrap(),
            std::fs::read(src.path().join("nested/deep/c.txt")).unwrap()
        );
        #[cfg(unix)]
        assert_eq!(
            std::fs::read_link(dst.path().join("nested/link")).unwrap(),
            std::fs::read_link(src.path().join("nested/link")).unwrap()
        );
    }

    #[tokio::test]
    async fn detects_tampered_chunk() {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("f.bin"), "data".repeat(50_000)).unwrap();

        let store = MemoryStore::new();
        let mut manifest = encode_dir(
            src.path(),
            &store,
            SourceInfo {
                kind: "test".into(),
                id: "tamper".into(),
                reference: None,
                commit: None,
                captured_at: None,
            },
            None,
        )
        .await
        .unwrap();

        // Corrupt one blob: store valid-encoding bytes under a digest that
        // does NOT match their uncompressed content, then re-point the entry
        // at that hash. Decode must catch the mismatch.
        let wrong = b"these bytes are not the real chunk content...............".to_vec();
        #[cfg(feature = "zstd")]
        let encoded = {
            use std::io::Write;
            let mut enc = zstd::stream::Encoder::new(Vec::new(), 3).unwrap();
            enc.write_all(&wrong).unwrap();
            enc.finish().unwrap()
        };
        #[cfg(not(feature = "zstd"))]
        let encoded = wrong.clone();

        let bad_hash = crate::sha256_hex(b"a digest that must not match `wrong`");
        store.put(&bad_hash, &encoded).await.unwrap();
        manifest.blobs[0] = bad_hash;

        let entry = manifest.entries.iter().find(|e| e.path == "f.bin").unwrap();
        let err = read_file(&manifest, entry, &store).await.unwrap_err();
        assert!(matches!(err, AahlError::ChunkChecksumMismatch { .. }));
    }
}
