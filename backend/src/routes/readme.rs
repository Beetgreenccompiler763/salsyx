//! README endpoint.
//!
//! `GET /api/v1/repo/{owner}/{repo}/readme` — return the default-branch README
//! in markdown, preferring the preserved copy stored by the crawler and
//! falling back to a live fetch from GitHub (which is cached for next time).

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;

use crate::error::AppError;
use crate::service::normalize_full_name_public;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ReadmeResponse {
    pub full_name: String,
    /// Markdown source of the README.
    pub readme: String,
    /// `salsyx` if served from the preserved snapshot, `github` if live.
    pub source: &'static str,
    pub html_url: Option<String>,
}

/// `GET /api/v1/repo/{owner}/{repo}/readme`
pub async fn get_readme(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
) -> Result<Json<ReadmeResponse>, AppError> {
    let full_name = normalize_full_name_public(&format!("{owner}/{repo}"))?;

    // 1. Preserved copy first — this is the whole point of Salsyx: a README
    //    survives even after the repository is deleted from GitHub.
    let row = crate::db::find_repository(&state.pool, &full_name).await?;
    if let Some(row) = &row {
        if let Some(readme) = crate::db::find_readme(&state.pool, row.id).await? {
            if !readme.trim().is_empty() {
                return Ok(Json(ReadmeResponse {
                    html_url: Some(format!("https://github.com/{full_name}")),
                    readme,
                    source: "salsyx",
                    full_name,
                }));
            }
        }
    }

    // 2. Fall back to a live fetch (and cache it when we know the repo id).
    match state.github.get_readme(&full_name).await {
        Ok(data) => {
            if let Some(row) = &row {
                let _ = crate::db::upsert_readme(&state.pool, row.id, &data.text).await;
            }
            Ok(Json(ReadmeResponse {
                html_url: Some(data.html_url),
                readme: data.text,
                source: "github",
                full_name,
            }))
        }
        Err(crate::github::GithubError::NotFound) => Err(AppError::NotFound { full_name }),
        Err(crate::github::GithubError::RateLimited) => Err(AppError::RateLimited),
        Err(e) => Err(AppError::Upstream(e.to_string())),
    }
}
