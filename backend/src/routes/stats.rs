//! Platform statistics endpoints.
//!
//! `GET /api/v1/stats`      — aggregate platform metrics
//! `GET /api/v1/stats/top`  — top languages + highest-starred repos

use axum::{extract::State, Json};
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub total_repositories: i64,
    pub archived_repositories: i64,
    pub total_archives: i64,
    pub total_archived_bytes: i64,
    pub total_downloads: i64,
    pub deleted_archived: i64,
    pub total_owners: i64,
    pub indexed_bytes: i64,
}

#[derive(Debug, Serialize)]
pub struct TopResponse {
    pub languages: Vec<LanguageCount>,
    pub top_repositories: Vec<TopRepo>,
}

#[derive(Debug, Serialize)]
pub struct LanguageCount {
    pub language: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct TopRepo {
    pub full_name: String,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars_count: i64,
    pub is_deleted: bool,
    pub has_archive: bool,
}

/// `GET /api/v1/stats`
pub async fn stats(
    State(state): State<AppState>,
) -> Result<Json<StatsResponse>, crate::error::AppError> {
    let row = crate::db::platform_stats(&state.pool).await?;

    Ok(Json(StatsResponse {
        total_repositories: row.total_repositories,
        archived_repositories: row.archived_repositories,
        total_archives: row.total_archives,
        total_archived_bytes: row.total_archived_bytes,
        total_downloads: row.total_downloads,
        deleted_archived: row.deleted_archived,
        total_owners: row.total_owners,
        indexed_bytes: row.indexed_bytes,
    }))
}

/// `GET /api/v1/stats/top`
pub async fn top(
    State(state): State<AppState>,
) -> Result<Json<TopResponse>, crate::error::AppError> {
    let languages = crate::db::top_languages(&state.pool, 10).await?;
    let repos = crate::db::top_repositories(&state.pool, 10).await?;

    Ok(Json(TopResponse {
        languages: languages
            .into_iter()
            .map(|l| LanguageCount {
                language: l.language,
                count: l.count,
            })
            .collect(),
        top_repositories: repos
            .into_iter()
            .map(|r| TopRepo {
                full_name: r.full_name,
                owner: r.owner_login,
                name: r.name,
                description: r.description,
                language: r.language,
                stars_count: r.stars_count,
                is_deleted: r.is_deleted,
                has_archive: r.has_archive,
            })
            .collect(),
    }))
}
