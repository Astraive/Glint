-- Migration 004 (INS-004): full provenance per installed release.
--
-- Together with the columns from 001 (kind, package_id, version, digest,
-- archive_size, source_kind, source_locator, store_name) each row records
-- where a release came from: the canonical origin used for identity
-- (ADR 0006, `local`, `registry:<url>`, `git:<url>[#subpath]`,
-- `artifact:<url>`), the registry, the pinned Git commit, the canonical
-- local path, the signing status and when it was retrieved.
ALTER TABLE packages ADD COLUMN origin TEXT NOT NULL DEFAULT 'local';
ALTER TABLE packages ADD COLUMN registry TEXT;
ALTER TABLE packages ADD COLUMN git_commit TEXT;
ALTER TABLE packages ADD COLUMN local_path TEXT;
ALTER TABLE packages ADD COLUMN signing TEXT NOT NULL DEFAULT 'unsigned';
ALTER TABLE packages ADD COLUMN retrieved_at INTEGER NOT NULL DEFAULT 0;

CREATE INDEX packages_by_origin ON packages (kind, package_id, origin);
