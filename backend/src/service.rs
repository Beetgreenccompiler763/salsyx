//! Repository resolution service.
//!
//! Implements the primary user journey:
//!
//! ```text
//! user searches
//!     │
//!     ├─ GitHub API says "exists" ──► return live GitHub metadata
//!     │
//!     └─ GitHub API says "404" ────► check Salsyx database
//!             │
//!             ├─ archive exists ──► return archived snapshot
//!             │
//!             └─ no archive ──────► "this repository has not been archived"
//! ```
//!
//! This is a *service* — it orchestrates the GitHub client, database, and
//! storage without knowing anything about HTTP.

use tracing::instrument;

use crate::error::AppError;
use crate::state::AppState;

/// Outcome of resolving `owner/repo` against the live source.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // Archived carries a full archive record; fine for a response value.
pub enum ResolveOutcome {
    /// Repository exists on GitHub; return live metadata + download URL.
    Live {
        repository: salsyx_shared::repository::Repository,
        download_url: String,
    },
    /// Repository is gone from GitHub but we have an archived snapshot.
    Archived {
        repository: salsyx_shared::repository::Repository,
        archive: salsyx_shared::archive::Archive,
        download_url: String,
    },
    /// Gone from GitHub, but an external archive provider holds a copy.
    ExternalArchived {
        full_name: String,
        archive: crate::providers::ExternalArchive,
    },
    /// Gone from GitHub and not archived.
    NotFound,
}

#[derive(Debug)]
pub struct ResolveResult {
    pub outcome: ResolveOutcome,
    pub source: &'static str,
}

/// Resolve a repository, always checking GitHub first.
///
/// The `refresh` flag forces a re-check against GitHub even if our local
/// copy was recently refreshed (used by `POST /refresh`).
#[instrument(skip(state), fields(full_name = %full_name))]
pub async fn resolve_repository(
    state: &AppState,
    full_name: &str,
    refresh: bool,
) -> Result<ResolveResult, AppError> {
    let normalized = normalize_full_name(full_name)?;

    // 1. Ask GitHub first.
    match state.github.get_repository(&normalized).await {
        Ok(repo) => {
            // Repository exists → upsert metadata locally and return live.
            let owner_id = crate::db::upsert_owner_from_github(&state.pool, &repo).await?;
            let repo_id = crate::db::upsert_repository(&state.pool, owner_id, &repo).await?;

            // GitHub silently redirects renamed repositories (`old/name` →
            // canonical name). Record the requested name as an alias so the
            // old full name keeps resolving even after the repo is deleted.
            if repo.full_name != normalized {
                let _ = crate::db::upsert_repository_alias(&state.pool, &normalized, repo_id).await;
            }

            // Best-effort commit count (cheap, non-fatal).
            if let Ok(Some(count)) = state.github.get_commit_count(&normalized).await {
                let _ = crate::db::set_commit_count(&state.pool, repo_id, count).await;
            }

            // Fetch by the upserted row id — NOT by the requested name — so a
            // renamed repository resolves instead of 500ing with "upserted but
            // not found".
            let row = crate::db::find_repository_by_id(&state.pool, repo_id)
                .await?
                .ok_or_else(|| {
                    AppError::Internal(anyhow::anyhow!(
                        "repository upserted but not found: {normalized}"
                    ))
                })?;

            let branch = repo.default_branch.as_deref().unwrap_or("main");

            Ok(ResolveResult {
                outcome: ResolveOutcome::Live {
                    repository: crate::db::repository_from_row(row),
                    download_url: format!(
                        "https://github.com/{normalized}/archive/refs/heads/{branch}.zip"
                    ),
                },
                source: "github",
            })
        }
        Err(crate::github::GithubError::NotFound) => {
            // 2. Gone from GitHub → check our archive database.
            resolve_archived(state, &normalized).await
        }
        Err(crate::github::GithubError::RateLimited) => {
            tracing::warn!(full_name = %normalized, "github rate limited; falling back to archive");
            resolve_archived(state, &normalized).await
        }
        Err(e) => Err(AppError::Upstream(e.to_string())),
    }
}

