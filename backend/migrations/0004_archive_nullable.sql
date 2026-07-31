-- Allow pending archives to exist before the crawler fills in storage info.
--
-- A freshly enqueued archive has no blob yet: `storage_key`, `checksum`,
-- and `size_bytes` only become known once the pipeline runs. Enforce
-- presence at the application layer (crawler finalization) instead of the
-- schema so `INSERT ... status='pending'` stays simple and atomic.

ALTER TABLE archives
    ALTER COLUMN storage_key DROP NOT NULL,
    ALTER COLUMN checksum    DROP NOT NULL;
