-- Full-text search index over READMEs and descriptions.
--
-- This is a *forward-looking* migration: it lays the groundwork for
-- full-text search (spec requirement) while Postgres still hosts the index.
-- A future external search engine can read from `repo_documents` without
-- touching any other table.

CREATE TABLE IF NOT EXISTS repo_documents (
    repository_id UUID PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
    -- Extracted markdown/plaintext of the default-branch README.
    readme        TEXT,
    -- Combined searchable document (description + topics + readme).
    document      TEXT,
    -- Postgres FTS vector for `to_tsquery` full-text queries.
    search_vector tsvector,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_repo_documents_vector
    ON repo_documents USING GIN (search_vector);

-- Helper to keep the search vector in sync on every upsert.
CREATE OR REPLACE FUNCTION refresh_repo_document_vector() RETURNS trigger AS $$
BEGIN
    NEW.search_vector := to_tsvector('simple', COALESCE(NEW.document, ''));
    NEW.updated_at := now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_repo_documents_vector ON repo_documents;
CREATE TRIGGER trg_repo_documents_vector
    BEFORE INSERT OR UPDATE OF document ON repo_documents
    FOR EACH ROW EXECUTE FUNCTION refresh_repo_document_vector();
