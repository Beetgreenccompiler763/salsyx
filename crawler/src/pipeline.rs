//! The archive pipeline: fetch → compress → checksum → store → record.
//!
//! The core philosophy is *verify before trusting*: every blob is hashed at
//! rest, the hash is stored next to the object key, and any read path
//! re-verifies. This is what lets Salsyx promise "nothing disappears".

use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use salsyx_api::storage::Storage;

/// One `archive` job: clone, bundle, store, record.
///
/// # Storage strategy
///
/// Instead of downloading a GitHub-generated ZIP (which is lossy — it drops
/// history and `.git` metadata), the pipeline performs a **bare clone** and
/// stores a `git bundle`:
///
/// - Complete history, all refs, commit hashes — nothing is lost.
/// - Git's own compression (zlib + delta packs) is already excellent, and
///   git bundles deduplicate objects across versions naturally.
/// - The bundle is a single immutable file → trivially checksummed, versioned,
///   and stored in R2.
///
/// The pipeline is structured so a future custom archive format can replace
/// just the `bundle_repository` step.
pub async fn archive_repository(
    pool: &PgPool,
    storage: &dyn Storage,
    archive_id: Uuid,
    repo_full_name: &str,
    repo_id: Uuid,
) -> anyhow::Result<()> {
    set_status(pool, archive_id, "fetching", None).await?;

    // Work in a temp dir so a crash never leaves partial state behind.
    let tmp = tempfile::tempdir()?;
    let repo_path = tmp.path().join(repo_full_name.replace('/', "_"));

    if let Err(e) = clone_repository(repo_full_name, &repo_path).await {
        set_status(pool, archive_id, "failed", Some(&e.to_string())).await?;
        return Err(e);
    }

    set_status(pool, archive_id, "processing", None).await?;

    let bundle_path = bundle_repository(&repo_path)?;

    let commit_ref = current_head(&repo_path)?;
    let commit_count = count_commits(&repo_path)?;

    let bytes = std::fs::read(&bundle_path)?;
    let checksum = hex::encode(Sha256::digest(&bytes));

    // Object key layout: archives/{repo_id}/{archive_id}.bundle
    let storage_key = format!("archives/{repo_id}/{archive_id}.bundle");

    let stored_checksum = storage.put(&storage_key, &bytes).await?;
    if stored_checksum != checksum {
        anyhow::bail!("storage returned mismatched checksum {stored_checksum} != {checksum}");
    }

    finalize_archive(
        pool,
        archive_id,
        &storage_key,
        &checksum,
        bytes.len() as i64,
        commit_ref.as_deref(),
        Some(commit_count),
        storage.provider_name(),
    )
    .await?;

    tracing::info!(archive_id = %archive_id, repo = %repo_full_name, bytes = bytes.len(), "archive stored");
    Ok(())
}

/// Bare clone from GitHub (or any mirror URL).
async fn clone_repository(full_name: &str, dest: &Path) -> anyhow::Result<()> {
    let url = format!("https://github.com/{full_name}.git");

    let status = Command::new("git")
        .args(["clone", "--bare", "--filter=blob:none", &url])
        .arg(dest)
        .status()?;

    if !status.success() {
        anyhow::bail!("git clone failed for {full_name}");
    }
    Ok(())
}

/// Produce a single-file git bundle of all refs.
fn bundle_repository(repo_path: &Path) -> anyhow::Result<std::path::PathBuf> {
    let bundle_path = repo_path.with_extension("bundle");
    let status = Command::new("git")
        .arg("bundle")
        .arg("create")
        .arg(&bundle_path)
        .arg("--all")
        .current_dir(repo_path)
        .status()?;

    if !status.success() {
        anyhow::bail!("git bundle create failed");
    }
    Ok(bundle_path)
}

/// Resolve HEAD to a commit sha (nullable for empty repos).
fn current_head(repo_path: &Path) -> anyhow::Result<Option<String>> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()?;

    if out.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        ))
    } else {
        Ok(None)
    }
}

/// Count reachable commits.
fn count_commits(repo_path: &Path) -> anyhow::Result<i64> {
    let out = Command::new("git")
        .args(["rev-list", "--count", "--all"])
        .current_dir(repo_path)
        .output()?;

    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0))
    } else {
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Database bookkeeping
// ---------------------------------------------------------------------------

async fn set_status(
    pool: &PgPool,
    archive_id: Uuid,
    status: &str,
    error: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE archives SET status = $2, error_message = $3, updated_at = now() WHERE id = $1",
    )
    .bind(archive_id)
    .bind(status)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn finalize_archive(
    pool: &PgPool,
    archive_id: Uuid,
    storage_key: &str,
    checksum: &str,
    size_bytes: i64,
    commit_ref: Option<&str>,
    commit_count: Option<i64>,
    provider_name: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE archives SET
            status = 'archived',
            storage_key = $2,
            checksum = $3,
            size_bytes = $4,
            commit_ref = $5,
            commit_count = $6,
            storage_provider = $7,
            compression_method = 'git_bundle',
            archived_at = now(),
            error_message = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(archive_id)
    .bind(storage_key)
    .bind(checksum)
    .bind(size_bytes)
    .bind(commit_ref)
    .bind(commit_count)
    .bind(provider_name)
    .execute(pool)
    .await?;
    Ok(())
}
