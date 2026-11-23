//! Glint's local state (INS-001, INS-002).
//!
//! - [`GlintPaths`] resolves Glint's configuration, data and cache
//!   directories from the operating system (or an explicit override) and
//!   derives every file location Glint writes: the state database, the
//!   package store, private staging and the installer lock. Nothing in Glint
//!   hardcodes a home directory or user name.
//! - [`Storage`] is the SQLite state database (rusqlite with bundled SQLite,
//!   ADR 0008): versioned [`MIGRATIONS`] applied transactionally, a failed
//!   migration rolled back with the previous schema still readable, and a
//!   corrupt database quarantined and recreated without losing the
//!   [`ActiveTheme`].
//!
//! # Errors
//!
//! Every failure is a [`StorageError`] with a stable [`ErrorCode`] whose
//! string form is `GLINT_STORAGE_*`, and an [`ErrorCategory`] that separates
//! unsupported, permission-denied, unavailable, timeout and other failures.
//!
//! # Example
//!
//! ```no_run
//! use glint_storage::{GlintPaths, Storage};
//!
//! let paths = GlintPaths::resolve()?;
//! let storage = Storage::open(&paths)?;
//! if let Some(recovery) = storage.recovery() {
//!     eprintln!("database recovered: {}", recovery.reason);
//! }
//! println!("active: {:?}", storage.active_theme()?);
//! # Ok::<(), glint_storage::StorageError>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod active;
mod db;
mod error;
mod paths;

pub use active::{ActiveTheme, activate_in, validate_digest, validate_profile_id};
pub use db::{
    DatabaseRecovery, MIGRATIONS, Migration, Storage, latest_schema_version, read_active_theme,
};
pub use error::{ErrorCategory, ErrorCode, StorageError};
pub use paths::{
    BaseDirectories, GlintPaths, HOME_OVERRIDE_ENV, PathSource, PlatformLayout, is_real_dir,
};
/// The SQLite binding, re-exported so table-owning crates (the installer)
/// use exactly this version.
pub use rusqlite;

/// Seconds since the Unix epoch (0 if the clock is before it).
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}
