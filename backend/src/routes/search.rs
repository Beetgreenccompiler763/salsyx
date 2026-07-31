//! Search endpoint.
//!
//! `GET /api/v1/search?q=...&language=rust&min_stars=100&page=1`

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct SearchParams {
    /// The search term. Searches name, owner, description.
    pub q: Option<String>,
    /// Match mode: `exact` | `partial` | `fuzzy`.
    pub mode: Option<String>,
    /// Restrict to this owner login.
    pub owner: Option<String>,
    pub language: Option<String>,
    pub license: Option<String>,
    pub topics: Option<String>,
    pub min_stars: Option<i64>,
    pub include_deleted: Option<bool>,
    pub archived_only: Option<bool>,
    /// Sort: `relevance` | `stars` | `forks` | `name` | `updated_at`.
    pub sort: Option<String>,
    pub order: Option<String>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub query: String,
    pub items: Vec<SearchItem>,
}

#[derive(Debug, Serialize)]
pub struct SearchItem {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub stars_count: i64,
    pub forks_count: i64,
    pub is_deleted: bool,
    pub has_archive: bool,
    pub archived_at: Option<String>,
    pub html_url: Option<String>,
    pub last_checked_at: Option<String>,
}

/// `GET /api/v1/search`
pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, crate::error::AppError> {
    let q = params.q.clone().unwrap_or_default().trim().to_string();
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).clamp(1, 100);

    let topics: Vec<String> = params
        .topics
        .clone()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();

    let (rows, total) = crate::db::search_repositories(
        &state.pool,
        &q,
        params.mode.as_deref().unwrap_or("partial"),
        params.owner.as_deref(),
        params.language.as_deref(),
        params.license.as_deref(),
        &topics,
        params.min_stars,
        params.include_deleted.unwrap_or(false),
        params.archived_only.unwrap_or(false),
        params.sort.as_deref().unwrap_or("relevance"),
        params.order.as_deref().unwrap_or("desc"),
        page,
        per_page,
    )
    .await?;

    let items = rows
        .into_iter()
        .map(|r| SearchItem {
            id: r.id.to_string(),
            owner: r.owner_login,
            name: r.name,
            full_name: r.full_name,
            description: r.description,
            language: r.language,
            license: r.license,
            topics: r.topics,
            stars_count: r.stars_count,
            forks_count: r.forks_count,
            is_deleted: r.is_deleted,
            has_archive: r.has_archive,
            archived_at: r.archived_at.map(|d| d.to_rfc3339()),
            html_url: r.html_url,
            last_checked_at: r.last_checked_at.map(|d| d.to_rfc3339()),
        })
        .collect();

    Ok(Json(SearchResponse {
        total,
        page,
        per_page,
        query: q,
        items,
    }))
}
