-- 0006_performance_indexes.sql
-- Performance indexes for the most common query paths:
--   * latest_archive / list_archives  -> (repository_id, status, archived_at DESC)
--   * search filters                  -> (owner_id, stars_count DESC)
--   * stats / admin overview          -> partial count on status = 'archived'
--   * crawl scheduler                 -> (status, next_run_at)

-- Cover the "latest archive per repository" and archive-history queries.
CREATE INDEX IF NOT EXISTS idx_archives_repository_status_archived_at
    ON archives (repository_id, status, archived_at DESC);

-- Partial index so status = 'archived' counts/aggregates scan only archived rows.
CREATE INDEX IF NOT EXISTS idx_archives_archived_status
    ON archives (status) WHERE status = 'archived';

-- Search: "has archived" lateral join + popular-repo sort.
CREATE INDEX IF NOT EXISTS idx_repositories_owner_stars
    ON repositories (owner_id, stars_count DESC);

-- Crawler scheduler scans for the next due job.
CREATE INDEX IF NOT EXISTS idx_crawl_jobs_status_next_run
    ON crawl_jobs (status, next_run_at) WHERE status IN ('pending', 'running');
