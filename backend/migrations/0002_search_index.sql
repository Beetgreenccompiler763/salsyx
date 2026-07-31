-- Fuzzy / substring search support.
--
-- ArchiveHub's free-tier target is Neon Postgres, which includes the
-- `pg_trgm` extension. Trigram indexes give us fast ILIKE substring matching
-- and `similarity()` for fuzzy ranking without any external search service.
--
-- When search volume grows beyond Postgres, the search route already
-- isolates queries behind `db::search_repositories`, so swapping in a
-- dedicated engine (Typesense/Meilisearch/Tantivy) is a contained change.

CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Trigram GIN indexes on every text field we search.
CREATE INDEX IF NOT EXISTS idx_repositories_full_name_trgm
    ON repositories USING GIN (full_name gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_repositories_name_trgm
    ON repositories USING GIN (name gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_repositories_description_trgm
    ON repositories USING GIN (description gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_owners_login_trgm
    ON owners USING GIN (login gin_trgm_ops);
