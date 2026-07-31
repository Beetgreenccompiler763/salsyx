-- Salsyx: file-tree snapshots + repository rename aliases.
--
-- This migration is additive (never edit earlier migrations). It powers two
-- spec features:
--   1. Browsing the contents of an archived repository (`/archive/{id}/tree`
--      and `/archive/{id}/blob`). The crawler snapshots the tree at archive
--      time and stores it as JSONB so the API never has to open the bundle
--      to render the file listing.
--   2. Repository renames. GitHub silently redirects `old-owner/old-name` to
--      the new canonical name. We store the requested (old) full name as an
--      alias so both `GET /repo/...` lookups and the archive fallback still
--      resolve after the rename — and even after the repo is later deleted.

ALTER TABLE archives
    ADD COLUMN IF NOT EXISTS file_tree JSONB NOT NULL DEFAULT '[]';

CREATE TABLE IF NOT EXISTS repository_aliases (
    full_name     TEXT PRIMARY KEY,             -- requested/old 'owner/repo'
    repository_id UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_repository_aliases_repository
    ON repository_aliases (repository_id);
