//! AAHL integration.
//!
//! [`StorageChunkStore`] adapts the backend [`Storage`] abstraction to the
//! AAHL [`ChunkStore`] contract, so the crawler can persist content-addressed
//! archive chunks through the same local/R2 layer as everything else. Chunks
//! are addressed by the digest of their *uncompressed* bytes (per the AAHL
//! format) and stored under the `aahl/` object-key namespace.

use async_trait::async_trait;

use crate::storage::Storage;

/// Key namespace for AAHL chunks (addresses the digest of the uncompressed
/// chunk, matching [`aahl::Manifest::blobs`]).
pub const CHUNK_PREFIX: &str = "aahl";

/// AAHL chunk store backed by a [`Storage`] provider.
///
/// Stored bytes are the *encoded* (compressed) chunk; the content address
/// `hash` is the digest of the uncompressed data. Integrity of the
/// uncompressed bytes is re-verified by the decoder during reconstruction, so
/// `get` does not ask the storage layer to checksum against `hash`.
pub struct StorageChunkStore<'a> {
    storage: &'a dyn Storage,
}

impl<'a> StorageChunkStore<'a> {
    pub fn new(storage: &'a dyn Storage) -> Self {
        Self { storage }
    }

    fn key(&self, hash: &str) -> String {
        format!("{CHUNK_PREFIX}/{hash}")
    }
}

#[async_trait]
impl<'a> aahl::store::ChunkStore for StorageChunkStore<'a> {
    async fn put(&self, hash: &str, bytes: &[u8]) -> aahl::Result<()> {
        self.storage
            .put(&self.key(hash), bytes)
            .await
            .map_err(|e| aahl::AahlError::Store(format!("put {}: {e}", self.key(hash))))?;
        Ok(())
    }

    async fn get(&self, hash: &str) -> aahl::Result<Vec<u8>> {
        let blob = self
            .storage
            .get(&self.key(hash), None)
            .await
            .map_err(|e| aahl::AahlError::Store(format!("get {}: {e}", self.key(hash))))?;
        Ok(blob.bytes)
    }

    async fn has(&self, hash: &str) -> aahl::Result<bool> {
        self.storage
            .exists(&self.key(hash))
            .await
            .map_err(|e| aahl::AahlError::Store(format!("exists {}: {e}", self.key(hash))))
    }

    fn name(&self) -> &'static str {
        self.storage.provider_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StorageConfig;
    use crate::storage::from_config;

    #[tokio::test]
    async fn encode_decode_roundtrip_via_storage() {
        // Build a small source tree and a local storage backend.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        let store_root = tmp.path().join("store");

        std::fs::create_dir_all(src.join("lib")).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("README.md"), "# hello\n\nworld\n").unwrap();
        std::fs::write(
            src.join("lib").join("mod.rs"),
            "pub fn answer() -> u32 { 42 }\n",
        )
        .unwrap();
        std::fs::create_dir_all(src.join("lib").join("nested")).unwrap();
        std::fs::write(
            src.join("lib").join("nested").join("data.txt"),
            "x".repeat(4096),
        )
        .unwrap();

        let config = StorageConfig {
            provider: "local".to_string(),
            providers: Vec::new(),
            local_root: store_root.to_string_lossy().into_owned(),
            r2_endpoint: None,
            r2_bucket: None,
            r2_account_id: None,
            r2_access_key_id: None,
            r2_secret_access_key: None,
            r2_public_base_url: None,
            s3_endpoint: None,
            s3_bucket: None,
            s3_region: "auto".to_string(),
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_public_base_url: None,
        };
        let storage = from_config(&config).unwrap();

        let store = StorageChunkStore::new(storage.as_ref());
        let source = aahl::SourceInfo {
            kind: "test".to_string(),
            id: "local/test".to_string(),
            reference: None,
            commit: None,
            captured_at: None,
        };

        let manifest = aahl::encode::encode_dir(&src, &store, source, None)
            .await
            .expect("encode");
        assert!(!manifest.blobs.is_empty(), "expected chunked blobs");

        aahl::decode::extract(&manifest, &store, &dst)
            .await
            .expect("decode");

        let orig = std::fs::read(src.join("lib").join("nested").join("data.txt")).unwrap();
        let got = std::fs::read(dst.join("lib").join("nested").join("data.txt")).unwrap();
        assert_eq!(got, orig);

        // Chunk must be addressable by its uncompressed digest in storage.
        let first = &manifest.blobs[0];
        assert!(storage
            .exists(&format!("{CHUNK_PREFIX}/{first}"))
            .await
            .unwrap());
    }
}
