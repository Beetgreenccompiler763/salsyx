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
use serde::{Deserialize, Serialize};
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

/// GitHub user/org profile payload (subset we care about).
#[derive(Debug, Clone, Deserialize)]
pub struct GithubUser {
    pub id: i64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub company: Option<String>,
    pub blog: Option<String>,
    pub location: Option<String>,
    pub twitter_username: Option<String>,
    pub followers: i64,
    pub following: i64,
    #[serde(rename = "public_repos")]
    pub public_repos: i64,
    #[serde(rename = "type")]
    pub user_type: String,
    pub created_at: Option<String>,
}

/// Raw README content returned by the GitHub API.
#[derive(Debug, Clone)]
pub struct ReadmeData {
    pub text: String,
    pub html_url: String,
    #[allow(dead_code)]
    pub download_url: String,
}

/// A repository pinned on an owner's GitHub profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedRepo {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars_count: i64,
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
    fn authorize(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) => req.header("Authorization", format!("Bearer {token}")),
            None => req,
        }
    }

    /// Search repositories via the GitHub Search API.
    ///
    /// Used as a live fallback when the local index has no matches, so
    /// searching for any public repository on GitHub finds it. Results reuse
    /// the full `GithubRepo` shape (the search payload is a repository list).
    #[instrument(skip(self), fields(query = %query))]
    pub async fn search_repositories(
        &self,
        query: &str,
        per_page: i64,
    ) -> Result<Vec<GithubRepo>, GithubError> {
        let encoded = url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>();
        let url = self.endpoint(&format!(
            "/search/repositories?q={encoded}&per_page={per_page}"
        ));

        #[derive(Debug, Deserialize)]
        struct SearchResponse {
            #[allow(dead_code)]
            total_count: i64,
            items: Vec<GithubRepo>,
        }

        let resp = self.authorize(self.http.get(&url)).send().await?;

        match resp.status() {
            StatusCode::OK => {
                let body: SearchResponse = resp.json().await?;
                Ok(body.items)
            }
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => Err(GithubError::RateLimited),
            status => Err(GithubError::Upstream(format!("unexpected status {status}"))),
        }
    }

    /// Fetch a user/organization profile from the REST API.
    #[instrument(skip(self), fields(login = %login))]
    pub async fn get_user(&self, login: &str) -> Result<GithubUser, GithubError> {
        let url = self.endpoint(&format!("/users/{login}"));
        let resp = self.authorize(self.http.get(&url)).send().await?;

        match resp.status() {
            StatusCode::OK => Ok(resp.json().await?),
            StatusCode::NOT_FOUND => Err(GithubError::NotFound),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => Err(GithubError::RateLimited),
            status => Err(GithubError::Upstream(format!("unexpected status {status}"))),
        }
    }

    /// List a user's repositories, most recently pushed first.
    ///
    /// Used by the owner profile endpoint to surface top repos. `per_page`
    /// is capped by GitHub at 100.
    #[instrument(skip(self), fields(login = %login))]
    pub async fn list_user_repos(
        &self,
        login: &str,
        per_page: i64,
    ) -> Result<Vec<GithubRepo>, GithubError> {
        let url = self.endpoint(&format!(
            "/users/{login}/repos?sort=pushed&per_page={per_page}"
        ));
        let resp = self.authorize(self.http.get(&url)).send().await?;

        match resp.status() {
            StatusCode::OK => Ok(resp.json().await?),
            StatusCode::NOT_FOUND => Err(GithubError::NotFound),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => Err(GithubError::RateLimited),
            status => Err(GithubError::Upstream(format!("unexpected status {status}"))),
        }
    }

    /// Fetch the default-branch README as raw text (plus its HTML URL).
    ///
    /// Sends `Accept: application/vnd.github.raw+json` so GitHub returns the
    /// plaintext README instead of a base64 blob.
    #[instrument(skip(self), fields(full_name = %full_name))]
    pub async fn get_readme(&self, full_name: &str) -> Result<ReadmeData, GithubError> {
        let url = self.endpoint(&format!("/repos/{full_name}/readme"));
        let resp = self
            .authorize(
                self.http
                    .get(&url)
                    .header("Accept", "application/vnd.github.raw+json"),
            )
            .send()
            .await?;

        match resp.status() {
            StatusCode::OK => {
                let text = resp.text().await?;
                // GitHub returns the raw README with a redirect to raw.githubusercontent.
                let html_url = format!("https://github.com/{full_name}");
                Ok(ReadmeData {
                    text,
                    html_url,
                    download_url: format!("https://raw.githubusercontent.com/{full_name}/HEAD/"),
                })
            }
            StatusCode::NOT_FOUND => Err(GithubError::NotFound),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => Err(GithubError::RateLimited),
            status => Err(GithubError::Upstream(format!("unexpected status {status}"))),
        }
    }

    /// Fetch the raw bytes of a file from a live repository.
    ///
    /// Uses the Contents API (`base64` body) so we can serve files from live
    /// repos without hitting raw.githubusercontent directly.
    #[instrument(skip(self), fields(full_name = %full_name, path = %path))]
    pub async fn get_file_contents(
        &self,
        full_name: &str,
        path: &str,
        branch: Option<&str>,
    ) -> Result<Option<Vec<u8>>, GithubError> {
        let url = self.endpoint(&format!(
            "/repos/{full_name}/contents/{}?{}",
            url::form_urlencoded::byte_serialize(path.as_bytes()).collect::<String>(),
            branch
                .map(|b| format!(
                    "ref={}",
                    url::form_urlencoded::byte_serialize(b.as_bytes()).collect::<String>()
                ))
                .unwrap_or_default()
        ));

        let resp = self.authorize(self.http.get(&url)).send().await?;

        match resp.status() {
            StatusCode::OK => {
                #[derive(serde::Deserialize)]
                struct ContentsResponse {
                    content: Option<String>,
                    encoding: Option<String>,
                }
                let body: ContentsResponse = resp.json().await?;
                if body.encoding.as_deref() == Some("base64") {
                    if let Some(encoded) = body.content {
                        use base64::Engine;
                        return Ok(Some(
                            base64::engine::general_purpose::STANDARD
                                .decode(encoded.replace('\n', ""))
                                .unwrap_or_default(),
                        ));
                    }
                }
                Ok(None)
            }
            StatusCode::NOT_FOUND => Ok(None),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => Err(GithubError::RateLimited),
            _ => Err(GithubError::Upstream(format!(
                "unexpected status {}",
                resp.status()
            ))),
        }
    }

    /// Fetch pinned repositories via the GitHub GraphQL API.
    ///
    /// Anonymous GraphQL is not allowed, so this returns an empty vec when no
    /// token is configured (the REST `list_user_repos` result is the fallback).
    #[instrument(skip(self), fields(login = %login))]
    pub async fn get_pinned_repos(&self, login: &str) -> Result<Vec<PinnedRepo>, GithubError> {
        let Some(token) = &self.token else {
            return Ok(Vec::new());
        };

        let query = r#"query($login: String!) {
            user(login: $login) {
                pinnedItems(first: 6, types: REPOSITORY) {
                    nodes {
                        ... on Repository {
                            name
                            nameWithOwner
                            description
                            primaryLanguage { name }
                            stargazerCount
                        }
                    }
                }
            }
        }"#;

        #[derive(serde::Serialize)]
        struct GraphqlBody<'a> {
            query: &'a str,
            variables: serde_json::Value,
        }

        let resp = self
            .http
            .post(self.endpoint("/graphql"))
            .header("Authorization", format!("Bearer {token}"))
            .json(&GraphqlBody {
                query,
                variables: serde_json::json!({ "login": login }),
            })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(GithubError::Upstream(format!(
                "graphql returned {}",
                resp.status()
            )));
        }

        #[derive(serde::Deserialize)]
        struct GraphqlResponse {
            #[allow(dead_code)]
            errors: Option<serde_json::Value>,
            data: Option<GraphqlData>,
        }
        #[derive(serde::Deserialize)]
        struct GraphqlData {
            user: Option<GraphqlUser>,
        }
        #[derive(serde::Deserialize)]
        struct GraphqlUser {
            pinned_items: Option<GraphqlPinned>,
        }
        #[derive(serde::Deserialize)]
        struct GraphqlPinned {
            nodes: Vec<GraphqlNode>,
        }
        #[derive(serde::Deserialize)]
        struct GraphqlNode {
            name: String,
            name_with_owner: String,
            description: Option<String>,
            primary_language: Option<GraphqlLang>,
            stargazer_count: i64,
        }
        #[derive(serde::Deserialize)]
        struct GraphqlLang {
            name: String,
        }

        let body: GraphqlResponse = resp.json().await?;
        let Some(user) = body.data.and_then(|d| d.user) else {
            return Ok(Vec::new());
        };
        let Some(pinned) = user.pinned_items else {
            return Ok(Vec::new());
        };

        Ok(pinned
            .nodes
            .into_iter()
            .map(|n| PinnedRepo {
                name: n.name,
                full_name: n.name_with_owner,
                description: n.description,
                language: n.primary_language.map(|l| l.name),
                stars_count: n.stargazer_count,
            })
            .collect())
    }

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
