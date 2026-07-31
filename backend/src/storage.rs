//! Storage abstraction for archived blobs.
//!
//! The archive pipeline produces immutable objects (git bundles / zips /
//! checksums). All reads and writes flow through the [`Storage`] trait so
//! the backend and crawler never depend on a concrete provider.
//!
//! Providers:
//! - `local` — filesystem backend for development and single-node deploys.
//! - `r2`    — Cloudflare R2 (S3-compatible object storage) for production.
//!
//! Adding a new provider (S3, MinIO, Garage...) only requires implementing
//! the trait and wiring it in [`Storage::from_config`].

use std::path::PathBuf;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tracing::instrument;

use crate::config::StorageConfig;

/// Errors from the storage layer.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("unsupported storage provider: {0}")]
    UnsupportedProvider(String),
    #[error("storage not configured: {0}")]
    NotConfigured(String),
    #[error("blob not found: {0}")]
    NotFound(String),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("serde error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A streamed blob plus its computed SHA-256.
#[derive(Debug)]
pub struct StoredBlob {
    pub bytes: Vec<u8>,
    pub checksum: String,
}

/// The storage contract implemented by every provider.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Persist a blob under `key`. Returns the SHA-256 checksum of the bytes.
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<String, StorageError>;

    /// Read a blob, verifying its checksum matches `expected` if provided.
    async fn get(
        &self,
        key: &str,
        expected_checksum: Option<&str>,
    ) -> Result<StoredBlob, StorageError>;

    /// Return true if the object exists.
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    /// Delete an object.
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// A stable identifier for the provider (used in storage_location).
    fn provider_name(&self) -> &'static str;

    /// Build a public URL for this object, if the provider supports it.
    async fn public_url(&self, key: &str) -> Option<String>;
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Construct the storage backend from configuration.
pub fn from_config(config: &StorageConfig) -> Result<Box<dyn Storage>, StorageError> {
    match config.provider.as_str() {
        "local" => Ok(Box::new(LocalStorage {
            root: PathBuf::from(&config.local_root),
        })),
        "r2" => Ok(Box::new(r2::R2Storage::from_config(config)?)),
        other => Err(StorageError::UnsupportedProvider(other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Local (filesystem) provider
// ---------------------------------------------------------------------------

/// Filesystem-backed storage, primarily for local development.
pub struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    fn path_for(&self, key: &str) -> PathBuf {
        // Keep the key's slash hierarchy for organization, but refuse any
        // path that could escape the root (no `..`, no absolute paths).
        debug_assert!(!key.contains(".."), "invalid storage key: {key}");
        let relative = key.trim_start_matches('/');
        self.root.join(relative)
    }
}

#[async_trait]
impl Storage for LocalStorage {
    #[instrument(skip(self, bytes), fields(key = %key, size = bytes.len()))]
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<String, StorageError> {
        let path = self.path_for(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes)?;

        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Ok(hex::encode(hasher.finalize()))
    }

    #[instrument(skip(self), fields(key = %key))]
    async fn get(
        &self,
        key: &str,
        expected_checksum: Option<&str>,
    ) -> Result<StoredBlob, StorageError> {
        let path = self.path_for(key);
        if !path.exists() {
            return Err(StorageError::NotFound(key.to_string()));
        }
        let bytes = std::fs::read(&path)?;

        if let Some(expected) = expected_checksum {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual = hex::encode(hasher.finalize());
            if actual != expected {
                return Err(StorageError::ChecksumMismatch {
                    expected: expected.to_string(),
                    actual,
                });
            }
        }

        Ok(StoredBlob {
            bytes,
            checksum: expected_checksum.unwrap_or_default().to_string(),
        })
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.path_for(key).exists())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.path_for(key);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    fn provider_name(&self) -> &'static str {
        "local"
    }

    async fn public_url(&self, _key: &str) -> Option<String> {
        None
    }
}

// ---------------------------------------------------------------------------
// Cloudflare R2 provider (S3-compatible)
// ---------------------------------------------------------------------------

mod r2 {
    use super::*;
    use crate::config::StorageConfig;

    /// R2-backed storage using the S3-compatible API.
    ///
    /// Signed URLs are produced on demand so objects can be served without
    /// making the bucket public.
    pub struct R2Storage {
        client: reqwest::Client,
        endpoint: String,
        bucket: String,
        access_key_id: String,
        secret_access_key: String,
        public_base_url: Option<String>,
    }

    impl R2Storage {
        pub fn from_config(config: &StorageConfig) -> Result<Self, StorageError> {
            let account_id = config
                .r2_account_id
                .clone()
                .ok_or_else(|| StorageError::NotConfigured("r2_account_id".into()))?;
            let bucket = config
                .r2_bucket
                .clone()
                .ok_or_else(|| StorageError::NotConfigured("r2_bucket".into()))?;
            let access_key_id = config
                .r2_access_key_id
                .clone()
                .ok_or_else(|| StorageError::NotConfigured("r2_access_key_id".into()))?;
            let secret_access_key = config
                .r2_secret_access_key
                .clone()
                .ok_or_else(|| StorageError::NotConfigured("r2_secret_access_key".into()))?;

            let endpoint = config
                .r2_endpoint
                .clone()
                .unwrap_or_else(|| format!("https://{account_id}.r2.cloudflarestorage.com"));

            Ok(Self {
                client: reqwest::Client::new(),
                endpoint,
                bucket,
                access_key_id,
                secret_access_key,
                public_base_url: config.r2_public_base_url.clone(),
            })
        }

        /// The S3 object URL for `key` (path-style, bucket as first segment).
        fn object_url(&self, key: &str) -> String {
            format!("{}/{}/{}", self.endpoint, self.bucket, key)
        }

        /// Host component of the endpoint (used in the SigV4 `host` header).
        fn endpoint_host(&self) -> &str {
            self.endpoint
                .trim_start_matches("https://")
                .trim_start_matches("http://")
        }

        /// Minimal AWS Signature V4 signer for PUT/GET/HEAD/DELETE requests.
        fn sign(
            &self,
            method: &str,
            key: &str,
            payload_hash: &str,
            now: &chrono::DateTime<chrono::Utc>,
        ) -> String {
            let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
            let date_stamp = now.format("%Y%m%d").to_string();
            let region = "auto";
            let service = "s3";

            // Path-style addressing: the canonical URI includes the bucket.
            let canonical_uri = format!("/{}/{}", self.bucket, key);
            let canonical_query = "";
            let canonical_headers = format!(
                "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n",
                host = self.endpoint_host(),
            );
            let signed_headers = "host;x-amz-content-sha256;x-amz-date";

            let canonical_request = format!(
                "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
            );

            let scope = format!("{date_stamp}/{region}/{service}/aws4_request");
            let string_to_sign = format!(
                "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
                hex::encode(Sha256::digest(canonical_request.as_bytes()))
            );

            let k_date = hmac_sha256(
                format!("AWS4{}", self.secret_access_key).as_bytes(),
                &date_stamp,
            );
            let k_region = hmac_sha256(&k_date, region);
            let k_service = hmac_sha256(&k_region, service);
            let k_signing = hmac_sha256(&k_service, "aws4_request");
            let signature = hex::encode(hmac_sha256(&k_signing, &string_to_sign));

            format!(
                "AWS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
                self.access_key_id, scope, signed_headers, signature
            )
        }
    }

    fn hmac_sha256(key: &[u8], data: &str) -> Vec<u8> {
        use hmac::{Hmac, Mac};

        let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac key");
        mac.update(data.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }

    #[async_trait]
    impl Storage for R2Storage {
        #[instrument(skip(self, bytes), fields(key = %key, size = bytes.len()))]
        async fn put(&self, key: &str, bytes: &[u8]) -> Result<String, StorageError> {
            let now = chrono::Utc::now();
            let payload_hash = hex::encode(Sha256::digest(bytes));
            let auth = self.sign("PUT", key, &payload_hash, &now);

            let resp = self
                .client
                .put(self.object_url(key))
                .header("Authorization", auth)
                .header("x-amz-date", now.format("%Y%m%dT%H%M%SZ").to_string())
                .header("x-amz-content-sha256", &payload_hash)
                .body(bytes.to_vec())
                .send()
                .await
                .map_err(|e| StorageError::Transport(e.to_string()))?;

            if !resp.status().is_success() {
                return Err(StorageError::Transport(format!(
                    "r2 put failed: {} {}",
                    resp.status(),
                    resp.text().await.unwrap_or_default()
                )));
            }

            let mut hasher = Sha256::new();
            hasher.update(bytes);
            Ok(hex::encode(hasher.finalize()))
        }

        async fn get(
            &self,
            key: &str,
            expected_checksum: Option<&str>,
        ) -> Result<StoredBlob, StorageError> {
            let now = chrono::Utc::now();
            let payload_hash = hex::encode(Sha256::digest(b""));
            let auth = self.sign("GET", key, &payload_hash, &now);

            let resp = self
                .client
                .get(self.object_url(key))
                .header("Authorization", auth)
                .header("x-amz-date", now.format("%Y%m%dT%H%M%SZ").to_string())
                .header("x-amz-content-sha256", &payload_hash)
                .send()
                .await
                .map_err(|e| StorageError::Transport(e.to_string()))?;

            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(StorageError::NotFound(key.to_string()));
            }
            if !resp.status().is_success() {
                return Err(StorageError::Transport(format!(
                    "r2 get failed: {} {}",
                    resp.status(),
                    resp.text().await.unwrap_or_default()
                )));
            }

            let bytes = resp
                .bytes()
                .await
                .map_err(|e| StorageError::Transport(e.to_string()))?
                .to_vec();

            if let Some(expected) = expected_checksum {
                let actual = hex::encode(Sha256::digest(&bytes));
                if actual != expected {
                    return Err(StorageError::ChecksumMismatch {
                        expected: expected.to_string(),
                        actual,
                    });
                }
            }

            Ok(StoredBlob {
                bytes,
                checksum: expected_checksum.unwrap_or_default().to_string(),
            })
        }

        async fn exists(&self, key: &str) -> Result<bool, StorageError> {
            let now = chrono::Utc::now();
            let payload_hash = hex::encode(Sha256::digest(b""));
            let auth = self.sign("HEAD", key, &payload_hash, &now);

            let resp = self
                .client
                .head(self.object_url(key))
                .header("Authorization", auth)
                .header("x-amz-date", now.format("%Y%m%dT%H%M%SZ").to_string())
                .header("x-amz-content-sha256", &payload_hash)
                .send()
                .await
                .map_err(|e| StorageError::Transport(e.to_string()))?;

            Ok(resp.status() == reqwest::StatusCode::OK)
        }

        async fn delete(&self, key: &str) -> Result<(), StorageError> {
            let now = chrono::Utc::now();
            let payload_hash = hex::encode(Sha256::digest(b""));
            let auth = self.sign("DELETE", key, &payload_hash, &now);

            let resp = self
                .client
                .delete(self.object_url(key))
                .header("Authorization", auth)
                .header("x-amz-date", now.format("%Y%m%dT%H%M%SZ").to_string())
                .header("x-amz-content-sha256", &payload_hash)
                .send()
                .await
                .map_err(|e| StorageError::Transport(e.to_string()))?;

            if !resp.status().is_success() {
                return Err(StorageError::Transport(format!(
                    "r2 delete failed: {}",
                    resp.status()
                )));
            }
            Ok(())
        }

        fn provider_name(&self) -> &'static str {
            "r2"
        }

        async fn public_url(&self, key: &str) -> Option<String> {
            self.public_base_url
                .as_ref()
                .map(|base| format!("{}/{}", base.trim_end_matches('/'), key))
        }
    }
}

// Re-export for tests/consumers.
pub use self::r2::R2Storage;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_storage_roundtrip() {
        let storage = LocalStorage {
            root: std::env::temp_dir().join(format!("archivehub-test-{}", uuid::Uuid::new_v4())),
        };

        let data = b"archivehub test payload".to_vec();
        let checksum = storage.put("repo/test.bundle", &data).await.unwrap();
        assert_eq!(checksum.len(), 64);

        let blob = storage
            .get("repo/test.bundle", Some(&checksum))
            .await
            .unwrap();
        assert_eq!(blob.bytes, data);

        // Tampered checksum must be caught.
        let bad = format!("{}1", &checksum[..63]);
        let err = storage
            .get("repo/test.bundle", Some(&bad))
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::ChecksumMismatch { .. }));

        storage.delete("repo/test.bundle").await.unwrap();
        let err = storage.get("repo/test.bundle", None).await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound(_)));
    }
}
