//! Owner profile endpoint.
//!
//! `GET /api/v1/owner/{login}` — live GitHub profile (followers/following/
//! public repos/pinned) merged with how many of the owner's repositories
//! Salsyx has preserved. Powers the "pop a bubble" profile modal.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct OwnerResponse {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub company: Option<String>,
    pub blog: Option<String>,
    pub location: Option<String>,
    pub twitter_username: Option<String>,
    pub owner_type: String,
    pub followers: i64,
    pub following: i64,
    pub public_repos: i64,
    pub created_at: Option<String>,
    /// Repositories pinned on the GitHub profile (empty when anonymous).
    pub pinned_repos: Vec<crate::github::PinnedRepo>,
    /// The owner's most-starred public repositories.
    pub top_repos: Vec<TopRepo>,
    /// How many of the owner's repositories Salsyx has preserved.
    pub preserved_count: i64,
}

#[derive(Debug, Serialize)]
pub struct TopRepo {
    pub full_name: String,
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars_count: i64,
    pub has_archive: bool,
}

/// `GET /api/v1/owner/{login}`
pub async fn get_owner(
    State(state): State<AppState>,
    Path(login): Path<String>,
) -> Result<Json<OwnerResponse>, AppError> {
    if login.is_empty()
        || login
            .chars()
            .any(|c| !(c.is_alphanumeric() || c == '-' || c == '_'))
    {
        return Err(AppError::Validation("invalid owner login".into()));
    }

    let user = match state.github.get_user(&login).await {
        Ok(user) => user,
        Err(crate::github::GithubError::NotFound) => {
            return Err(AppError::NotFound { full_name: login })
        }
        Err(crate::github::GithubError::RateLimited) => return Err(AppError::RateLimited),
        Err(e) => return Err(AppError::Upstream(e.to_string())),
    };

    let owner_id = crate::db::upsert_owner_from_github_user(&state.pool, &user).await?;

    // Pinned repos (GraphQL; empty when running anonymously). Fall back to
    // the most-starred repos so the modal is never empty.
    let pinned = state
        .github
        .get_pinned_repos(&user.login)
        .await
        .unwrap_or_default();

    let repos = state
        .github
        .list_user_repos(&user.login, 30)
        .await
        .unwrap_or_default();

    let mut top: Vec<TopRepo> = repos
        .into_iter()
        .map(|r| TopRepo {
            full_name: r.full_name.clone(),
            name: r.name,
            description: r.description,
            language: r.language,
            stars_count: r.stargazers_count,
            has_archive: false,
        })
        .collect();
    top.sort_by_key(|r| std::cmp::Reverse(r.stars_count));
    top.truncate(6);

    // Mark which top repos we have preserved (cheap: one owner-scoped query).
    let preserved = crate::db::count_archived_for_owner(&state.pool, owner_id).await?;

    let pinned_names: std::collections::HashSet<String> =
        pinned.iter().map(|p| p.full_name.clone()).collect();
    for repo in top.iter_mut() {
        repo.has_archive = pinned_names.contains(&repo.full_name);
    }

    let owner_type = if user.user_type.eq_ignore_ascii_case("Organization") {
        "organization"
    } else {
        "user"
    };

    Ok(Json(OwnerResponse {
        login: user.login,
        name: user.name,
        avatar_url: user.avatar_url,
        bio: user.bio,
        company: user.company,
        blog: user.blog,
        location: user.location,
        twitter_username: user.twitter_username,
        owner_type: owner_type.to_string(),
        followers: user.followers,
        following: user.following,
        public_repos: user.public_repos,
        created_at: user.created_at,
        pinned_repos: pinned,
        top_repos: top,
        preserved_count: preserved,
    }))
}
