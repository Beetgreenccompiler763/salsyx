//! Salsyx crawler — entry point.
//!
//! Runs a pool of workers that poll `crawl_jobs` and execute the archive
//! pipeline. Independent process from the API server; shares only the
//! database and the storage backend.

use std::sync::Arc;

use salsyx_api::config::Config;
use salsyx_crawler::{jobs, pipeline, DEFAULT_CONCURRENCY};
use sqlx::PgPool;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let config = Config::load()?;
    init_tracing(&config.app.env);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "starting salsyx-crawler"
    );

    let pool = salsyx_api::db::connect(&config.database).await?;
    let storage: Arc<dyn salsyx_api::storage::Storage> =
        Arc::from(salsyx_api::storage::from_config(&config.storage)?);

    let concurrency = std::env::var("AH_CRAWLER_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_CONCURRENCY);

    info!(concurrency, "spawning worker pool");

    let mut handles = Vec::new();
    for i in 0..concurrency {
        let pool = pool.clone();
        let storage = storage.clone();
        handles.push(tokio::spawn(async move {
            worker_loop(i, pool, storage.as_ref()).await;
        }));
    }

    for h in handles {
        h.await?;
    }

    Ok(())
}

/// Infinite worker loop: claim a job, execute it, retry with backoff.
async fn worker_loop(id: usize, pool: PgPool, storage: &dyn salsyx_api::storage::Storage) {
    info!(worker = id, "worker online");

    loop {
        match jobs::claim_job(&pool).await {
            Ok(Some(job)) => {
                info!(worker = id, job_id = %job.id, job_type = %job.job_type, "processing job");
                let result = execute_job(&pool, storage, &job).await;
                match result {
                    Ok(()) => {
                        if let Err(e) = jobs::complete_job(&pool, job.id).await {
                            tracing::error!(worker = id, job_id = %job.id, error = %e, "failed to complete job");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(worker = id, job_id = %job.id, error = %e, "job failed; scheduling retry");
                        if let Err(e) = jobs::fail_job(&pool, job.id, &e.to_string()).await {
                            tracing::error!(worker = id, job_id = %job.id, error = %e, "failed to mark job failed");
                        }
                    }
                }
            }
            Ok(None) => {
                // No jobs; poll again after a short interval.
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            Err(e) => {
                tracing::error!(worker = id, error = %e, "error claiming job");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// Dispatch a claimed job to the right handler.
async fn execute_job(
    pool: &PgPool,
    storage: &dyn salsyx_api::storage::Storage,
    job: &jobs::CrawlJob,
) -> anyhow::Result<()> {
    match job.job_type.as_str() {
        "archive" => {
            let repo_id = job
                .repository_id
                .ok_or_else(|| anyhow::anyhow!("archive job missing repository_id"))?;
            let archive_id = job
                .archive_id
                .ok_or_else(|| anyhow::anyhow!("archive job missing archive_id"))?;
            let row: (String,) = sqlx::query_as("SELECT full_name FROM repositories WHERE id = $1")
                .bind(repo_id)
                .fetch_one(pool)
                .await?;
            pipeline::archive_repository(pool, storage, archive_id, &row.0, repo_id).await
        }
        other => Err(anyhow::anyhow!("unknown job type: {other}")),
    }
}

fn init_tracing(env: &str) {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if env == "production" {
            EnvFilter::new("salsyx_crawler=info,info")
        } else {
            EnvFilter::new("salsyx_crawler=debug,debug")
        }
    });

    if env == "production" {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().pretty())
            .init();
    }
}
