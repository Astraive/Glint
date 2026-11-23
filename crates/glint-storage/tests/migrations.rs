//! Migrations and failure recovery (INS-002): an injected migration failure
//! rolls back and the active profile stays readable; a corrupt database is
//! quarantined and recreated without losing the previously active theme.
//!
//! Every database lives in a temporary directory.
//!
//! Run with `cargo test -p glint-storage --test migrations`.

use std::fs;

use glint_storage::rusqlite::Connection;
use glint_storage::{
    ErrorCategory, ErrorCode, GlintPaths, MIGRATIONS, Migration, Storage, latest_schema_version,
    read_active_theme,
};

const DIGEST_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DIGEST_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn temp_paths() -> (tempfile::TempDir, GlintPaths) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = GlintPaths::under_root(tmp.path().join("glint")).unwrap();
    (tmp, paths)
}

fn user_version(paths: &GlintPaths) -> i64 {
    let conn = Connection::open(paths.database_file()).unwrap();
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn table_exists(paths: &GlintPaths, table: &str) -> bool {
    let conn = Connection::open(paths.database_file()).unwrap();
    conn.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        == 1
}

/// The built-in migrations plus one more that the test controls.
fn with_extra(extra: Migration) -> Vec<Migration> {
    let mut all = MIGRATIONS.to_vec();
    all.push(extra);
    all
}

#[test]
fn fresh_database_is_migrated_to_the_latest_schema() {
    let (_tmp, paths) = temp_paths();
    let storage = Storage::open(&paths).unwrap();
    assert_eq!(storage.schema_version(), latest_schema_version());
    assert!(storage.recovery().is_none());
    assert_eq!(storage.active_theme().unwrap(), None);
    drop(storage);
    for table in [
        "packages",
        "grants",
        "profiles",
        "install_locks",
        "active_profile",
    ] {
        assert!(table_exists(&paths, table), "{table}");
    }
    assert_eq!(user_version(&paths), i64::from(latest_schema_version()));
}

#[test]
fn reopening_is_idempotent() {
    let (_tmp, paths) = temp_paths();
    Storage::open(&paths)
        .unwrap()
        .set_active_theme("default", DIGEST_A)
        .unwrap();
    let storage = Storage::open(&paths).unwrap();
    assert_eq!(storage.schema_version(), latest_schema_version());
    assert_eq!(
        storage.active_theme().unwrap().unwrap().package_digest,
        DIGEST_A
    );
}

#[test]
fn activation_bumps_the_revision_and_mirrors_the_record() {
    let (_tmp, paths) = temp_paths();
    let mut storage = Storage::open(&paths).unwrap();
    let first = storage.set_active_theme("default", DIGEST_A).unwrap();
    assert_eq!(first.revision, 1);
    let second = storage.set_active_theme("default", DIGEST_B).unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(second.package_digest, DIGEST_B);
    let record = fs::read_to_string(paths.active_record_file()).unwrap();
    assert!(record.contains(DIGEST_B), "{record}");
    assert!(record.contains("revision=2"), "{record}");
}

#[test]
fn invalid_profile_ids_and_digests_are_refused() {
    let (_tmp, paths) = temp_paths();
    let mut storage = Storage::open(&paths).unwrap();
    for (profile, digest) in [
        ("", DIGEST_A),
        ("bad\nid", DIGEST_A),
        ("default", "not-a-digest"),
        ("default", &DIGEST_A.to_uppercase()),
    ] {
        let error = storage.set_active_theme(profile, digest).unwrap_err();
        assert_eq!(
            error.code(),
            ErrorCode::InvalidValue,
            "{profile:?} {digest}"
        );
    }
    assert_eq!(storage.active_theme().unwrap(), None);
}

#[test]
fn injected_migration_failure_rolls_back_and_the_active_profile_stays_readable() {
    let (_tmp, paths) = temp_paths();
    Storage::open(&paths)
        .unwrap()
        .set_active_theme("default", DIGEST_A)
        .unwrap();

    // The first statement succeeds, the second fails: the whole migration
    // must roll back, including the table the first statement created.
    let failing = Migration {
        version: latest_schema_version() + 1,
        name: "injected-failure",
        sql: "CREATE TABLE half_done (x INTEGER); \
              ALTER TABLE packages ADD COLUMN digest TEXT;",
        tables: &["half_done"],
    };
    let error = Storage::open_with_migrations(&paths, &with_extra(failing)).unwrap_err();
    assert_eq!(error.code(), ErrorCode::MigrationFailed, "{error}");
    assert!(error.to_string().contains("injected-failure"), "{error}");

    assert_eq!(user_version(&paths), i64::from(latest_schema_version()));
    assert!(!table_exists(&paths, "half_done"));

    // Readable without migrating, from the database itself.
    let active = read_active_theme(&paths).expect("active theme readable");
    assert_eq!(active.profile_id, "default");
    assert_eq!(active.package_digest, DIGEST_A);

    // And the current build still opens it normally.
    let storage = Storage::open(&paths).unwrap();
    assert!(storage.recovery().is_none());
    assert_eq!(storage.active_theme().unwrap(), Some(active));
}

#[test]
fn a_migration_that_skips_its_tables_is_a_failure() {
    let (_tmp, paths) = temp_paths();
    Storage::open(&paths).unwrap();
    let incomplete = Migration {
        version: latest_schema_version() + 1,
        name: "forgot-table",
        sql: "CREATE TABLE something_else (x INTEGER);",
        tables: &["promised_table"],
    };
    let error = Storage::open_with_migrations(&paths, &with_extra(incomplete)).unwrap_err();
    assert_eq!(error.code(), ErrorCode::MigrationFailed);
    assert!(!table_exists(&paths, "something_else"));
}

#[test]
fn a_successful_extra_migration_is_applied_once() {
    let (_tmp, paths) = temp_paths();
    let extra = Migration {
        version: latest_schema_version() + 1,
        name: "extra",
        sql: "CREATE TABLE extra (x INTEGER);",
        tables: &["extra"],
    };
    let migrations = with_extra(extra);
    let storage = Storage::open_with_migrations(&paths, &migrations).unwrap();
    assert_eq!(storage.schema_version(), extra.version);
    drop(storage);
    // A second open finds nothing pending (re-running would fail: the
    // table already exists).
    Storage::open_with_migrations(&paths, &migrations).unwrap();
}

#[test]
fn a_newer_schema_is_left_untouched() {
    let (_tmp, paths) = temp_paths();
    Storage::open(&paths).unwrap();
    let conn = Connection::open(paths.database_file()).unwrap();
    conn.pragma_update(None, "user_version", 999).unwrap();
    drop(conn);
    let before = fs::read(paths.database_file()).unwrap();
    let error = Storage::open(&paths).unwrap_err();
    assert_eq!(error.code(), ErrorCode::SchemaTooNew);
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(fs::read(paths.database_file()).unwrap(), before);
}

#[test]
fn a_garbage_database_is_quarantined_and_the_active_theme_restored() {
    let (_tmp, paths) = temp_paths();
    Storage::open(&paths)
        .unwrap()
        .set_active_theme("work", DIGEST_B)
        .unwrap();
    let garbage = b"this is definitely not an SQLite database file, just noise".repeat(100);
    fs::write(paths.database_file(), &garbage).unwrap();

    let storage = Storage::open(&paths).unwrap();
    let recovery = storage.recovery().expect("recovery reported");
    assert!(
        recovery.reason.contains("unreadable"),
        "{}",
        recovery.reason
    );
    assert!(paths.contains(&recovery.quarantined));
    assert_eq!(fs::read(&recovery.quarantined).unwrap(), garbage);
    let restored = recovery.restored_active.clone().expect("active restored");
    assert_eq!(restored.profile_id, "work");
    assert_eq!(restored.package_digest, DIGEST_B);
    assert_eq!(storage.active_theme().unwrap(), Some(restored));
    assert_eq!(storage.schema_version(), latest_schema_version());
}

#[test]
fn damaged_pages_are_detected_and_recovered() {
    let (_tmp, paths) = temp_paths();
    let mut storage = Storage::open(&paths).unwrap();
    storage.set_active_theme("default", DIGEST_A).unwrap();
    // Grow the file past a few pages so there is something to damage.
    storage
        .connection()
        .execute_batch(
            "CREATE TABLE filler (x BLOB);
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 200)
             INSERT INTO filler SELECT randomblob(1000) FROM n;",
        )
        .unwrap();
    drop(storage);
    let mut bytes = fs::read(paths.database_file()).unwrap();
    // Keep the 100-byte header valid; overwrite the schema page and beyond.
    for byte in bytes.iter_mut().skip(100).take(16 * 1024) {
        *byte = 0xA5;
    }
    fs::write(paths.database_file(), &bytes).unwrap();

    let storage = Storage::open(&paths).unwrap();
    let recovery = storage.recovery().expect("recovery reported");
    assert_eq!(
        recovery.restored_active.as_ref().unwrap().package_digest,
        DIGEST_A
    );
    assert_eq!(
        storage.active_theme().unwrap().unwrap().package_digest,
        DIGEST_A
    );
}

