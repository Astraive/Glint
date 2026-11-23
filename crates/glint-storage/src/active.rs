//! The active theme and its last-known record beside the database.
//!
//! The database is the source of truth for the active profile. After every
//! committed change the same value is mirrored, by write-to-temporary and
//! rename, into a tiny text record ([`GlintPaths::active_record_file`]). If
//! the database is later found corrupt, recovery restores the active theme
//! from that record instead of losing it.
//!
//! [`GlintPaths::active_record_file`]: crate::GlintPaths::active_record_file

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::StorageError;

const RECORD_HEADER: &str = "glint-active-theme 1";
const MAX_PROFILE_ID_LEN: usize = 128;

/// The active profile and the exact release its base theme uses.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActiveTheme {
    /// The active profile's ID.
    pub profile_id: String,
    /// Content digest (lowercase hex SHA-256) of the base theme release.
    pub package_digest: String,
    /// Profile revision; increases on every activation.
    pub revision: u64,
}

/// Check a profile ID: 1 to 128 bytes, no control characters.
///
/// # Errors
///
/// [`ErrorCode::InvalidValue`](crate::ErrorCode::InvalidValue).
pub fn validate_profile_id(id: &str) -> Result<(), StorageError> {
    let invalid = |reason| StorageError::InvalidValue {
        what: "profile id",
        reason,
    };
    if id.is_empty() {
        return Err(invalid("empty"));
    }
    if id.len() > MAX_PROFILE_ID_LEN {
        return Err(invalid("longer than 128 bytes"));
    }
    if id.chars().any(char::is_control) {
        return Err(invalid("contains a control character"));
    }
    Ok(())
}

/// Check a content digest: 64 lowercase hexadecimal digits.
///
/// # Errors
///
/// [`ErrorCode::InvalidValue`](crate::ErrorCode::InvalidValue).
pub fn validate_digest(digest: &str) -> Result<(), StorageError> {
    if digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(StorageError::InvalidValue {
            what: "package digest",
            reason: "not 64 lowercase hexadecimal digits",
        })
    }
}

/// Read the active theme from an open database.
pub(crate) fn query_active(conn: &Connection) -> rusqlite::Result<Option<ActiveTheme>> {
    conn.query_row(
        "SELECT p.profile_id, p.base_digest, p.revision
           FROM active_profile a JOIN profiles p ON p.profile_id = a.profile_id
          WHERE a.singleton = 1",
        [],
        |row| {
            Ok(ActiveTheme {
                profile_id: row.get(0)?,
                package_digest: row.get(1)?,
                revision: row.get::<_, i64>(2)?.try_into().unwrap_or(0),
            })
        },
    )
    .optional()
}

/// Make `profile_id` (with base release `digest`) the active profile inside
/// the caller's transaction, creating the profile if needed and bumping its
/// revision. The caller commits, then calls
/// [`Storage::write_active_record`](crate::Storage::write_active_record).
///
/// # Errors
///
/// [`ErrorCode::InvalidValue`](crate::ErrorCode::InvalidValue) or
/// [`ErrorCode::Database`](crate::ErrorCode::Database).
pub fn activate_in(
    conn: &Connection,
    profile_id: &str,
    digest: &str,
) -> Result<ActiveTheme, StorageError> {
    validate_profile_id(profile_id)?;
    validate_digest(digest)?;
    let now = crate::unix_now();
    conn.execute(
        "INSERT INTO profiles (profile_id, display_name, base_digest, revision, updated_at)
              VALUES (?1, ?1, ?2, 1, ?3)
         ON CONFLICT (profile_id) DO UPDATE
                 SET base_digest = excluded.base_digest,
                     revision = profiles.revision + 1,
                     updated_at = excluded.updated_at",
        params![profile_id, digest, now],
    )
    .map_err(StorageError::db("upsert profile"))?;
    conn.execute(
        "INSERT INTO active_profile (singleton, profile_id, activated_at) VALUES (1, ?1, ?2)
         ON CONFLICT (singleton) DO UPDATE
                 SET profile_id = excluded.profile_id, activated_at = excluded.activated_at",
        params![profile_id, now],
    )
    .map_err(StorageError::db("set active profile"))?;
    query_active(conn)
        .map_err(StorageError::db("read active profile"))?
        .ok_or(StorageError::DatabaseCorrupt(
            "active profile vanished inside its own transaction".into(),
        ))
}

/// Restore a record into a fresh database (after corruption recovery),
/// keeping its revision.
pub(crate) fn restore_in(conn: &Connection, theme: &ActiveTheme) -> Result<(), StorageError> {
    let now = crate::unix_now();
    let revision = i64::try_from(theme.revision).unwrap_or(i64::MAX);
    conn.execute(
        "INSERT INTO profiles (profile_id, display_name, base_digest, revision, updated_at)
              VALUES (?1, ?1, ?2, ?3, ?4)",
        params![theme.profile_id, theme.package_digest, revision, now],
    )
    .map_err(StorageError::db("restore profile"))?;
    conn.execute(
        "INSERT INTO active_profile (singleton, profile_id, activated_at) VALUES (1, ?1, ?2)",
        params![theme.profile_id, now],
    )
    .map_err(StorageError::db("restore active profile"))?;
    Ok(())
}

/// Write the record atomically: temporary file, flush to disk, rename.
pub(crate) fn write_record(path: &Path, theme: Option<&ActiveTheme>) -> Result<(), StorageError> {
    let Some(theme) = theme else {
        return match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(StorageError::io("remove active-theme record", path)(error)),
        };
    };
    let text = format!(
        "{RECORD_HEADER}\nprofile={}\ndigest={}\nrevision={}\n",
        theme.profile_id, theme.package_digest, theme.revision
    );
    let tmp = path.with_extension("record.tmp");
    let mut file =
        File::create(&tmp).map_err(StorageError::io("write active-theme record", &tmp))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(StorageError::io("write active-theme record", &tmp))?;
    drop(file);
    fs::rename(&tmp, path).map_err(StorageError::io("replace active-theme record", path))
}

/// Read the record. A missing or malformed record is `None`: it is a
/// recovery aid, never trusted over a healthy database.
pub(crate) fn read_record(path: &Path) -> Option<ActiveTheme> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != RECORD_HEADER {
        return None;
    }
    let mut profile = None;
    let mut digest = None;
    let mut revision = None;
    for line in lines {
        let (key, value) = line.split_once('=')?;
        match key {
            "profile" => profile = Some(value.to_owned()),
            "digest" => digest = Some(value.to_owned()),
            "revision" => revision = Some(value.parse::<u64>().ok()?),
            _ => return None,
        }
    }
    let theme = ActiveTheme {
        profile_id: profile?,
        package_digest: digest?,
        revision: revision?,
    };
    validate_profile_id(&theme.profile_id).ok()?;
    validate_digest(&theme.package_digest).ok()?;
    Some(theme)
}
