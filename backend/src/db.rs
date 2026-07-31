//! Database access layer (repository pattern).
//!
//! All SQL lives here — handlers never write SQL directly. The layer maps
//! between sqlx row structs and the shared domain types, keeping the rest of
//! the application free of persistence concerns (Clean Architecture).
//!
//! # Compile-time vs runtime queries
//!
//! We deliberately use runtime-typed `query_as` with `FromRow` derives rather
//! than the `query!` macros: this keeps `cargo build` green without a live
//! database, which matters for CI and local iteration.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::config::DatabaseConfig;
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Row structs (persistence shapes)
// ---------------------------------------------------------------------------

#[derive(Debug, FromRow)]
pub struct OwnerRow {
    pub id: Uuid,
    pub github_id: i64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub owner_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct RepoRow {
    pub id: Uuid,
    pub github_id: i64,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub default_branch: Option<String>,
    pub language: Option<String>,
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub stars_count: i64,
    pub forks_count: i64,
    pub watchers_count: i64,
    pub open_issues_count: i64,
    pub commit_count: i64,
    pub size_bytes: i64,
    pub source: String,
    pub visibility: String,
    pub is_github_archived: bool,
    pub is_deleted: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub pushed_at: Option<DateTime<Utc>>,
    pub github_created_at: Option<DateTime<Utc>>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct RepoWithOwnerRow {
    pub id: Uuid,
    pub github_id: i64,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub default_branch: Option<String>,
    pub language: Option<String>,
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub stars_count: i64,
    pub forks_count: i64,
    pub watchers_count: i64,
    pub open_issues_count: i64,
    pub commit_count: i64,
    pub size_bytes: i64,
    pub source: String,
    pub visibility: String,
    pub is_github_archived: bool,
    pub is_deleted: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub pushed_at: Option<DateTime<Utc>>,
    pub github_created_at: Option<DateTime<Utc>>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub owner_id: Uuid,
    pub owner_github_id: i64,
    pub owner_login: String,
    pub owner_name: Option<String>,
    pub owner_avatar_url: Option<String>,
    pub owner_bio: Option<String>,
    pub owner_type: String,
    pub owner_created_at: DateTime<Utc>,
    pub owner_updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct ArchiveRow {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub commit_ref: Option<String>,
    pub commit_count: Option<i64>,
    pub checksum: String,
    pub size_bytes: i64,
    pub storage_provider: String,
    pub storage_key: String,
    pub compression_method: String,
    pub status: String,
    pub deleted_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub archived_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct SearchHitRow {
    pub id: Uuid,
    pub owner_login: String,
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
    pub archived_at: Option<DateTime<Utc>>,
    pub html_url: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
pub struct StatsRow {
    pub total_repositories: i64,
    pub archived_repositories: i64,
    pub total_archives: i64,
    pub total_archived_bytes: i64,
    pub total_downloads: i64,
    pub deleted_archived: i64,
    pub total_owners: i64,
    pub indexed_bytes: i64,
}

#[derive(Debug, FromRow)]
pub struct RepoStatRow {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub date: chrono::NaiveDate,
    pub stars_count: i64,
    pub forks_count: i64,
    pub watchers_count: i64,
    pub open_issues_count: i64,
    pub recorded_at: DateTime<Utc>,
}

/// Repository counts grouped by language (for `/stats` breakdowns).
#[derive(Debug, FromRow)]
pub struct LanguageRow {
    pub language: String,
    pub count: i64,
}

/// Highest-starred repositories (for `/stats/top`).
#[derive(Debug, FromRow)]
pub struct TopRepoRow {
    pub full_name: String,
    pub owner_login: String,
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars_count: i64,
    pub is_deleted: bool,
    pub has_archive: bool,
}

// ---------------------------------------------------------------------------
// Pool construction
// ---------------------------------------------------------------------------

/// Create a Postgres connection pool from config.
pub async fn connect(config: &DatabaseConfig) -> Result<PgPool, AppError> {
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    let options: PgConnectOptions = config.url.parse()?;
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(config.acquire_timeout_secs))
        .connect_with(options)
        .await?;

    Ok(pool)
}

/// Run pending embedded migrations on startup.
pub async fn run_migrations(pool: &PgPool) -> Result<(), AppError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    tracing::info!("database migrations applied");
    Ok(())
}

// ---------------------------------------------------------------------------
// Queries — owners & repositories from GitHub payloads
// ---------------------------------------------------------------------------

/// Upsert an owner directly from a GitHub repository payload.
pub async fn upsert_owner_from_github(
    pool: &PgPool,
    repo: &crate::github::GithubRepo,
) -> Result<Uuid, AppError> {
    let id = Uuid::new_v4();
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO owners (id, github_id, login, avatar_url, owner_type)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (github_id) DO UPDATE SET
            login = EXCLUDED.login,
            avatar_url = COALESCE(EXCLUDED.avatar_url, owners.avatar_url),
            owner_type = EXCLUDED.owner_type,
            updated_at = now()
         RETURNING id",
    )
    .bind(id)
    .bind(repo.owner.id)
    .bind(&repo.owner.login)
    .bind(&repo.owner.avatar_url)
    .bind(&repo.owner.owner_type)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Update the (best-effort) commit count for a repository.
pub async fn set_commit_count(
    pool: &PgPool,
    repository_id: Uuid,
    commit_count: i64,
) -> Result<(), AppError> {
    sqlx::query("UPDATE repositories SET commit_count = $2, updated_at = now() WHERE id = $1")
        .bind(repository_id)
        .bind(commit_count)
        .execute(pool)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Queries — repositories
// ---------------------------------------------------------------------------

/// Upsert an owner; returns its id. Uses `ON CONFLICT (github_id)`.
pub async fn upsert_owner(pool: &PgPool, owner: &RepoWithOwnerRow) -> Result<Uuid, AppError> {
    let id = Uuid::new_v4();
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO owners (id, github_id, login, name, avatar_url, bio, owner_type)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (github_id) DO UPDATE SET
            login = EXCLUDED.login,
            name = COALESCE(EXCLUDED.name, owners.name),
            avatar_url = COALESCE(EXCLUDED.avatar_url, owners.avatar_url),
            bio = COALESCE(EXCLUDED.bio, owners.bio),
            owner_type = EXCLUDED.owner_type,
            updated_at = now()
         RETURNING id",
    )
    .bind(id)
    .bind(owner.owner_github_id)
    .bind(&owner.owner_login)
    .bind(&owner.owner_name)
    .bind(&owner.owner_avatar_url)
    .bind(&owner.owner_bio)
    .bind(&owner.owner_type)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Upsert a repository from GitHub metadata. Returns the repo id.
pub async fn upsert_repository(
    pool: &PgPool,
    owner_id: Uuid,
    repo: &crate::github::GithubRepo,
) -> Result<Uuid, AppError> {
    let license = repo.license.as_ref().and_then(|l| l.spdx_id.clone());
    let created_at: Option<DateTime<Utc>> = repo.created_at.as_ref().and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    });
    let pushed_at: Option<DateTime<Utc>> = repo.pushed_at.as_ref().and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    });

