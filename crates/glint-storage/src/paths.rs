//! OS-resolved configuration, data and cache locations (INS-001).
//!
//! Base directories come from the [`dirs`] crate, which follows the XDG base
//! directory specification on Linux (honouring `$XDG_CONFIG_HOME`,
//! `$XDG_DATA_HOME` and `$XDG_CACHE_HOME`, and ignoring relative values as
//! the specification requires), asks Windows through
//! `SHGetKnownFolderPath` and uses the Standard Directories on macOS. No user
//! name, home directory or drive letter is ever written into Glint's code.
//!
//! Glint then adds its own application sub-paths, matching "Suggested
//! configuration locations" in `docs/plan/02-architecture/renderer-and-state.md`:
//!
//! | Layout | Config | Data | Cache |
//! |---|---|---|---|
//! | Windows | `{RoamingAppData}\Glint` | `{LocalAppData}\Glint\data` | `{LocalAppData}\Glint\cache` |
//! | macOS | `~/Library/Application Support/Glint` | `~/Library/Application Support/Glint/data` | `~/Library/Caches/Glint` |
//! | Linux (XDG) | `$XDG_CONFIG_HOME/glint` | `$XDG_DATA_HOME/glint` | `$XDG_CACHE_HOME/glint` |
//!
//! Package data stays out of the Windows roaming profile: only configuration
//! roams.
//!
//! # Overrides
//!
//! Setting [`HOME_OVERRIDE_ENV`] (`GLINT_HOME`) to an absolute directory puts
//! all three under it (`config/`, `data/`, `cache/`); tests and portable runs
//! use it, or [`GlintPaths::under_root`] directly. A relative override is an
//! error, never silently resolved against the current directory.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::error::StorageError;

/// Environment variable that overrides every Glint root with one directory.
pub const HOME_OVERRIDE_ENV: &str = "GLINT_HOME";

/// Application directory name on Windows and macOS.
const APP_DIR: &str = "Glint";
/// Application directory name under the XDG roots (lower case by convention).
const XDG_APP_DIR: &str = "glint";

/// Which platform convention to lay Glint's directories out by.
///
/// [`PlatformLayout::current`] is the host's; every layout can be computed on
/// any host from synthetic [`BaseDirectories`], which is how the per-OS
/// expectations are tested on the Windows-only test host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformLayout {
    /// `%APPDATA%` for configuration, `%LOCALAPPDATA%` for data and cache.
    Windows,
    /// `~/Library/Application Support` and `~/Library/Caches`.
    MacOs,
    /// XDG base directories (Linux and other Unix hosts).
    Xdg,
}

impl PlatformLayout {
    /// The layout of the host Glint was compiled for.
    pub const fn current() -> Self {
        if cfg!(windows) {
            PlatformLayout::Windows
        } else if cfg!(target_os = "macos") {
            PlatformLayout::MacOs
        } else {
            PlatformLayout::Xdg
        }
    }
}

/// The OS base directories Glint builds on, before Glint's own sub-paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseDirectories {
    /// User configuration root (`{RoamingAppData}`, `~/Library/Application
    /// Support`, `$XDG_CONFIG_HOME`).
    pub config: PathBuf,
    /// Local (non-roaming) data root (`{LocalAppData}`, `~/Library/Application
    /// Support`, `$XDG_DATA_HOME`).
    pub data_local: PathBuf,
    /// Cache root (`{LocalAppData}`, `~/Library/Caches`, `$XDG_CACHE_HOME`).
    pub cache: PathBuf,
}

impl BaseDirectories {
    /// Ask the operating system.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::PathUnavailable`](crate::ErrorCode::PathUnavailable) when
    /// the OS reports no such directory (for example no home directory).
    pub fn from_os() -> Result<Self, StorageError> {
        Ok(Self {
            config: dirs::config_dir().ok_or(StorageError::PathUnavailable { which: "config" })?,
            data_local: dirs::data_local_dir()
                .ok_or(StorageError::PathUnavailable { which: "data" })?,
            cache: dirs::cache_dir().ok_or(StorageError::PathUnavailable { which: "cache" })?,
        })
    }
}

