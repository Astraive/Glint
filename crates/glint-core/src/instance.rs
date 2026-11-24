//! Per-user single instance (COR-002).
//!
//! The core process takes an exclusive lock on
//! [`GlintPaths::instance_lock_file`] before it opens the state database,
//! with the same mechanism as the installer lock (`File::try_lock`: a
//! `LockFileEx` lock on Windows, `flock` elsewhere). The OS releases the
//! lock when the process exits for any reason, so a crash never leaves a
//! stale lock behind. While holding it the process writes an owner record
//! ([`GlintPaths::instance_owner_file`], a separate file because Windows
//! blocks reads of a locked range) so a second invocation can name the
//! running instance instead of touching the database.
//!
//! The lock lives under the data root, which is per user (and per
//! `GLINT_HOME`), so two users, or two portable roots, never block each
//! other.

use std::fmt;
use std::fs::{self, File, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use glint_storage::GlintPaths;

const OWNER_HEADER: &str = "glint-instance 1";
const MAX_OWNER_BYTES: u64 = 4096;

/// The process holding the instance lock, as recorded in the owner file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct InstanceOwner {
    /// OS process ID.
    pub pid: u32,
    /// Unix time the instance took the lock.
    pub started_at: i64,
}

impl InstanceOwner {
    fn current() -> Self {
        Self {
            pid: std::process::id(),
            started_at: glint_storage::unix_now(),
        }
    }

    fn encode(&self) -> String {
        format!(
            "{OWNER_HEADER}\npid={}\nstarted_at={}\n",
            self.pid, self.started_at
        )
    }

    fn decode(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != OWNER_HEADER {
            return None;
        }
        let mut pid = None;
        let mut started_at = None;
        for line in lines {
            match line.split_once('=') {
                Some(("pid", value)) => pid = value.parse().ok(),
                Some(("started_at", value)) => started_at = value.parse().ok(),
                _ => {}
            }
        }
        Some(Self {
            pid: pid?,
            started_at: started_at?,
        })
    }
}

impl fmt::Display for InstanceOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pid {} (since {})", self.pid, self.started_at)
    }
}

/// Why the instance lock could not be taken.
#[derive(Debug)]
#[non_exhaustive]
pub enum InstanceError {
    /// Another process holds the lock. `owner` is its record, or `None`
    /// when the record is missing or unreadable (the owner may be starting).
    AlreadyRunning {
        /// The lock file.
        lock: PathBuf,
        /// The running instance.
        owner: Option<InstanceOwner>,
    },
    /// The lock or owner file could not be opened, locked or written.
    Io {
        /// The step that failed.
        context: &'static str,
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
}

impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstanceError::AlreadyRunning {
                owner: Some(owner), ..
            } => {
                write!(f, "Glint is already running for this user ({owner})")
            }
            InstanceError::AlreadyRunning { owner: None, .. } => {
                f.write_str("Glint is already running for this user (owner not recorded yet)")
            }
            InstanceError::Io {
                context,
                path,
                source,
            } => write!(f, "{context} {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for InstanceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            InstanceError::Io { source, .. } => Some(source),
            InstanceError::AlreadyRunning { .. } => None,
        }
    }
}

fn io_error(context: &'static str, path: &Path) -> impl FnOnce(io::Error) -> InstanceError {
    let path = path.to_path_buf();
    move |source| InstanceError::Io {
        context,
        path,
        source,
    }
}

/// The held per-user instance lock. Dropping it (or [`release`](Self::release))
/// removes the owner record and then releases the lock.
#[derive(Debug)]
pub struct InstanceLock {
    file: Option<File>,
    lock_path: PathBuf,
    owner_path: PathBuf,
    owner: InstanceOwner,
}

