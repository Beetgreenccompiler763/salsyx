//! GitHub REST API client.
//!
//! Responsible for the *live* resolution step: given `owner/repo`, ask
//! GitHub whether the repository still exists and return its metadata.
//!
//! # Rate limits
//!
//! Unauthenticated requests are limited to 60/hour. When the token is set
//! we get 5,000/hour. The client surfaces `429`/`403`-with-ratelimit as
//! `ApiError::RateLimited` and the resolver maps that to a friendly API
//! response instead of a hard failure.

use std::time::Duration;

use anyhow::Context;
use reqwest::StatusCode;
use serde::Deserialize;
use tracing::{debug, instrument};

use crate::config::GithubConfig;

/// Errors specific to talking to the GitHub API.
#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    /// The repository does not exist (or was made private/deleted).
    #[error("github returned 404 for repository")]
    NotFound,
    /// We hit the API rate limit.
    #[error("github api rate limited")]
    RateLimited,
    /// GitHub is having a bad day; caller should treat as transient.
    #[error("github upstream error: {0}")]
    Upstream(String),
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
}

/// GitHub's repository payload (subset we care about).
#[derive(Debug, Clone, Deserialize)]
pub struct GithubRepo {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub owner: GithubOwner,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub default_branch: Option<String>,
    pub language: Option<String>,
    pub license: Option<GithubLicense>,
    pub topics: Vec<String>,
    pub stargazers_count: i64,
    pub forks_count: i64,
    pub watchers_count: i64,
    pub open_issues_count: i64,
    pub size: i64,
    pub archived: bool,
    pub visibility: String,
    pub pushed_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubOwner {
    pub id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
    #[serde(rename = "type")]
    pub owner_type: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubLicense {
    pub key: String,
    pub name: String,
    pub spdx_id: Option<String>,
}

/// The subset of `GithubRepo` that the rate-limited commit-count call needs.
///
/// GitHub's `GET /repos/{owner}/{repo}/commits` returns a JSON array; we only
/// read the `Link` header to estimate commit counts cheaply, so a stripped
/// payload is enough.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct GithubCommit {
    pub sha: String,
}

/// Minimal `Link` header parser for paginated GitHub responses.
fn parse_link_rel(link_header: Option<&str>) -> Option<i64> {
    let link_header = link_header?;
    for part in link_header.split(',') {
        if part.contains("rel=\"last\"") {
            if let Some(url_part) = part.split(';').next() {
                let url = url_part
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>');
                let parsed = url::Url::parse(url).ok()?;
                if let Some(page) = parsed
                    .query_pairs()
                    .find(|(k, _)| k == "page")
                    .map(|(_, v)| v.to_string())
                {
                    return page.parse().ok();
                }
            }
        }
    }
    None
}

/// Async client wrapping GitHub's REST API.
#[derive(Debug, Clone)]
pub struct GithubClient {
    http: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl GithubClient {
    pub fn new(config: &GithubConfig) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .context("failed to build github http client")?;

        // An empty-string token means "anonymous" — never send a blank
        // `Authorization: Bearer` header (GitHub rejects it with 401).
        let token = config.token.clone().filter(|t| !t.trim().is_empty());

        Ok(Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            token,
        })
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    #[instrument(skip(self), fields(full_name = %full_name))]
    pub async fn get_repository(&self, full_name: &str) -> Result<GithubRepo, GithubError> {
        let url = self.endpoint(&format!("/repos/{full_name}"));

        let mut req = self.http.get(&url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let resp = req.send().await?;

        match resp.status() {
            StatusCode::OK => {
                let repo: GithubRepo = resp.json().await?;
                debug!(stars = repo.stargazers_count, "resolved github repository");
                Ok(repo)
            }
            StatusCode::NOT_FOUND | StatusCode::GONE => Err(GithubError::NotFound),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
                let limited = resp
                    .headers()
                    .get("x-ratelimit-remaining")
                    .and_then(|v| v.to_str().ok())
                    == Some("0");
                if limited {
                    Err(GithubError::RateLimited)
                } else {
                    Err(GithubError::Upstream("forbidden".to_string()))
                }
            }
            status => Err(GithubError::Upstream(format!("unexpected status {status}"))),
        }
    }

    /// Estimate commit count by inspecting the `Link` header of the commits
    /// endpoint (cheap: one request, no bodies transferred).
    #[instrument(skip(self), fields(full_name = %full_name))]
    pub async fn get_commit_count(&self, full_name: &str) -> Result<Option<i64>, GithubError> {
        let url = self.endpoint(&format!("/repos/{full_name}/commits?per_page=1"));

        let mut req = self.http.get(&url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let resp = req.send().await?;

        match resp.status() {
            StatusCode::OK => {
                let link = resp
                    .headers()
                    .get("link")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                drop(resp);
                Ok(parse_link_rel(link.as_deref()))
            }
            StatusCode::NOT_FOUND | StatusCode::GONE => Ok(None),
            _ => Ok(None),
        }
    }
}