/// Where the resolved paths came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathSource {
    /// The operating system's base directories.
    Os,
    /// The [`HOME_OVERRIDE_ENV`] environment variable.
    EnvOverride,
    /// An explicit root passed to [`GlintPaths::under_root`].
    ExplicitRoot,
}

/// Glint's resolved configuration, data and cache directories.
///
/// Every file Glint writes lives under one of these three roots;
/// [`GlintPaths::check_within`] enforces it for derived paths. Resolving
/// paths never touches the filesystem; [`GlintPaths::ensure_dirs`] creates
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlintPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    source: PathSource,
}

impl GlintPaths {
    /// Resolve from [`HOME_OVERRIDE_ENV`] if set, otherwise from the OS.
    ///
    /// # Errors
    ///
    /// See [`GlintPaths::resolve_with_env`].
    pub fn resolve() -> Result<Self, StorageError> {
        Self::resolve_with_env(|name| std::env::var_os(name))
    }

    /// Resolve with an injected environment lookup (tests never mutate the
    /// process environment).
    ///
    /// An empty override counts as unset.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::PathNotAbsolute`](crate::ErrorCode::PathNotAbsolute) for a
    /// relative override, or the errors of [`GlintPaths::from_os`].
    pub fn resolve_with_env(
        lookup: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Self, StorageError> {
        match lookup(HOME_OVERRIDE_ENV) {
            Some(value) if !value.is_empty() => {
                let root = PathBuf::from(value);
                require_absolute("GLINT_HOME override", &root)?;
                Ok(Self::rooted(root, PathSource::EnvOverride))
            }
            _ => Self::from_os(),
        }
    }

    /// Resolve from the operating system's base directories with the host's
    /// [`PlatformLayout`].
    ///
    /// # Errors
    ///
    /// [`ErrorCode::PathUnavailable`](crate::ErrorCode::PathUnavailable) or
    /// [`ErrorCode::PathNotAbsolute`](crate::ErrorCode::PathNotAbsolute).
    pub fn from_os() -> Result<Self, StorageError> {
        Self::from_base(PlatformLayout::current(), &BaseDirectories::from_os()?)
    }

    /// Lay Glint's directories out over explicit base directories.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::PathNotAbsolute`](crate::ErrorCode::PathNotAbsolute) when a
    /// base directory is relative.
    pub fn from_base(layout: PlatformLayout, base: &BaseDirectories) -> Result<Self, StorageError> {
        require_absolute("config base directory", &base.config)?;
        require_absolute("data base directory", &base.data_local)?;
        require_absolute("cache base directory", &base.cache)?;
        let (config_dir, data_dir, cache_dir) = match layout {
            PlatformLayout::Windows => (
                base.config.join(APP_DIR),
                base.data_local.join(APP_DIR).join("data"),
                base.cache.join(APP_DIR).join("cache"),
            ),
            PlatformLayout::MacOs => (
                base.config.join(APP_DIR),
                base.data_local.join(APP_DIR).join("data"),
                base.cache.join(APP_DIR),
            ),
            PlatformLayout::Xdg => (
                base.config.join(XDG_APP_DIR),
                base.data_local.join(XDG_APP_DIR),
                base.cache.join(XDG_APP_DIR),
            ),
        };
        Ok(Self {
            config_dir,
            data_dir,
            cache_dir,
            source: PathSource::Os,
        })
    }

    /// Put every root under one absolute directory: `config/`, `data/` and
    /// `cache/`.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::PathNotAbsolute`](crate::ErrorCode::PathNotAbsolute) for a
    /// relative root.
    pub fn under_root(root: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = root.into();
        require_absolute("explicit root", &root)?;
        Ok(Self::rooted(root, PathSource::ExplicitRoot))
    }

    fn rooted(root: PathBuf, source: PathSource) -> Self {
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
            source,
        }
    }

    /// User configuration directory.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Local data directory (database, package store, staging, journal).
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Evictable cache directory.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Where these paths came from.
    pub fn source(&self) -> PathSource {
        self.source
    }

