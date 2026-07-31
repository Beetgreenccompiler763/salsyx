//! Polling + claiming of `crawl_jobs` rows.
//!
//! The crawler and API server share the Postgres database but not a
//! process. Coordination happens through the `crawl_jobs` table:
//!
//! - Workers claim a job with an atomic `UPDATE ... RETURNING` guarded by
//!   `WHERE status = 'pending' AND (next_run_at IS NULL OR next_run_at <= now())`.
//! - While a job is `running` no other worker can claim it (the guard above).
//! - `attempts` increments on failure; jobs exceeding `max_attempts` become
//!   `dead` so they stop being retried forever.

use chrono::Utc;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// A job row claimed by a worker.
#[derive(Debug, FromRow)]
pub struct CrawlJob {
    pub id: Uuid,
    pub repository_id: Option<Uuid>,
    pub archive_id: Option<Uuid>,
    pub job_type: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub next_run_at: Option<chrono::DateTime<Utc>>,
}

/// Claim one job for execution, if any is available.
///
/// Returns `None` when the queue is empty. A second call from another worker
/// at the same time cannot claim the same job (row lock via the UPDATE).
pub async fn claim_job(pool: &PgPool) -> anyhow::Result<Option<CrawlJob>> {
    let now = Utc::now();

    let job: Option<CrawlJob> = sqlx::query_as(
        r#"
        UPDATE crawl_jobs
        SET status = 'running',
            attempts = attempts + 1,
            updated_at = now()
        WHERE id = (
            SELECT id FROM crawl_jobs
            WHERE status = 'pending'
              AND attempts < max_attempts
              AND (next_run_at IS NULL OR next_run_at <= $1)
            ORDER BY created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING id, repository_id, archive_id, job_type, status, attempts,
                  max_attempts, last_error, next_run_at
        "#,
    )
    .bind(now)
    .fetch_optional(pool)
    .await?;

    Ok(job)
}

/// Record a successful execution.
pub async fn complete_job(pool: &PgPool, job_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE crawl_jobs SET status = 'done', next_run_at = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a failure; schedules a retry with exponential backoff up to
/// `max_attempts`, then marks the job `dead`.
pub async fn fail_job(pool: &PgPool, job_id: Uuid, error: &str) -> anyhow::Result<()> {
    let (attempts, max_attempts): (i32, i32) =
        sqlx::query_as("SELECT attempts, max_attempts FROM crawl_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(pool)
            .await?;

    let status = if attempts >= max_attempts {
        "dead"
    } else {
        "pending"
    };

    // Exponential backoff: 2^attempts minutes.
    let backoff_mins = 1u32 << (attempts as u32).min(6);
    let next_run_at = Utc::now() + chrono::Duration::minutes(backoff_mins as i64);

    sqlx::query(
        r#"
        UPDATE crawl_jobs
        SET status = $2, last_error = $3, next_run_at = $4, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .bind(status)
    .bind(error)
    .bind(next_run_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Enqueue an archive job for a repository (idempotent: skips if a job of
/// the same type for this repo is already pending/running).
pub async fn enqueue_archive_job(
    pool: &PgPool,
    repository_id: Uuid,
    archive_id: Option<Uuid>,
    job_type: &str,
) -> anyhow::Result<()> {
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
