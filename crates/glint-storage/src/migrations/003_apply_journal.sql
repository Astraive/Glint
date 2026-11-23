-- Migration 003 (INS-005): the apply journal.
--
-- Written before an apply changes anything: the prior Glint state (the
-- active profile snapshot) and every native preference an adapter plans to
-- change, with the adapter's snapshot of its current value and its
-- canSnapshot/canRestore semantics. The rows are deleted in the same
-- transaction that commits the new active profile, so rows present at
-- startup mean an apply was interrupted and must be rolled back.

-- At most one apply is in flight.
CREATE TABLE apply_journal (
    singleton           INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    -- Snapshot of the active profile before the apply (NULL: none active).
    prior_profile_id    TEXT,
    prior_digest        TEXT,
    prior_revision      INTEGER,
    target_profile_id   TEXT NOT NULL,
    target_digest       TEXT NOT NULL,
    started_at          INTEGER NOT NULL
) STRICT;

-- Planned native changes, in application order.
CREATE TABLE apply_native (
    seq                 INTEGER PRIMARY KEY NOT NULL,
    adapter             TEXT NOT NULL,
    setting             TEXT NOT NULL,
    -- The adapter's snapshot of the current value (NULL when the adapter
    -- cannot snapshot).
    prior_value         TEXT,
    new_value           TEXT NOT NULL,
    can_snapshot        INTEGER NOT NULL CHECK (can_snapshot IN (0, 1)),
    can_restore         INTEGER NOT NULL CHECK (can_restore IN (0, 1)),
    -- planned: not touched; applying: the adapter may have changed it;
    -- applied: the adapter reported success.
    state               TEXT NOT NULL CHECK (state IN ('planned', 'applying', 'applied')),
    UNIQUE (adapter, setting)
) STRICT;
