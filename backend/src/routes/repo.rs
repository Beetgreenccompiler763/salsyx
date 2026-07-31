//! Repository resolution endpoints.
//!
//! `GET  /api/v1/repo/{owner}/{repo}` — resolve against GitHub, fall back to
//! the archive database, and clearly report when neither has the repo.
//! `POST /api/v1/refresh`            — force a re-check of an existing repo.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use uuid::Uuid;

use crate::service::{resolve_repository, ResolveOutcome};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct RepoResponse {
    /// `github` if the repo is live, `salsyx` if served from the Salsyx archive.
    pub source: &'static str,
    /// `live` | `archived` | `not_found` | `not_archived`
    pub status: &'static str,
    pub repository: Option<salsyx_shared::repository::Repository>,
    pub archive: Option<salsyx_shared::archive::Archive>,
    pub download_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// `GET /api/v1/repo/{owner}/{repo}`
pub async fn resolve(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
) -> Result<Json<RepoResponse>, crate::error::AppError> {
    let full_name = format!("{owner}/{repo}");
    let result = resolve_repository(&state, &full_name, false).await?;

    match result.outcome {
        ResolveOutcome::Live {
            repository,
            download_url,
        } => Ok(Json(RepoResponse {
            source: result.source,
            status: "live",
            repository: Some(repository),
            archive: None,
            download_url: Some(download_url),
            message: None,
        })),
        ResolveOutcome::Archived {
            repository,
            archive,
            download_url,
        } => Ok(Json(RepoResponse {
            source: result.source,
            status: "archived",
            repository: Some(repository),
            archive: Some(archive),
            download_url: Some(download_url),
            message: None,
        })),
        ResolveOutcome::NotFound => Ok(Json(RepoResponse {
            source: result.source,
            status: "not_found",
            repository: None,
            archive: None,
            download_url: None,
            message: Some(format!(
                "`{full_name}` was not found on GitHub and has not been archived."
            )),
        })),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct RefreshRequest {
    pub full_name: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub full_name: String,
    pub status: &'static str,
    pub source: &'static str,
    pub archive_id: Option<Uuid>,
}

/// `POST /api/v1/refresh` — force re-resolution of a repository.
pub async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, crate::error::AppError> {
    let result = resolve_repository(&state, &body.full_name, true).await?;

    let (status, archive_id) = match &result.outcome {
        ResolveOutcome::Live { .. } => ("live", None),
        ResolveOutcome::Archived { archive, .. } => ("archived", Some(archive.id)),
        ResolveOutcome::NotFound => ("not_found", None),
    };

    Ok(Json(RefreshResponse {
        full_name: body.full_name,
        status,
        source: result.source,
        archive_id,
    }))
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub full_name: String,
    pub status: &'static str,
    /// All archives for the repository, newest first. Only `archived` records
    /// are usable for browsing/downloading.
    pub archives: Vec<HistoryArchive>,
}

#[derive(Debug, Serialize)]
pub struct HistoryArchive {
    pub id: Uuid,
    pub commit_ref: Option<String>,
    pub commit_count: Option<i64>,
    pub checksum: String,
    pub size_bytes: i64,
    pub compression: String,
    pub status: String,
    pub archived_at: String,
    pub error_message: Option<String>,
    pub download_url: String,
}

/// `GET /api/v1/repo/{owner}/{repo}/archives` — archive history for a repo.
pub async fn history(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
) -> Result<Json<HistoryResponse>, crate::error::AppError> {
    let full_name = format!("{owner}/{repo}");
    let normalized = crate::service::normalize_full_name_public(&full_name)?;

    let row = crate::db::find_repository(&state.pool, &normalized)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound {
            full_name: normalized.clone(),
        })?;

    let archives = crate::db::list_archives(&state.pool, row.id, 50).await?;

    let mut items = Vec::with_capacity(archives.len());
    for a in archives {
        let download_url = state
            .storage
            .public_url(&a.storage_key)
            .await
            .unwrap_or_else(|| format!("/api/v1/download/{}", a.id));

        items.push(HistoryArchive {
            id: a.id,
            commit_ref: a.commit_ref,
            commit_count: a.commit_count,
            checksum: a.checksum,
            size_bytes: a.size_bytes,
            compression: a.compression_method,
            status: a.status,
            archived_at: a.archived_at.to_rfc3339(),
            error_message: a.error_message,
            download_url,
        });
    }

    Ok(Json(HistoryResponse {
        full_name: row.full_name,
        status: if row.is_deleted { "deleted" } else { "live" },
        archives: items,
    }))
}
