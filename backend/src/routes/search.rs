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

    let mode = params.mode.as_deref().unwrap_or("partial");

    // `full_text` matches against preserved READMEs + descriptions via the
    // `repo_documents` search vector (migration 0003).
    let (rows, total) = if mode == "full_text" && !q.is_empty() {
        crate::db::search_repositories_fulltext(
            &state.pool,
            &q,
            params.owner.as_deref(),
            params.language.as_deref(),
            params.min_stars,
            params.include_deleted.unwrap_or(false),
            params.archived_only.unwrap_or(false),
            page,
            per_page,
        )
        .await?
    } else {
        crate::db::search_repositories(
            &state.pool,
            &q,
            mode,
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
        .await?
    };

    // Live fallback: the local index only knows repos that have been visited
    // or archived, so a cold search for an existing GitHub repo would show
    // "no results". When the index is empty (and the filters aren't pinned to
    // archived-only semantics GitHub can't serve), ask GitHub, upsert the
    // hits, then re-run the local query so sorting/filters stay consistent.
    let (rows, total) = if total == 0 && !q.is_empty() && mode != "full_text" {
        match live_github_fallback(&state, &params, &q, per_page).await {
            Ok((rows, total)) if total > 0 => (rows, total),
            _ => (rows, total),
        }
    } else {
        (rows, total)
    };

    let items = rows.into_iter().map(to_search_item).collect();

    Ok(Json(SearchResponse {
        total,
        page,
        per_page,
        query: q,
        items,
    }))
}

fn to_search_item(r: crate::db::SearchHitRow) -> SearchItem {
    SearchItem {
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
    }
}

/// Ask GitHub for repositories matching `q`, persist them, then re-run the
/// local search so the response is shaped/sorted exactly like an indexed hit.
///
/// Skipped when the caller wants deleted/archived-only results (GitHub only
/// has live repositories) or a full-text (README) match.
async fn live_github_fallback(
    state: &AppState,
    params: &SearchParams,
    q: &str,
    per_page: u64,
) -> Result<(Vec<crate::db::SearchHitRow>, u64), crate::error::AppError> {
    if params.include_deleted.unwrap_or(false) || params.archived_only.unwrap_or(false) {
        return Ok((Vec::new(), 0));
    }

    // `owner/repo` queries become exact `repo:` qualifiers; plain terms stay
    // as-is (GitHub search tokenizes on whitespace).
    let github_q = if q.contains('/') {
        format!("repo:{q}")
    } else {
        q.to_string()
    };

    let hits = match state
        .github
        .search_repositories(&github_q, per_page.clamp(1, 100) as i64)
        .await
    {
        Ok(hits) => hits,
        Err(e) => {
            // Degrade gracefully: a failed fallback returns the (empty)
            // index-only result instead of erroring the whole search.
            tracing::warn!(error = %e, "github search fallback failed; returning index results");
            return Ok((Vec::new(), 0));
        }
    };

    for repo in &hits {
        if let Ok(owner_id) = crate::db::upsert_owner_from_github(&state.pool, repo).await {
            let _ = crate::db::upsert_repository(&state.pool, owner_id, repo).await;
        }
    }

    crate::db::search_repositories(
        &state.pool,
        q,
        "partial",
        params.owner.as_deref(),
        params.language.as_deref(),
        params.license.as_deref(),
        &[],
        params.min_stars,
        false,
        false,
        params.sort.as_deref().unwrap_or("relevance"),
        params.order.as_deref().unwrap_or("desc"),
        1,
        per_page,
    )
    .await
}
