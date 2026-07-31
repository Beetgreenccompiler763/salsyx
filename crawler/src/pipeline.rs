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

/// One `archive` job: clone, compress, store, record.
///
/// # Storage strategy
///
/// Instead of downloading a GitHub-generated ZIP (which is lossy — it drops
/// history and `.git` metadata), the pipeline performs a **bare clone** and
/// stores either:
///
/// - a **git bundle** (`format = "git_bundle"`, the default): complete
///   history, all refs, commit hashes — nothing is lost. Git's own
///   compression (zlib + delta packs) is already excellent, and bundles
///   deduplicate objects across versions naturally.
/// - an **AAHL snapshot** (`format = "aahl"`): a content-addressed, chunked
///   manifest over a checked-out worktree of `HEAD`. Deduplicates at the
///   chunk level (shared across repositories and snapshots) and stores a
///   single small manifest per archive; the full git history is lost, so
///   `git_bundle` remains the default.
///
/// Either way the output is a single immutable blob (bundle or manifest
/// JSON) that is checksummed at rest and re-verified on read.
pub async fn archive_repository(
    pool: &PgPool,
    storage: &dyn Storage,
    archive_id: Uuid,
    repo_full_name: &str,
    repo_id: Uuid,
    format: &str,
) -> anyhow::Result<()> {
    match format {
        "aahl" => archive_repository_aahl(pool, storage, archive_id, repo_full_name, repo_id).await,
        _ => archive_repository_bundle(pool, storage, archive_id, repo_full_name, repo_id).await,
    }
}

/// Bare clone plus a best-effort snapshot of the file tree + README. Shared
/// by both formats. Returns the tempdir (kept alive), the repo path, and the
/// resolved HEAD.
async fn prepare_repo(
    pool: &PgPool,
    archive_id: Uuid,
    repo_full_name: &str,
    repo_id: Uuid,
) -> anyhow::Result<(tempfile::TempDir, std::path::PathBuf, Option<String>, i64)> {
    set_status(pool, archive_id, "fetching", None).await?;

    // Work in a temp dir so a crash never leaves partial state behind.
    let tmp = tempfile::tempdir()?;
    let repo_path = tmp.path().join(repo_full_name.replace('/', "_"));

    if let Err(e) = clone_repository(repo_full_name, &repo_path).await {
        set_status(pool, archive_id, "failed", Some(&e.to_string())).await?;
        return Err(e);
    }

    set_status(pool, archive_id, "processing", None).await?;

    let commit_ref = current_head(&repo_path)?;
    let commit_count = count_commits(&repo_path)?;

    // Snapshot the file tree + README so the API can browse preserved
    // contents without opening the archive. Best-effort: never fails the job.
    let tree_entries = list_tree(&repo_path).unwrap_or_default();
    if !tree_entries.is_empty() {
        let value = serde_json::Value::Array(tree_entries);
        if let Err(e) = salsyx_api::db::set_archive_tree(pool, archive_id, &value).await {
            tracing::warn!(archive_id = %archive_id, error = %e, "failed to store file tree");
        }
    }
    if let Some(readme) = extract_readme(&repo_path) {
        if !readme.trim().is_empty() {
            if let Err(e) = salsyx_api::db::upsert_readme(pool, repo_id, &readme).await {
                tracing::warn!(archive_id = %archive_id, error = %e, "failed to store readme");
            }
        }
    }

    Ok((tmp, repo_path, commit_ref, commit_count))
}

/// Default format: store a single-file git bundle of all refs.
async fn archive_repository_bundle(
    pool: &PgPool,
    storage: &dyn Storage,
    archive_id: Uuid,
    repo_full_name: &str,
    repo_id: Uuid,
) -> anyhow::Result<()> {
    let (_tmp, repo_path, commit_ref, commit_count) =
        prepare_repo(pool, archive_id, repo_full_name, repo_id).await?;

    let bundle_path = bundle_repository(&repo_path)?;

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
        "git_bundle",
    )
    .await?;

    tracing::info!(archive_id = %archive_id, repo = %repo_full_name, bytes = bytes.len(), "bundle stored");
    Ok(())
}

