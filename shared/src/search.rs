//! Search query and result types.
//!
//! The search API is intentionally schema-agnostic: the backend decides how
//! to satisfy a query (Postgres trigram index today, full-text / external
//! search engine later) without the frontend knowing the implementation.

use serde::{Deserialize, Serialize};

/// How strict the match should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SearchMode {
    /// Exact / prefix match on name or owner login.
    Exact,
    /// Substring match anywhere in the field.
    #[default]
    Partial,
    /// Trigram-similarity based fuzzy matching.
    Fuzzy,
    /// Future full-text search over README + description.
    FullText,
}

/// Everything a client may filter or sort by.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub mode: Option<SearchMode>,
    pub owner: Option<String>,
    pub language: Option<String>,
    pub license: Option<String>,
    pub topics: Option<Vec<String>>,
    pub min_stars: Option<i64>,
    pub max_stars: Option<i64>,
    pub include_deleted: Option<bool>,
    pub include_archived: Option<bool>,
    /// Sort key, defaults to relevance.
    pub sort: Option<SearchSort>,
    /// `asc` or `desc`, defaults to `desc`.
    pub order: Option<String>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}

/// Fields that can be sorted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSort {
    Relevance,
    Stars,
    Forks,
    Name,
    UpdatedAt,
    ArchivedAt,
    CommitCount,
}

/// One row of search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
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
}

/// Paginated search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub items: Vec<SearchHit>,
}
