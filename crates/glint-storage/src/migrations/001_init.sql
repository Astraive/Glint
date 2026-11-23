-- Migration 001 (INS-002): packages, grants, profiles and install locks.
--
-- Applied inside one transaction together with `PRAGMA user_version = 1`;
-- a failure leaves the database exactly as it was. Never edit an applied
-- migration: add a new numbered file instead.

-- Installed packages. One row per release in the content-addressed store;
-- `digest` (lowercase hex SHA-256 of the verified archive) is the store key.
CREATE TABLE packages (
    digest          TEXT PRIMARY KEY NOT NULL
                    CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    kind            TEXT NOT NULL,
    package_id      TEXT NOT NULL,
    version         TEXT NOT NULL,
    -- Directory name inside the store (never an absolute path, so the data
    -- directory can move).
    store_name      TEXT NOT NULL UNIQUE,
    archive_size    INTEGER NOT NULL CHECK (archive_size >= 0),
    -- Source kind (`local-archive`, `git`, ...) and canonical locator, as
    -- recorded by glint-sources. Full provenance is INS-004's.
    source_kind     TEXT NOT NULL,
    source_locator  TEXT NOT NULL,
    installed_at    INTEGER NOT NULL
) STRICT;

CREATE INDEX packages_by_identity ON packages (kind, package_id, version);

-- Capability grants, bound to one exact release (digest), never to an id
-- alone. The grant model itself is CAP-004's.
CREATE TABLE grants (
    digest          TEXT NOT NULL REFERENCES packages (digest) ON DELETE CASCADE,
    capability      TEXT NOT NULL,
    granted_at      INTEGER NOT NULL,
    PRIMARY KEY (digest, capability)
) STRICT;

-- Profiles. `base_digest` deliberately has no foreign key: after database
-- recovery the active profile is restored before the package index is
-- rebuilt from the store.
CREATE TABLE profiles (
    profile_id      TEXT PRIMARY KEY NOT NULL CHECK (length(profile_id) > 0),
    display_name    TEXT NOT NULL,
    base_digest     TEXT NOT NULL,
    revision        INTEGER NOT NULL CHECK (revision >= 0),
    updated_at      INTEGER NOT NULL
) STRICT;

-- Install locks: the exact releases each profile pins (version/lock
-- policy). A pinned release cannot be removed.
CREATE TABLE install_locks (
    profile_id      TEXT NOT NULL REFERENCES profiles (profile_id) ON DELETE CASCADE,
    digest          TEXT NOT NULL,
    PRIMARY KEY (profile_id, digest)
) STRICT;

CREATE INDEX install_locks_by_digest ON install_locks (digest);

-- The single active profile.
CREATE TABLE active_profile (
    singleton       INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    profile_id      TEXT NOT NULL REFERENCES profiles (profile_id),
    activated_at    INTEGER NOT NULL
) STRICT;
