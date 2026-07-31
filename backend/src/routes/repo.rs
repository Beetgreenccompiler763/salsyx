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
    /// `github` if the repo is live, `archivehub` if served from archive.
    pub source: &'static str,
    /// `live` | `archived` | `not_found` | `not_archived`
    pub status: &'static str,
    pub repository: Option<archivehub_shared::repository::Repository>,
    pub archive: Option<archivehub_shared::archive::Archive>,
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