#[test]
fn a_schema_missing_tables_from_an_applied_migration_is_recovered() {
    let (_tmp, paths) = temp_paths();
    Storage::open(&paths)
        .unwrap()
        .set_active_theme("default", DIGEST_A)
        .unwrap();
    // Simulate a corrupt earlier migration: version 1 recorded, table gone.
    let conn = Connection::open(paths.database_file()).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF; DROP TABLE install_locks;")
        .unwrap();
    drop(conn);

    let storage = Storage::open(&paths).unwrap();
    let recovery = storage.recovery().expect("recovery reported");
    assert!(
        recovery.reason.contains("install_locks"),
        "{}",
        recovery.reason
    );
    assert_eq!(
        storage.active_theme().unwrap().unwrap().package_digest,
        DIGEST_A
    );
    drop(storage);
    assert!(table_exists(&paths, "install_locks"));
}

#[test]
fn corruption_without_a_record_recovers_to_an_empty_database() {
    let (_tmp, paths) = temp_paths();
    paths.ensure_dirs().unwrap();
    fs::write(paths.database_file(), b"garbage garbage garbage garbage").unwrap();
    let storage = Storage::open(&paths).unwrap();
    let recovery = storage.recovery().expect("recovery reported");
    assert_eq!(recovery.restored_active, None);
    assert_eq!(storage.active_theme().unwrap(), None);
}