    /// `config.toml` in the configuration directory.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// The SQLite state database.
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("glint.sqlite3")
    }

    /// The last-known active theme record kept beside the database, so a
    /// corrupt database never loses the active theme.
    pub fn active_record_file(&self) -> PathBuf {
        self.data_dir.join("active-theme.record")
    }

    /// The immutable, content-addressed package store.
    pub fn store_dir(&self) -> PathBuf {
        self.data_dir.join("store")
    }

    /// Private staging for packages being inspected or installed. On the same
    /// volume as [`store_dir`](Self::store_dir), so moving a staged package
    /// into the store is a single rename.
    pub fn staging_dir(&self) -> PathBuf {
        self.data_dir.join("staging")
    }

    /// Where removed packages wait for deletion (same volume as the store).
    pub fn trash_dir(&self) -> PathBuf {
        self.data_dir.join("trash")
    }

    /// The exclusive installer lock file.
    pub fn installer_lock_file(&self) -> PathBuf {
        self.data_dir.join("installer.lock")
    }

    /// The per-user single-instance lock file (COR-002). Held for the core
    /// process's lifetime; the OS releases it when the process exits.
    pub fn instance_lock_file(&self) -> PathBuf {
        self.data_dir.join("instance.lock")
    }

    /// The record naming the process that holds the instance lock, kept
    /// beside the lock so a second instance can read it.
    pub fn instance_owner_file(&self) -> PathBuf {
        self.data_dir.join("instance.owner")
    }

    /// The three roots.
    pub fn roots(&self) -> [&Path; 3] {
        [&self.config_dir, &self.data_dir, &self.cache_dir]
    }

    /// True when `path` is absolute, has no `.` or `..` components and lies
    /// under one of the three roots. The check is lexical; it never touches
    /// the filesystem.
    pub fn contains(&self, path: &Path) -> bool {
        path.is_absolute()
            && !path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
            && self.roots().iter().any(|root| path.starts_with(root))
    }

    /// Refuse a path outside the roots.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::PathOutsideRoots`](crate::ErrorCode::PathOutsideRoots).
    pub fn check_within(&self, path: &Path) -> Result<(), StorageError> {
        if self.contains(path) {
            Ok(())
        } else {
            Err(StorageError::PathOutsideRoots(path.to_path_buf()))
        }
    }

    /// Create the three roots and the data sub-directories (store, staging,
    /// trash) if missing, then check that each Glint-owned directory is a real
    /// directory and not a file, symlink or junction.
    ///
    /// On Unix new directories get mode `0700`. On Windows they inherit the
    /// per-user ACL of `%LOCALAPPDATA%` / `%APPDATA%`.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Io`](crate::ErrorCode::Io) or
    /// [`ErrorCode::PathNotDirectory`](crate::ErrorCode::PathNotDirectory).
    pub fn ensure_dirs(&self) -> Result<(), StorageError> {
        let owned = [
            self.config_dir.clone(),
            self.data_dir.clone(),
            self.cache_dir.clone(),
            self.store_dir(),
            self.staging_dir(),
            self.trash_dir(),
        ];
        for dir in &owned {
            create_private_dir_all(dir).map_err(StorageError::io("create directory", dir))?;
            if !is_real_dir(dir) {
                return Err(StorageError::PathNotDirectory(dir.clone()));
            }
        }
        Ok(())
    }
}

fn require_absolute(what: &'static str, path: &Path) -> Result<(), StorageError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(StorageError::PathNotAbsolute {
            what,
            path: path.to_path_buf(),
        })
    }
}

fn create_private_dir_all(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)
    }
}

/// True when `path` is a directory and not a symlink, junction or other
/// reparse point.
pub fn is_real_dir(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) => meta.is_dir() && !meta.file_type().is_symlink() && !is_reparse_point(&meta),
        Err(_) => false,
    }
}

#[cfg(windows)]
fn is_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_meta: &fs::Metadata) -> bool {
    false
}
