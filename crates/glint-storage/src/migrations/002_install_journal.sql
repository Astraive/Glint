-- Migration 002 (INS-003): the install/remove journal.
--
-- A row is committed before the installer touches the store and deleted in
-- the same transaction that commits the package change, so a row that is
-- still present at startup is an interrupted operation for
-- `Installer::recover_pending` to roll back or finish. Paths are stored as
-- directory names relative to the staging, store and trash directories.
CREATE TABLE install_journal (
    txn_id          INTEGER PRIMARY KEY AUTOINCREMENT,
    operation       TEXT NOT NULL CHECK (operation IN ('install', 'remove')),
    digest          TEXT NOT NULL
                    CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    -- install: the staging directory being moved into the store.
    staging_name    TEXT,
    -- remove: the trash directory the store entry is moved to.
    trash_name      TEXT,
    created_at      INTEGER NOT NULL,
    CHECK ((operation = 'install') = (staging_name IS NOT NULL)),
    CHECK ((operation = 'remove') = (trash_name IS NOT NULL))
) STRICT;
