-- ArchiveHub initial schema.
--
-- Design goals:
-- - Fully normalized: owners, repositories, archives, downloads, stats.
-- - IDs are UUIDv4 (app-generated) — keeps sharding/multi-writer possible
--   later and avoids exposing sequential counts.
-- - All status enums are stored as TEXT with CHECK constraints so the
--   canonical set lives in one place (the constraint), not scattered code.
-- - Indexes are chosen for the hot paths: full_name lookups, search filters,
--   and "latest archive per repo" queries.

-- ---------------------------------------------------------------------------
-- Owners (GitHub users & organizations)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS owners (
    id            UUID PRIMARY KEY,
    github_id     BIGINT NOT NULL UNIQUE,
    login         TEXT   NOT NULL,
    name          TEXT,
    avatar_url    TEXT,
    bio           TEXT,
    owner_type    TEXT   NOT NULL DEFAULT 'user', -- 'user' | 'organization'
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (login)
);

-- ---------------------------------------------------------------------------
-- Repositories
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS repositories (
    id                UUID PRIMARY KEY,
    github_id         BIGINT NOT NULL UNIQUE,
    owner_id          UUID NOT NULL REFERENCES owners(id) ON DELETE CASCADE,
    name              TEXT NOT NULL,
    full_name         TEXT NOT NULL UNIQUE,          -- 'owner/repo'
    description       TEXT,
    homepage          TEXT,
    default_branch    TEXT,
    language          TEXT,
    license           TEXT,                           -- SPDX key, e.g. 'MIT'
    topics            TEXT[] NOT NULL DEFAULT '{}',
    stars_count       BIGINT NOT NULL DEFAULT 0,
    forks_count       BIGINT NOT NULL DEFAULT 0,
    watchers_count    BIGINT NOT NULL DEFAULT 0,
    open_issues_count BIGINT NOT NULL DEFAULT 0,
    commit_count      BIGINT NOT NULL DEFAULT 0,
    size_bytes        BIGINT NOT NULL DEFAULT 0,
    source            TEXT NOT NULL DEFAULT 'github',
    visibility        TEXT NOT NULL DEFAULT 'public',
    is_github_archived BOOLEAN NOT NULL DEFAULT false,
    is_deleted        BOOLEAN NOT NULL DEFAULT false,
    deleted_at        TIMESTAMPTZ,
    pushed_at         TIMESTAMPTZ,
    github_created_at TIMESTAMPTZ,
    last_checked_at   TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT fk_repository_owner FOREIGN KEY (owner_id) REFERENCES owners(id),
    CONSTRAINT ck_repositories_source CHECK (source IN ('github')),
    CONSTRAINT ck_repositories_visibility CHECK (visibility IN ('public', 'private', 'internal'))
);

-- Hot lookup paths.
CREATE INDEX IF NOT EXISTS idx_repositories_full_name ON repositories (full_name);
CREATE INDEX IF NOT EXISTS idx_repositories_owner_id   ON repositories (owner_id);
CREATE INDEX IF NOT EXISTS idx_repositories_language   ON repositories (language);
CREATE INDEX IF NOT EXISTS idx_repositories_license    ON repositories (license);
CREATE INDEX IF NOT EXISTS idx_repositories_stars      ON repositories (stars_count DESC);
CREATE INDEX IF NOT EXISTS idx_repositories_deleted    ON repositories (is_deleted) WHERE is_deleted = true;
CREATE INDEX IF NOT EXISTS idx_repositories_topics     ON repositories USING GIN (topics);

-- ---------------------------------------------------------------------------
-- Archives (point-in-time immutable snapshots)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS archives (
    id                UUID PRIMARY KEY,
    repository_id     UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    commit_ref        TEXT,                            -- git sha/ref captured
    commit_count      BIGINT,
    checksum          TEXT,                            -- sha256 hex of stored object
    size_bytes        BIGINT NOT NULL DEFAULT 0,
    storage_provider  TEXT NOT NULL DEFAULT 'local',   -- 'local' | 'r2'
    storage_key       TEXT NOT NULL,                   -- object key
    compression_method TEXT NOT NULL DEFAULT 'git_bundle',
    status            TEXT NOT NULL DEFAULT 'pending',
    deleted_at        TIMESTAMPTZ,
    error_message     TEXT,
    archived_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_archives_status CHECK (status IN
        ('pending', 'fetching', 'processing', 'archived', 'verification_failed', 'failed')),
    CONSTRAINT ck_archives_compression CHECK (compression_method IN
        ('zip', 'git_bundle', 'tar', 'custom'))
);

-- Latest archive per repo (hot path when serving a deleted repo).
CREATE INDEX IF NOT EXISTS idx_archives_repository
    ON archives (repository_id, archived_at DESC);
CREATE INDEX IF NOT EXISTS idx_archives_status ON archives (status) WHERE status = 'archived';

-- ---------------------------------------------------------------------------
-- Downloads (per-archive analytics)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS downloads (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    archive_id  UUID NOT NULL REFERENCES archives(id) ON DELETE CASCADE,
    ip_hash     TEXT,                       -- hashed, not raw, IPs
    user_agent  TEXT,
    bytes_sent  BIGINT NOT NULL DEFAULT 0,
    downloaded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_downloads_archive ON downloads (archive_id);
CREATE INDEX IF NOT EXISTS idx_downloads_date ON downloads (downloaded_at);

-- ---------------------------------------------------------------------------
-- Repository statistics (daily snapshots of stars/forks/...)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS repo_stats (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id    UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    date             DATE NOT NULL,
    stars_count      BIGINT NOT NULL,
    forks_count      BIGINT NOT NULL,
    watchers_count   BIGINT NOT NULL,
    open_issues_count BIGINT NOT NULL,
    recorded_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (repository_id, date)
);

CREATE INDEX IF NOT EXISTS idx_repo_stats_repository ON repo_stats (repository_id, date);

-- ---------------------------------------------------------------------------
-- Crawler job bookkeeping (so workers can dedupe + resume)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS crawl_jobs (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id    UUID REFERENCES repositories(id) ON DELETE CASCADE,
    archive_id       UUID REFERENCES archives(id) ON DELETE CASCADE,
    job_type         TEXT NOT NULL,          -- 'archive' | 'refresh' | 'verify'
    status           TEXT NOT NULL DEFAULT 'pending',
    attempts         INTEGER NOT NULL DEFAULT 0,
    max_attempts     INTEGER NOT NULL DEFAULT 5,
    last_error       TEXT,
    next_run_at      TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_crawl_jobs_status CHECK (status IN ('pending', 'running', 'done', 'failed', 'dead'))
);

CREATE INDEX IF NOT EXISTS idx_crawl_jobs_status ON crawl_jobs (status, next_run_at);