impl InstanceLock {
    /// Take the lock for `paths` and record this process as the owner. The
    /// data directory must exist ([`GlintPaths::ensure_dirs`]).
    ///
    /// # Errors
    ///
    /// [`InstanceError::AlreadyRunning`] with the running instance's record,
    /// or [`InstanceError::Io`].
    pub fn acquire(paths: &GlintPaths) -> Result<Self, InstanceError> {
        let lock_path = paths.instance_lock_file();
        let owner_path = paths.instance_owner_file();
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(io_error("open instance lock", &lock_path))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(InstanceError::AlreadyRunning {
                    lock: lock_path,
                    owner: read_owner(&owner_path),
                });
            }
            Err(TryLockError::Error(source)) => {
                return Err(InstanceError::Io {
                    context: "take instance lock",
                    path: lock_path,
                    source,
                });
            }
        }
        // A record left by a crashed owner is simply replaced.
        let owner = InstanceOwner::current();
        write_owner(&owner_path, &owner)?;
        Ok(Self {
            file: Some(file),
            lock_path,
            owner_path,
            owner,
        })
    }

    /// This process's owner record.
    pub fn owner(&self) -> &InstanceOwner {
        &self.owner
    }

    /// The lock file.
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    /// Remove the owner record, then release the lock.
    ///
    /// # Errors
    ///
    /// [`InstanceError::Io`] when the owner record could not be removed; the
    /// lock is released anyway.
    pub fn release(mut self) -> Result<(), InstanceError> {
        self.release_inner()
    }

    fn release_inner(&mut self) -> Result<(), InstanceError> {
        let Some(file) = self.file.take() else {
            return Ok(());
        };
        let removed = match fs::remove_file(&self.owner_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(InstanceError::Io {
                context: "remove instance owner record",
                path: self.owner_path.clone(),
                source,
            }),
        };
        // Unlocking explicitly before closing; closing alone also releases.
        let _ = file.unlock();
        drop(file);
        removed
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        let _ = self.release_inner();
    }
}

/// Whether an instance holds the lock, as seen by a probe.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstanceProbe {
    /// No instance runs; the lock is free.
    Free,
    /// An instance holds the lock; its record, if written yet.
    Running(Option<InstanceOwner>),
}

/// Probe for a running instance without taking the lock or touching the
/// database: what a client (the CLI, a second launch) checks before it
/// would connect. Until the IPC transport exists (COR-003) a client can
/// only report the running instance.
///
/// # Errors
///
/// [`InstanceError::Io`] when the lock file cannot be opened.
pub fn probe_instance(paths: &GlintPaths) -> Result<InstanceProbe, InstanceError> {
    let lock_path = paths.instance_lock_file();
    let file = match File::options().read(true).write(true).open(&lock_path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(InstanceProbe::Free),
        Err(source) => {
            return Err(InstanceError::Io {
                context: "open instance lock",
                path: lock_path,
                source,
            });
        }
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            Ok(InstanceProbe::Free)
        }
        Err(TryLockError::WouldBlock) => Ok(InstanceProbe::Running(read_owner(
            &paths.instance_owner_file(),
        ))),
        Err(TryLockError::Error(source)) => Err(InstanceError::Io {
            context: "probe instance lock",
            path: lock_path,
            source,
        }),
    }
}

fn read_owner(path: &Path) -> Option<InstanceOwner> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(MAX_OWNER_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    InstanceOwner::decode(&text)
}

fn write_owner(path: &Path, owner: &InstanceOwner) -> Result<(), InstanceError> {
    let tmp = path.with_extension("owner.tmp");
    let mut file = File::create(&tmp).map_err(io_error("write instance owner record", &tmp))?;
    file.write_all(owner.encode().as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(io_error("write instance owner record", &tmp))?;
    drop(file);
    fs::rename(&tmp, path).map_err(io_error("write instance owner record", path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_records_round_trip_and_reject_garbage() {
        let owner = InstanceOwner {
            pid: 4242,
            started_at: 1_790_000_000,
        };
        assert_eq!(InstanceOwner::decode(&owner.encode()), Some(owner));
        assert_eq!(InstanceOwner::decode("glint-instance 2\npid=1\n"), None);
        assert_eq!(
            InstanceOwner::decode("glint-instance 1\npid=x\nstarted_at=1\n"),
            None
        );
        assert_eq!(InstanceOwner::decode(""), None);
    }
}