/// AAHL format: checkout `HEAD` and encode it as a content-addressed
/// snapshot. Chunks are written through [`salsyx_api::aahl::StorageChunkStore`];
/// the manifest (the archive blob) is stored alongside them.
async fn archive_repository_aahl(
    pool: &PgPool,
    storage: &dyn Storage,
    archive_id: Uuid,
    repo_full_name: &str,
    repo_id: Uuid,
) -> anyhow::Result<()> {
    let (_tmp, repo_path, commit_ref, commit_count) =
        prepare_repo(pool, archive_id, repo_full_name, repo_id).await?;

    // Check out HEAD into a worktree. Empty repos have no HEAD; encode an
    // empty root instead so the snapshot is still a valid (empty) archive.
    let work_dir = repo_path.with_extension("work");
    std::fs::create_dir_all(&work_dir)?;
    if commit_ref.is_some() {
        let status = Command::new("git")
            .args([
                "--git-dir",
                repo_path.to_str().unwrap_or_default(),
                "--work-tree",
            ])
            .arg(&work_dir)
            .args(["checkout", "HEAD", "--"])
            .status()?;
        if !status.success() {
            anyhow::bail!("git checkout HEAD failed for {repo_full_name}");
        }
    }

    let source = aahl::SourceInfo {
        kind: "github".to_string(),
        id: repo_full_name.to_string(),
        reference: Some("HEAD".to_string()),
        commit: commit_ref.clone(),
        captured_at: Some(chrono::Utc::now()),
    };

    let chunk_store = salsyx_api::aahl::StorageChunkStore::new(storage);
    let manifest = aahl::encode::encode_dir(&work_dir, &chunk_store, source, None).await?;
    let checksum = manifest.digest()?;

    // Store the canonical manifest bytes so the stored blob hashes to
    // `checksum` exactly (signature is None for crawler-produced archives).
    let bytes = manifest.canonical_bytes()?;

    // Object key layout: archives/{repo_id}/{archive_id}.aahl
    let storage_key = format!("archives/{repo_id}/{archive_id}.aahl");

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
        "custom",
    )
    .await?;

    tracing::info!(
        archive_id = %archive_id,
        repo = %repo_full_name,
        entries = manifest.entries.len(),
        chunks = manifest.blobs.len(),
        bytes = bytes.len(),
        "aahl snapshot stored"
    );
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

/// Recursive file listing of `HEAD` as JSON entries for the browsing API.
///
/// Line format: `<mode> SP <type> SP <object> TAB <size> TAB <path>`.
/// Sizes come from the tree entries without fetching blob contents.
fn list_tree(repo_path: &Path) -> anyhow::Result<Vec<serde_json::Value>> {
    let out = Command::new("git")
        .args(["ls-tree", "-r", "-l", "HEAD"])
        .current_dir(repo_path)
        .output()?;

    if !out.status.success() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut parts = line.splitn(4, ' ');
        let _mode = parts.next();
        let Some(kind) = parts.next() else { continue };
        let _object = parts.next();
        let Some(rest) = parts.next() else { continue };
        let Some(tab) = rest.find('\t') else { continue };
        let size_str = &rest[..tab];
        let path = rest[tab + 1..].trim_end().to_string();
        if path.is_empty() {
            continue;
        }
        entries.push(serde_json::json!({
            "path": path,
            "type": match kind {
                "blob" => "blob",
                "tree" => "tree",
                _ => "other",
            },
            "size": size_str.trim().parse::<i64>().ok().filter(|s| *s >= 0),
        }));
    }

    Ok(entries)
}

/// Extract the default-branch README (best match) as markdown text.
///
/// Tries common README filenames in order; a bare-clone `git show` lazily
/// fetches the blob from the still-live repository.
fn extract_readme(repo_path: &Path) -> Option<String> {
    const CANDIDATES: &[&str] = &[
        "README.md",
        "README.markdown",
        "README.mkd",
        "README.rst",
        "README.txt",
        "readme.md",
        "README",
        "README.adoc",
        "README.org",
    ];

    for name in CANDIDATES {
        let out = Command::new("git")
            .args(["show", &format!("HEAD:{name}")])
            .current_dir(repo_path)
            .output()
            .ok()?;
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }

    // Fall back to a case-insensitive scan of the tree.
    let tree = list_tree(repo_path).ok()?;
    let readme = tree.into_iter().find(|e| {
        e.get("path")
            .and_then(|p| p.as_str())
            .map(|p| {
                p.split('/')
                    .next_back()
                    .map(|base| base.to_ascii_lowercase().starts_with("readme"))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    })?;
    let path = readme.get("path")?.as_str()?;

    let out = Command::new("git")
        .args(["show", &format!("HEAD:{path}")])
        .current_dir(repo_path)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
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
    compression_method: &str,
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
            compression_method = $8,
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
    .bind(compression_method)
    .execute(pool)
    .await?;
    Ok(())
}