#[test]
fn a_stale_record_is_brought_up_to_date_on_open() {
    let (_tmp, paths) = temp_paths();
    let mut storage = Storage::open(&paths).unwrap();
    storage.set_active_theme("default", DIGEST_A).unwrap();
    // A crash between commit and record write leaves an old record.
    let old = fs::read_to_string(paths.active_record_file()).unwrap();
    storage.set_active_theme("default", DIGEST_B).unwrap();
    drop(storage);
    fs::write(paths.active_record_file(), old).unwrap();

    Storage::open(&paths).unwrap();
    let record = fs::read_to_string(paths.active_record_file()).unwrap();
    assert!(record.contains(DIGEST_B), "{record}");
}

#[test]
fn a_malformed_record_is_ignored_not_trusted() {
    let (_tmp, paths) = temp_paths();
    paths.ensure_dirs().unwrap();
    fs::write(
        paths.active_record_file(),
        "glint-active-theme 1\nprofile=x\ndigest=../../etc\nrevision=1\n",
    )
    .unwrap();
    fs::write(paths.database_file(), b"garbage garbage garbage garbage").unwrap();
    let storage = Storage::open(&paths).unwrap();
    assert_eq!(storage.recovery().unwrap().restored_active, None);
    assert_eq!(read_active_theme(&paths), None);
}