    let id = Uuid::new_v4();
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO repositories (
            id, github_id, owner_id, name, full_name, description, homepage,
            default_branch, language, license, topics, stars_count, forks_count,
            watchers_count, open_issues_count, size_bytes, source, visibility,
            is_github_archived, is_deleted, pushed_at, github_created_at,
            last_checked_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                  'github', $17, $18, false, $19, $20, now())
        ON CONFLICT (full_name) DO UPDATE SET
            github_id = EXCLUDED.github_id,
            owner_id = EXCLUDED.owner_id,
            description = EXCLUDED.description,
            homepage = EXCLUDED.homepage,
            default_branch = EXCLUDED.default_branch,
            language = EXCLUDED.language,
            license = EXCLUDED.license,
            topics = EXCLUDED.topics,
            stars_count = EXCLUDED.stars_count,
            forks_count = EXCLUDED.forks_count,
            watchers_count = EXCLUDED.watchers_count,
            open_issues_count = EXCLUDED.open_issues_count,
            size_bytes = EXCLUDED.size_bytes,
            visibility = EXCLUDED.visibility,
            is_github_archived = EXCLUDED.is_github_archived,
            is_deleted = false,
            deleted_at = NULL,
            pushed_at = EXCLUDED.pushed_at,
            github_created_at = EXCLUDED.github_created_at,
            last_checked_at = now(),
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(repo.id)
    .bind(owner_id)
    .bind(&repo.name)
    .bind(&repo.full_name)
    .bind(&repo.description)
    .bind(&repo.homepage)
    .bind(&repo.default_branch)
    .bind(&repo.language)
    .bind(&license)
    .bind(&repo.topics)
    .bind(repo.stargazers_count)
    .bind(repo.forks_count)
    .bind(repo.watchers_count)
    .bind(repo.open_issues_count)
    .bind(repo.size)
    .bind(&repo.visibility)
    .bind(repo.archived)
    .bind(pushed_at)
    .bind(created_at)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Mark a repository as deleted (GitHub now returns 404).
///
/// Alias-aware: the requested name may be the old name of a renamed repo, so
/// we also mark the canonical row referenced by any matching alias.
pub async fn mark_repository_deleted(pool: &PgPool, full_name: &str) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE repositories SET is_deleted = true, deleted_at = now(), updated_at = now()
         WHERE full_name = $1
            OR id IN (SELECT repository_id FROM repository_aliases WHERE full_name = $1)",
    )
    .bind(full_name)
    .execute(pool)
    .await?;

    Ok(())
}

