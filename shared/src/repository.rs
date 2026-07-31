//! Repository domain types.
//!
//! A repository is the central entity: everything else (archives,
//! statistics, search index rows) hangs off a repository record.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Where the repository originates from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositorySource {
    /// Public GitHub repository.
    Github,
    // Future sources (GitLab, Gitea, ...) can be appended here.
}

/// Visibility of a public repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    // `private` / `internal` are not archived by ArchiveHub but the enum
    // allows the schema to store them faithfully if ever needed.
    Private,
    Internal,
}

/// A GitHub (or future source) account that owns repositories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryOwner {
    pub id: Uuid,
    /// Numeric GitHub user id.
    pub github_id: i64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    /// `user` or `organization`.
    pub owner_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The central repository record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: Uuid,
    pub owner: RepositoryOwner,
    /// Numeric GitHub repository id.
    pub github_id: i64,
    pub name: String,
    /// `owner/login` + `/` + `name`, unique across the platform.
    pub full_name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub default_branch: Option<String>,
    pub language: Option<String>,
    /// SPDX license key, e.g. `MIT`.
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub stars_count: i64,
    pub forks_count: i64,
    pub watchers_count: i64,
    pub open_issues_count: i64,
    pub commit_count: i64,
    pub size_bytes: i64,
    pub source: RepositorySource,
    pub visibility: Visibility,
    /// GitHub's own archived flag (a repository frozen on GitHub).
    pub is_github_archived: bool,
    /// Set to `true` once the source starts returning 404 for this repo.
    pub is_deleted: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub pushed_at: Option<DateTime<Utc>>,
    pub github_created_at: Option<DateTime<Utc>>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