/// Look up the local record + latest archive for a deleted repository.
///
/// Resolution order once GitHub reports the repository is gone:
/// 1. External providers (Software Heritage → Archive.org → Wayback).
/// 2. The local AAHL/git-bundle archive database.
async fn resolve_archived(state: &AppState, full_name: &str) -> Result<ResolveResult, AppError> {
    // Mark as deleted locally so search surfaces it as archived-only.
    let _ = crate::db::mark_repository_deleted(&state.pool, full_name).await;

    // 1. Ask the external provider chain first.
    if let Some(archive) = crate::providers::resolve_external(&state.providers, full_name).await {
        return Ok(ResolveResult {
            outcome: ResolveOutcome::ExternalArchived {
                full_name: full_name.to_string(),
                archive,
            },
            source: "external",
        });
    }

    // 2. Fall back to the local archive database.
    let row = crate::db::find_repository(&state.pool, full_name).await?;

    let Some(row) = row else {
        return Ok(ResolveResult {
            outcome: ResolveOutcome::NotFound,
            source: "salsyx",
        });
    };

    let repository = crate::db::repository_from_row(row);

    if repository.is_deleted {
        if let Some(archive_row) = crate::db::latest_archive(&state.pool, repository.id).await? {
            let archive = archive_row_to_domain(archive_row);
            let download_url = match archive_download_url(state, &archive).await {
                Some(url) => url,
                None => format!("/api/v1/download/{}", archive.id),
            };

            return Ok(ResolveResult {
                outcome: ResolveOutcome::Archived {
                    repository,
                    archive,
                    download_url,
                },
                source: "salsyx",
            });
        }
    }

    Ok(ResolveResult {
        outcome: ResolveOutcome::NotFound,
        source: "salsyx",
    })
}

/// Convert a storage key into a public download URL if the provider can.
async fn archive_download_url(
    state: &AppState,
    archive: &salsyx_shared::archive::Archive,
) -> Option<String> {
    state.storage.public_url(&archive.storage.key).await
}

/// Build a public URL for an archive, falling back to the API download route.
pub async fn public_download_url(state: &AppState, archive_id: uuid::Uuid) -> Option<String> {
    let row = crate::db::find_archive(&state.pool, archive_id)
        .await
        .ok()??;
    let archive = archive_row_to_domain(row);
    archive_download_url(state, &archive).await
}

/// Normalize `owner/repo` input: trim whitespace, strip URL prefixes and
/// trailing slashes, enforce a strict shape.
fn normalize_full_name(input: &str) -> Result<String, AppError> {
    let trimmed = input.trim().trim_end_matches('/');

    // Strip common URL prefixes so users can paste a repo URL directly.
    let without_prefix = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("github.com/"))
        .unwrap_or(trimmed);

    let parts: Vec<&str> = without_prefix
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 2 {
        return Err(AppError::Validation(format!(
            "expected owner/repo, got `{input}`"
        )));
    }

    let (owner, name) = (parts[0], parts[1]);

    if owner
        .chars()
        .any(|c| !(c.is_alphanumeric() || c == '-' || c == '_'))
    {
        return Err(AppError::Validation(format!("invalid owner: {owner}")));
    }
    if name
        .chars()
        .any(|c| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '.'))
    {
        return Err(AppError::Validation(format!("invalid repo name: {name}")));
    }

    Ok(format!("{owner}/{name}"))
}

/// Public wrapper around [`normalize_full_name`] for handlers that need it
/// before resolution.
pub fn normalize_full_name_public(input: &str) -> Result<String, AppError> {
    normalize_full_name(input)
}

/// Map an archive row to the shared domain type.
pub fn archive_row_to_domain(row: crate::db::ArchiveRow) -> salsyx_shared::archive::Archive {
    salsyx_shared::archive::Archive {
        id: row.id,
        repository_id: row.repository_id,
        commit_ref: row.commit_ref,
        commit_count: row.commit_count,
        checksum: row.checksum,
        size_bytes: row.size_bytes,
        storage: salsyx_shared::archive::StorageLocation {
            provider: row.storage_provider,
            key: row.storage_key,
        },
        compression: match row.compression_method.as_str() {
            "zip" => salsyx_shared::archive::CompressionMethod::Zip,
            "git_bundle" => salsyx_shared::archive::CompressionMethod::GitBundle,
            "tar" => salsyx_shared::archive::CompressionMethod::Tar,
            _ => salsyx_shared::archive::CompressionMethod::Custom,
        },
        status: match row.status.as_str() {
            "pending" => salsyx_shared::archive::ArchiveStatus::Pending,
            "fetching" => salsyx_shared::archive::ArchiveStatus::Fetching,
            "processing" => salsyx_shared::archive::ArchiveStatus::Processing,
            "archived" => salsyx_shared::archive::ArchiveStatus::Archived,
            "verification_failed" => salsyx_shared::archive::ArchiveStatus::VerificationFailed,
            _ => salsyx_shared::archive::ArchiveStatus::Failed,
        },
        deleted_at: row.deleted_at,
        error_message: row.error_message,
        archived_at: row.archived_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_full_name;

    #[test]
    fn normalizes_repo_names() {
        assert_eq!(
            normalize_full_name("torvalds/linux").unwrap(),
            "torvalds/linux"
        );
        assert_eq!(
            normalize_full_name("https://github.com/torvalds/linux").unwrap(),
            "torvalds/linux"
        );
        assert_eq!(
            normalize_full_name("github.com/facebook/react/").unwrap(),
            "facebook/react"
        );
        assert!(normalize_full_name("no-slash").is_err());
        assert!(normalize_full_name("bad owner/linux").is_err());
    }
}