/// Look up a repository by full name, with its owner attached.
///
/// Rename-aware: if the exact `full_name` is not found, we consult the
/// `repository_aliases` table (populated when GitHub silently redirects a
/// renamed repo) so an old `owner/name` still resolves — even after the
/// repository has been deleted from GitHub.
pub async fn find_repository(
    pool: &PgPool,
    full_name: &str,
) -> Result<Option<RepoWithOwnerRow>, AppError> {
    if let Some(row) = fetch_repo_with_owner_by_full_name(pool, full_name).await? {
        return Ok(Some(row));
    }

    // Alias fallback: requested name is an old name for a renamed repo.
    let repo_id: Option<(Uuid,)> =
        sqlx::query_as("SELECT repository_id FROM repository_aliases WHERE full_name = $1")
            .bind(full_name)
            .fetch_optional(pool)
            .await?;

    match repo_id {
        Some((id,)) => fetch_repo_with_owner_by_id(pool, id).await,
        None => Ok(None),
    }
}

async fn fetch_repo_with_owner_by_full_name(
    pool: &PgPool,
    full_name: &str,
) -> Result<Option<RepoWithOwnerRow>, AppError> {
    let row = sqlx::query_as::<_, RepoWithOwnerRow>(
        r#"
        SELECT
            r.id, r.github_id, r.name, r.full_name, r.description, r.homepage,
            r.default_branch, r.language, r.license, r.topics, r.stars_count,
            r.forks_count, r.watchers_count, r.open_issues_count, r.commit_count,
            r.size_bytes, r.source, r.visibility, r.is_github_archived,
            r.is_deleted, r.deleted_at, r.pushed_at, r.github_created_at,
            r.last_checked_at, r.created_at, r.updated_at,
            o.id AS owner_id, o.github_id AS owner_github_id, o.login AS owner_login,
            o.name AS owner_name, o.avatar_url AS owner_avatar_url,
            o.bio AS owner_bio, o.owner_type, o.created_at AS owner_created_at,
            o.updated_at AS owner_updated_at
        FROM repositories r
        JOIN owners o ON o.id = r.owner_id
        WHERE r.full_name = $1
        "#,
    )
    .bind(full_name)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Fetch a repository + owner by its primary key.
pub async fn find_repository_by_id(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<RepoWithOwnerRow>, AppError> {
    fetch_repo_with_owner_by_id(pool, id).await
}

async fn fetch_repo_with_owner_by_id(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<RepoWithOwnerRow>, AppError> {
    let row = sqlx::query_as::<_, RepoWithOwnerRow>(
        r#"
        SELECT
            r.id, r.github_id, r.name, r.full_name, r.description, r.homepage,
            r.default_branch, r.language, r.license, r.topics, r.stars_count,
            r.forks_count, r.watchers_count, r.open_issues_count, r.commit_count,
            r.size_bytes, r.source, r.visibility, r.is_github_archived,
            r.is_deleted, r.deleted_at, r.pushed_at, r.github_created_at,
            r.last_checked_at, r.created_at, r.updated_at,
            o.id AS owner_id, o.github_id AS owner_github_id, o.login AS owner_login,
            o.name AS owner_name, o.avatar_url AS owner_avatar_url,
            o.bio AS owner_bio, o.owner_type, o.created_at AS owner_created_at,
            o.updated_at AS owner_updated_at
        FROM repositories r
        JOIN owners o ON o.id = r.owner_id
        WHERE r.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Record that `full_name` was observed as an alias (old name) of a repo.
pub async fn upsert_repository_alias(
    pool: &PgPool,
    full_name: &str,
    repository_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO repository_aliases (full_name, repository_id)
         VALUES ($1, $2)
         ON CONFLICT (full_name) DO UPDATE SET repository_id = EXCLUDED.repository_id",
    )
    .bind(full_name)
    .bind(repository_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Look up an owner row by login.
pub async fn find_owner(pool: &PgPool, login: &str) -> Result<Option<OwnerRow>, AppError> {
    let row = sqlx::query_as::<_, OwnerRow>("SELECT * FROM owners WHERE login = $1")
        .bind(login)
        .fetch_optional(pool)
        .await?;

    Ok(row)
}

/// Upsert an owner from a GitHub user/org profile payload.
pub async fn upsert_owner_from_github_user(
    pool: &PgPool,
    user: &crate::github::GithubUser,
) -> Result<Uuid, AppError> {
    let id = Uuid::new_v4();
    let owner_type = if user.user_type.eq_ignore_ascii_case("Organization") {
        "organization"
    } else {
        "user"
    };
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO owners (id, github_id, login, name, avatar_url, bio, owner_type)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (github_id) DO UPDATE SET
            login = EXCLUDED.login,
            name = COALESCE(EXCLUDED.name, owners.name),
            avatar_url = COALESCE(EXCLUDED.avatar_url, owners.avatar_url),
            bio = COALESCE(EXCLUDED.bio, owners.bio),
            owner_type = EXCLUDED.owner_type,
            updated_at = now()
         RETURNING id",
    )
    .bind(id)
    .bind(user.id)
    .bind(&user.login)
    .bind(&user.name)
    .bind(&user.avatar_url)
    .bind(&user.bio)
    .bind(owner_type)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Get the most recent successful archive for a repository.
pub async fn latest_archive(
    pool: &PgPool,
    repository_id: Uuid,
) -> Result<Option<ArchiveRow>, AppError> {
    let row = sqlx::query_as::<_, ArchiveRow>(
        r#"
        SELECT * FROM archives
        WHERE repository_id = $1 AND status = 'archived'
        ORDER BY archived_at DESC
        LIMIT 1
        "#,
    )
    .bind(repository_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// List all archives for a repository (newest first) — powers the archive
/// history feature.
pub async fn list_archives(
    pool: &PgPool,
    repository_id: Uuid,
    limit: i64,
) -> Result<Vec<ArchiveRow>, AppError> {
    let rows = sqlx::query_as::<_, ArchiveRow>(
        r#"
        SELECT * FROM archives
        WHERE repository_id = $1
        ORDER BY archived_at DESC
        LIMIT $2
        "#,
    )
    .bind(repository_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Store the file-tree snapshot captured at archive time.
pub async fn set_archive_tree(
    pool: &PgPool,
    archive_id: Uuid,
    tree: &serde_json::Value,
) -> Result<(), AppError> {
    sqlx::query("UPDATE archives SET file_tree = $2, updated_at = now() WHERE id = $1")
        .bind(archive_id)
        .bind(tree)
        .execute(pool)
        .await?;

    Ok(())
}

/// Fetch the stored file-tree snapshot for an archive.
pub async fn archive_tree(
    pool: &PgPool,
    archive_id: Uuid,
) -> Result<serde_json::Value, AppError> {
    let row: (serde_json::Value,) =
        sqlx::query_as("SELECT file_tree FROM archives WHERE id = $1")
            .bind(archive_id)
            .fetch_one(pool)
            .await?;

    Ok(row.0)
}

/// Number of this owner's repositories that Salsyx has preserved.
pub async fn count_archived_for_owner(pool: &PgPool, owner_id: Uuid) -> Result<i64, AppError> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM repositories r
         WHERE r.owner_id = $1
           AND EXISTS (SELECT 1 FROM archives a
                       WHERE a.repository_id = r.id AND a.status = 'archived')",
    )
    .bind(owner_id)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

// ---------------------------------------------------------------------------
// Queries — README / repo documents
// ---------------------------------------------------------------------------

/// Store the extracted README for a repository, rebuilding the search
/// document (readme + description) so full-text search stays fresh.
pub async fn upsert_readme(
    pool: &PgPool,
    repository_id: Uuid,
    readme: &str,
) -> Result<(), AppError> {
    let description: Option<(Option<String>, Vec<String>)> =
        sqlx::query_as("SELECT description, topics FROM repositories WHERE id = $1")
            .bind(repository_id)
            .fetch_optional(pool)
            .await?;

    let document = match description {
        Some((desc, topics)) => {
            let mut parts = topics;
            if let Some(desc) = desc {
                parts.push(desc);
            }
            parts.push(readme.to_string());
            parts.join("\n\n")
        }
        None => readme.to_string(),
    };

    sqlx::query(
        r#"
        INSERT INTO repo_documents (repository_id, readme, document)
        VALUES ($1, $2, $3)
        ON CONFLICT (repository_id) DO UPDATE SET
            readme = EXCLUDED.readme,
            document = EXCLUDED.document,
            updated_at = now()
        "#,
    )
    .bind(repository_id)
    .bind(readme)
    .bind(document)
    .execute(pool)
    .await?;

    Ok(())
}

/// Fetch the stored README text for a repository, if any.
pub async fn find_readme(
    pool: &PgPool,
    repository_id: Uuid,
) -> Result<Option<String>, AppError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT readme FROM repo_documents WHERE repository_id = $1",
    )
    .bind(repository_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.0))
}

/// Insert a new archive record (status `pending`).
pub async fn create_archive(pool: &PgPool, repository_id: Uuid) -> Result<Uuid, AppError> {
    let id = Uuid::new_v4();
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO archives (id, repository_id, status)
        VALUES ($1, $2, 'pending')
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(repository_id)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Update archive status + error message.
pub async fn update_archive_status(
    pool: &PgPool,
    archive_id: Uuid,
    status: &str,
    error_message: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE archives
        SET status = $2, error_message = $3, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(archive_id)
    .bind(status)
    .bind(error_message)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get archive by id.
pub async fn find_archive(pool: &PgPool, archive_id: Uuid) -> Result<Option<ArchiveRow>, AppError> {
    let row = sqlx::query_as::<_, ArchiveRow>("SELECT * FROM archives WHERE id = $1")
        .bind(archive_id)
        .fetch_optional(pool)
        .await?;

    Ok(row)
}

/// True if the repository already has a pending/in-progress archive job.
pub async fn has_pending_archive(pool: &PgPool, repository_id: Uuid) -> Result<bool, AppError> {
    let row: (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM archives
            WHERE repository_id = $1
              AND status IN ('pending', 'fetching', 'processing')
        )",
    )
    .bind(repository_id)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Create a crawler job row (idempotent per repo+type while active).
pub async fn enqueue_crawl_job(
    pool: &PgPool,
    repository_id: Uuid,
    archive_id: Option<Uuid>,
    job_type: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO crawl_jobs (repository_id, archive_id, job_type)
        SELECT $1, $2, $3
        WHERE NOT EXISTS (
            SELECT 1 FROM crawl_jobs
            WHERE repository_id = $1 AND job_type = $3
              AND status IN ('pending', 'running')
        )
        "#,
    )
    .bind(repository_id)
    .bind(archive_id)
    .bind(job_type)
    .execute(pool)
    .await?;

    Ok(())
}

/// Record a download event.
pub async fn record_download(
    pool: &PgPool,
    archive_id: Uuid,
    ip: &str,
    user_agent: &str,
    bytes_sent: i64,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO downloads (archive_id, ip_hash, user_agent, bytes_sent)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(archive_id)
    .bind(ip)
    .bind(user_agent)
    .bind(bytes_sent)
    .execute(pool)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Queries — search
// ---------------------------------------------------------------------------

/// Execute a search query.
///
/// Uses a trigram index (see migration `0004`) for fast `ILIKE`-style
/// substring matches. The same query shape supports exact, partial, and
/// fuzzy modes by swapping operators.
#[allow(clippy::too_many_arguments)]
pub async fn search_repositories(
    pool: &PgPool,
    q: &str,
    _mode: &str,
    owner: Option<&str>,
    language: Option<&str>,
    license: Option<&str>,
    topics: &[String],
    min_stars: Option<i64>,
    include_deleted: bool,
    include_archived_only: bool,
    sort: &str,
    order: &str,
    page: u64,
    per_page: u64,
) -> Result<(Vec<SearchHitRow>, u64), AppError> {
    let offset = (page.saturating_sub(1)) * per_page;

    // Whitelist the sort column — never interpolate user input into SQL.
    // These are the column names exposed by the `ranked` CTE below.
    let sort_column = match sort {
        "stars" => "stars_count",
        "forks" => "forks_count",
        "name" => "name",
        "updated_at" => "last_checked_at",
        "archived_at" => "archived_at",
        "commit_count" => "commit_count",
        _ => "stars_count",
    };

    let order_dir = if order.eq_ignore_ascii_case("asc") {
        "ASC"
    } else {
        "DESC"
    };

    let query = format!(
        r#"
        WITH ranked AS (
            SELECT
                r.id, o.login AS owner_login, r.name, r.full_name,
                r.description, r.language, r.license, r.topics,
                r.stars_count, r.forks_count, r.is_deleted,
                CASE WHEN a.id IS NOT NULL THEN true ELSE false END AS has_archive,
                a.archived_at, r.last_checked_at,
                'https://github.com/' || r.full_name AS html_url,
                GREATEST(
                    similarity(r.full_name, $1),
                    similarity(r.name, $1),
                    similarity(COALESCE(r.description, ''), $1),
                    similarity(o.login, $1)
                ) AS relevance
            FROM repositories r
            JOIN owners o ON o.id = r.owner_id
            LEFT JOIN LATERAL (
                SELECT a.id, a.archived_at FROM archives a
                WHERE a.repository_id = r.id AND a.status = 'archived'
                ORDER BY a.archived_at DESC LIMIT 1
            ) a ON true
            WHERE
                ($2 = '' OR (r.full_name ILIKE '%' || $2 || '%'
                          OR r.name ILIKE '%' || $2 || '%'
                          OR o.login ILIKE '%' || $2 || '%'
                          OR COALESCE(r.description, '') ILIKE '%' || $2 || '%'))
                AND ($3 IS NULL OR o.login = $3)
                AND ($4 IS NULL OR r.language = $4)
                AND ($5 IS NULL OR r.license = $5)
                AND ($6::text[] IS NULL OR r.topics && $6)
                AND ($7 IS NULL OR r.stars_count >= $7)
                AND ($8 = false OR r.is_deleted = true)
                AND ($9 = false OR a.id IS NOT NULL)
        )
        SELECT * FROM ranked
        ORDER BY {sort_column} {order_dir}, relevance DESC
        LIMIT $10 OFFSET $11
        "#,
    );

    let rows = sqlx::query_as::<_, SearchHitRow>(&query)
        .bind(q)
        .bind(q)
        .bind(owner)
        .bind(language)
        .bind(license)
        .bind(if topics.is_empty() {
            None
        } else {
            Some(topics)
        })
        .bind(min_stars)
        .bind(include_deleted)
        .bind(include_archived_only)
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?;

    // Count query (same filters, no sort/limit).
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM repositories r
        JOIN owners o ON o.id = r.owner_id
        WHERE
            ($1 = '' OR (r.full_name ILIKE '%' || $1 || '%'
                      OR r.name ILIKE '%' || $1 || '%'
                      OR o.login ILIKE '%' || $1 || '%'
                      OR COALESCE(r.description, '') ILIKE '%' || $1 || '%'))
            AND ($2 IS NULL OR o.login = $2)
            AND ($3 IS NULL OR r.language = $3)
            AND ($4 IS NULL OR r.license = $4)
            AND ($5::text[] IS NULL OR r.topics && $5)
            AND ($6 IS NULL OR r.stars_count >= $6)
            AND ($7 = false OR r.is_deleted = true)
        "#,
    )
    .bind(q)
    .bind(owner)
    .bind(language)
    .bind(license)
    .bind(if topics.is_empty() {
        None
    } else {
        Some(topics)
    })
    .bind(min_stars)
    .bind(include_deleted)
    .fetch_one(pool)
    .await?;

    Ok((rows, count.0.max(0) as u64))
}

/// Full-text search over READMEs + descriptions via the `repo_documents`
/// search vector (see migration 0003). Supports the same filters/sorting as
/// `search_repositories` but matches document contents.
#[allow(clippy::too_many_arguments)]
pub async fn search_repositories_fulltext(
    pool: &PgPool,
    q: &str,
    owner: Option<&str>,
    language: Option<&str>,
    min_stars: Option<i64>,
    include_deleted: bool,
    include_archived_only: bool,
    page: u64,
    per_page: u64,
) -> Result<(Vec<SearchHitRow>, u64), AppError> {
    let offset = (page.saturating_sub(1)) * per_page;

    let rows = sqlx::query_as::<_, SearchHitRow>(
        r#"
        WITH ranked AS (
            SELECT
                r.id, o.login AS owner_login, r.name, r.full_name,
                r.description, r.language, r.license, r.topics,
                r.stars_count, r.forks_count, r.is_deleted,
                CASE WHEN a.id IS NOT NULL THEN true ELSE false END AS has_archive,
                a.archived_at, r.last_checked_at,
                'https://github.com/' || r.full_name AS html_url,
                ts_rank_cd(d.search_vector, plainto_tsquery('simple', $1)) AS relevance
            FROM repositories r
            JOIN owners o ON o.id = r.owner_id
            JOIN repo_documents d ON d.repository_id = r.id
            LEFT JOIN LATERAL (
                SELECT a.id, a.archived_at FROM archives a
                WHERE a.repository_id = r.id AND a.status = 'archived'
                ORDER BY a.archived_at DESC LIMIT 1
            ) a ON true
            WHERE d.search_vector @@ plainto_tsquery('simple', $1)
              AND ($2 IS NULL OR o.login = $2)
              AND ($3 IS NULL OR r.language = $3)
              AND ($4 IS NULL OR r.stars_count >= $4)
              AND ($5 = false OR r.is_deleted = true)
              AND ($6 = false OR a.id IS NOT NULL)
        )
        SELECT * FROM ranked
        ORDER BY relevance DESC
        LIMIT $7 OFFSET $8
        "#,
    )
    .bind(q)
    .bind(owner)
    .bind(language)
    .bind(min_stars)
    .bind(include_deleted)
    .bind(include_archived_only)
    .bind(per_page as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM repositories r
        JOIN owners o ON o.id = r.owner_id
        JOIN repo_documents d ON d.repository_id = r.id
        WHERE d.search_vector @@ plainto_tsquery('simple', $1)
          AND ($2 IS NULL OR o.login = $2)
          AND ($3 IS NULL OR r.language = $3)
          AND ($4 IS NULL OR r.stars_count >= $4)
          AND ($5 = false OR r.is_deleted = true)
          AND ($6 = false OR EXISTS (
              SELECT 1 FROM archives a
              WHERE a.repository_id = r.id AND a.status = 'archived'
          ))
        "#,
    )
    .bind(q)
    .bind(owner)
    .bind(language)
    .bind(min_stars)
    .bind(include_deleted)
    .bind(include_archived_only)
    .fetch_one(pool)
    .await?;

    Ok((rows, count.0.max(0) as u64))
}

// ---------------------------------------------------------------------------
// Queries — stats
// ---------------------------------------------------------------------------

/// Aggregate platform statistics.
pub async fn platform_stats(pool: &PgPool) -> Result<StatsRow, AppError> {
    let row = sqlx::query_as::<_, StatsRow>(
        r#"
        SELECT
            (SELECT COUNT(*) FROM repositories) AS total_repositories,
            (SELECT COUNT(*) FROM repositories r
                WHERE EXISTS (SELECT 1 FROM archives a
                              WHERE a.repository_id = r.id
                                AND a.status = 'archived')) AS archived_repositories,
            (SELECT COUNT(*) FROM archives WHERE status = 'archived') AS total_archives,
            COALESCE((SELECT SUM(size_bytes)::bigint FROM archives WHERE status = 'archived'), 0)
                AS total_archived_bytes,
            (SELECT COUNT(*) FROM downloads) AS total_downloads,
            (SELECT COUNT(*) FROM repositories WHERE is_deleted = true
                AND EXISTS (SELECT 1 FROM archives a
                            WHERE a.repository_id = repositories.id
                              AND a.status = 'archived')) AS deleted_archived,
            (SELECT COUNT(*) FROM owners) AS total_owners,
            COALESCE((SELECT SUM(size_bytes)::bigint FROM repositories), 0) AS indexed_bytes
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Repository counts grouped by language, top N.
pub async fn top_languages(pool: &PgPool, limit: i64) -> Result<Vec<LanguageRow>, AppError> {
    let rows = sqlx::query_as::<_, LanguageRow>(
        r#"
        SELECT COALESCE(language, 'unknown') AS language, COUNT(*) AS count
        FROM repositories
        GROUP BY language
        ORDER BY count DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Highest-starred repositories.
pub async fn top_repositories(pool: &PgPool, limit: i64) -> Result<Vec<TopRepoRow>, AppError> {
    let rows = sqlx::query_as::<_, TopRepoRow>(
        r#"
        SELECT r.full_name, o.login AS owner_login, r.name, r.description,
               r.language, r.stars_count, r.is_deleted,
               CASE WHEN a.id IS NOT NULL THEN true ELSE false END AS has_archive
        FROM repositories r
        JOIN owners o ON o.id = r.owner_id
        LEFT JOIN LATERAL (
            SELECT a.id FROM archives a
            WHERE a.repository_id = r.id AND a.status = 'archived'
            ORDER BY a.archived_at DESC LIMIT 1
        ) a ON true
        ORDER BY r.stars_count DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

// ---------------------------------------------------------------------------
// Conversions to domain types
// ---------------------------------------------------------------------------

impl From<OwnerRow> for crate::shared::repository::RepositoryOwner {
    fn from(row: OwnerRow) -> Self {
        crate::shared::repository::RepositoryOwner {
            id: row.id,
            github_id: row.github_id,
            login: row.login,
            name: row.name,
            avatar_url: row.avatar_url,
            bio: row.bio,
            owner_type: row.owner_type,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// Convert a `RepoWithOwnerRow` into a shared `Repository`.
pub fn repository_from_row(row: RepoWithOwnerRow) -> crate::shared::repository::Repository {
    crate::shared::repository::Repository {
        id: row.id,
        owner: crate::shared::repository::RepositoryOwner {
            id: row.owner_id,
            github_id: row.owner_github_id,
            login: row.owner_login,
            name: row.owner_name,
            avatar_url: row.owner_avatar_url,
            bio: row.owner_bio,
            owner_type: row.owner_type,
            created_at: row.owner_created_at,
            updated_at: row.owner_updated_at,
        },
        github_id: row.github_id,
        name: row.name,
        full_name: row.full_name,
        description: row.description,
        homepage: row.homepage,
        default_branch: row.default_branch,
        language: row.language,
        license: row.license,
        topics: row.topics,
        stars_count: row.stars_count,
        forks_count: row.forks_count,
        watchers_count: row.watchers_count,
        open_issues_count: row.open_issues_count,
        commit_count: row.commit_count,
        size_bytes: row.size_bytes,
        source: crate::shared::repository::RepositorySource::Github,
        visibility: match row.visibility.as_str() {
            "private" => crate::shared::repository::Visibility::Private,
            "internal" => crate::shared::repository::Visibility::Internal,
            _ => crate::shared::repository::Visibility::Public,
        },
        is_github_archived: row.is_github_archived,
        is_deleted: row.is_deleted,
        deleted_at: row.deleted_at,
        pushed_at: row.pushed_at,
        github_created_at: row.github_created_at,
        last_checked_at: row.last_checked_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}
