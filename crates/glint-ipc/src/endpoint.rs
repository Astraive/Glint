//! Where the core listens and how a client finds it (ADR 0010).
//!
//! - **Windows:** a named pipe `\\.\pipe\glint-ipc-<16 hex>`. The suffix is
//!   random per server start, so another user cannot predict and squat the
//!   name before the core starts (threat model B7.2). The pipe is created
//!   with `FILE_FLAG_FIRST_PIPE_INSTANCE` and `PIPE_REJECT_REMOTE_CLIENTS`
//!   (both set by `interprocess`) and a protected DACL that grants access
//!   only to the pipe's owner, the user running the core ([`PIPE_SDDL`]).
//! - **Unix:** a socket at `<data>/ipc.sock` inside the `0700` data root,
//!   created with mode `0600`.
//!
//! The server writes the endpoint into `<data>/ipc.endpoint` (write and
//! rename) after it binds and removes the record when it stops. Clients
//! read the record; it lives in the per-user data root, so only that user
//! can read or replace it.

use std::fmt;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use glint_storage::GlintPaths;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ListenerNonblockingMode, ListenerOptions, Name};

use crate::error::{ErrorCode, IpcError};
use crate::protocol::SUPPORTED_VERSIONS;

const RECORD_HEADER: &str = "glint-ipc-endpoint 1";
const MAX_RECORD_BYTES: u64 = 4096;
const PIPE_PREFIX: &str = "glint-ipc-";

/// The named pipe's security descriptor (SDDL): a protected DACL with one
/// entry granting generic all to `OW`, the owner rights of the pipe, which
/// is the user that created it. Everyone else, including other users,
/// anonymous logons and AppContainer processes, gets no access. The
/// default (unlabelled, medium) integrity level is kept, so lower-integrity
/// processes cannot write to it either.
pub const PIPE_SDDL: &str = "D:P(A;;GA;;;OW)";

/// The kind of local transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Transport {
    /// A Windows named pipe.
    NamedPipe,
    /// A Unix domain socket.
    UnixSocket,
}

impl Transport {
    /// This platform's transport.
    pub const fn current() -> Self {
        if cfg!(windows) {
            Transport::NamedPipe
        } else {
            Transport::UnixSocket
        }
    }

    /// A stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Transport::NamedPipe => "named-pipe",
            Transport::UnixSocket => "unix-socket",
        }
    }
}

/// A local IPC endpoint: a pipe name on Windows, a socket path elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    transport: Transport,
    name: String,
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.transport {
            Transport::NamedPipe => write!(f, r"\\.\pipe\{}", self.name),
            Transport::UnixSocket => f.write_str(&self.name),
        }
    }
}

fn random_suffix(paths: &GlintPaths) -> String {
    // `RandomState` is keyed from the OS random source once per process
    // and differs per instance; mixing in the pid, the clock and the data
    // root keeps two servers from ever colliding. The suffix only has to
    // be unpredictable to another user, not cryptographically strong:
    // squatting a guessed name merely makes the core refuse to start.
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_i64(glint_storage::unix_now());
    hasher.write(paths.data_dir().as_os_str().as_encoded_bytes());
    if let Ok(elapsed) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(elapsed.as_nanos());
    }
    format!("{:016x}", hasher.finish())
}

fn valid_pipe_name(name: &str) -> bool {
    name.strip_prefix(PIPE_PREFIX).is_some_and(|hex| {
        hex.len() == 16
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn invalid(message: impl Into<String>) -> IpcError {
    IpcError::new(ErrorCode::EndpointInvalid, message)
}

impl Endpoint {
    /// A fresh endpoint for a server over `paths`: a new random pipe name
    /// on Windows, `<data>/ipc.sock` elsewhere.
    pub fn generate(paths: &GlintPaths) -> Self {
        match Transport::current() {
            Transport::NamedPipe => Self {
                transport: Transport::NamedPipe,
                name: format!("{PIPE_PREFIX}{}", random_suffix(paths)),
            },
            Transport::UnixSocket => Self {
                transport: Transport::UnixSocket,
                name: paths.ipc_socket_file().to_string_lossy().into_owned(),
            },
        }
    }

    /// The running core's endpoint, from `<data>/ipc.endpoint`.
    ///
    /// # Errors
    ///
    /// `GLINT_IPC_UNAVAILABLE` when there is no record (no core running),
    /// `GLINT_IPC_PERMISSION_DENIED` when it cannot be read,
    /// `GLINT_IPC_ENDPOINT_INVALID` when it is malformed or names an
    /// endpoint Glint would not create.
    pub fn read(paths: &GlintPaths) -> Result<Self, IpcError> {
        let path = paths.ipc_endpoint_file();
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(IpcError::new(
                    ErrorCode::Unavailable,
                    format!(
                        "no Glint core is serving IPC ({} is absent)",
                        path.display()
                    ),
                ));
            }
            Err(error) => return Err(IpcError::from_io("read endpoint record", &error)),
        };
        let mut text = String::new();
        file.take(MAX_RECORD_BYTES)
            .read_to_string(&mut text)
            .map_err(|error| IpcError::from_io("read endpoint record", &error))?;
        Self::decode(&text, paths)
    }

    fn decode(text: &str, paths: &GlintPaths) -> Result<Self, IpcError> {
        let mut lines = text.lines();
        if lines.next() != Some(RECORD_HEADER) {
            return Err(invalid("endpoint record has an unknown header"));
        }
        let mut transport = None;
        let mut name = None;
        for line in lines {
            match line.split_once('=') {
                Some(("transport", value)) => transport = Some(value.to_owned()),
                Some(("name", value)) => name = Some(value.to_owned()),
                _ => {}
            }
        }
        let transport = transport.ok_or_else(|| invalid("endpoint record has no transport"))?;
        let name = name.ok_or_else(|| invalid("endpoint record has no name"))?;
        let expected = Transport::current();
        if transport != expected.as_str() {
            return Err(invalid(format!(
                "endpoint record names transport {transport:?}, this platform uses {}",
                expected.as_str()
            )));
        }
        // Only connect where Glint itself would listen: a tampered record
        // must not point the client at an arbitrary pipe or socket.
        let acceptable = match expected {
            Transport::NamedPipe => valid_pipe_name(&name),
            Transport::UnixSocket => Path::new(&name) == paths.ipc_socket_file(),
        };
        if !acceptable {
            return Err(invalid(format!(
                "endpoint record names {name:?}, not a Glint endpoint"
            )));
        }
        Ok(Self {
            transport: expected,
            name,
        })
    }

    /// The transport.
    pub fn transport(&self) -> Transport {
        self.transport
    }

    /// The pipe name (without `\\.\pipe\`) or socket path.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn os_name(&self) -> io::Result<Name<'_>> {
        match self.transport {
            Transport::NamedPipe => self
                .name
                .as_str()
                .to_ns_name::<interprocess::local_socket::GenericNamespaced>(),
            Transport::UnixSocket => self
                .name
                .as_str()
                .to_fs_name::<interprocess::local_socket::GenericFilePath>(),
        }
    }

    /// Bind a listener restricted to the current user (see the module
    /// docs). Accept is non-blocking so the server can poll for stop.
    pub(crate) fn listen(&self) -> Result<LocalSocketListener, IpcError> {
        let name = self
            .os_name()
            .map_err(|error| IpcError::from_io("endpoint name", &error))?;
        let options = ListenerOptions::new()
            .name(name)
            .nonblocking(ListenerNonblockingMode::Accept);
        let options = restrict_to_current_user(options)?;
        let listener = options.create_sync().map_err(|error| {
            let mut ipc = IpcError::from_io("create endpoint", &error);
            // The first-instance flag (Windows) turns a squatted name into
            // access denied or "in use"; report both as in use.
            if matches!(
                error.kind(),
                io::ErrorKind::AddrInUse | io::ErrorKind::PermissionDenied
            ) {
                ipc = IpcError::new(
                    ErrorCode::EndpointInUse,
                    format!("{self} is already in use: {error}"),
                );
            }
            ipc
        })?;
        if self.transport == Transport::UnixSocket {
            restrict_socket_file(&self.name)?;
        }
        Ok(listener)
    }

    /// Connect to the endpoint.
    pub(crate) fn connect(&self) -> Result<LocalSocketStream, IpcError> {
        let name = self
            .os_name()
            .map_err(|error| IpcError::from_io("endpoint name", &error))?;
        LocalSocketStream::connect(name)
            .map_err(|error| IpcError::from_io(&format!("connect to {self}"), &error))
    }

    /// Write the endpoint record for clients (write to a temporary file,
    /// then rename over the record).
    pub(crate) fn write_record(&self, paths: &GlintPaths) -> Result<PathBuf, IpcError> {
        let path = paths.ipc_endpoint_file();
        let tmp = path.with_extension("endpoint.tmp");
        let text = format!(
            "{RECORD_HEADER}\ntransport={}\nname={}\npid={}\napi_versions={}-{}\n",
            self.transport.as_str(),
            self.name,
            std::process::id(),
            SUPPORTED_VERSIONS.min,
            SUPPORTED_VERSIONS.max,
        );
        let write = || -> io::Result<()> {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::rename(&tmp, &path)
        };
        write().map_err(|error| {
            let _ = fs::remove_file(&tmp);
            IpcError::from_io("write endpoint record", &error)
        })?;
        Ok(path)
    }
}

#[cfg(windows)]
fn restrict_to_current_user(options: ListenerOptions<'_>) -> Result<ListenerOptions<'_>, IpcError> {
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;

    let sddl = widestring::U16CString::from_str(PIPE_SDDL)
        .map_err(|error| IpcError::new(ErrorCode::Internal, format!("pipe SDDL: {error}")))?;
    let descriptor = SecurityDescriptor::deserialize(&sddl)
        .map_err(|error| IpcError::from_io("pipe security descriptor", &error))?;
    Ok(options.security_descriptor(descriptor))
}

#[cfg(target_os = "linux")]
fn restrict_to_current_user(options: ListenerOptions<'_>) -> Result<ListenerOptions<'_>, IpcError> {
    use interprocess::os::unix::local_socket::ListenerOptionsExt;

    // The data root is already 0700; the socket itself is 0600. The core
    // holds the single-instance lock, so a socket file left at the path is
    // a crashed core's and is replaced.
    Ok(options.mode(0o600).try_overwrite(true))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn restrict_to_current_user(options: ListenerOptions<'_>) -> Result<ListenerOptions<'_>, IpcError> {
    // macOS and the BSDs reject fchmod on a socket, so the listener cannot
    // set the mode at bind time; `restrict_socket_file` chmods the socket by
    // path right after binding. The 0700 data root keeps other users out of
    // that window. A stale socket from a crashed core is replaced, as above.
    Ok(options.try_overwrite(true))
}

/// After binding, make the socket file owner-only on Unix systems where the
/// listener could not set the mode itself.
#[cfg(all(unix, not(target_os = "linux")))]
fn restrict_socket_file(path: &str) -> Result<(), IpcError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| IpcError::from_io("restrict endpoint socket", &error))
}

#[cfg(not(all(unix, not(target_os = "linux"))))]
fn restrict_socket_file(_: &str) -> Result<(), IpcError> {
    Ok(())
}

#[cfg(not(any(windows, unix)))]
fn restrict_to_current_user(_: ListenerOptions<'_>) -> Result<ListenerOptions<'_>, IpcError> {
    Err(IpcError::new(
        ErrorCode::Unavailable,
        "local IPC is not supported on this platform",
    ))
}

/// Remove the endpoint record if it still names `endpoint` (a newer server
/// may have replaced it).
pub(crate) fn remove_record(paths: &GlintPaths, endpoint: &Endpoint) {
    if let Ok(current) = Endpoint::read(paths)
        && current != *endpoint
    {
        return;
    }
    let _ = fs::remove_file(paths.ipc_endpoint_file());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_pipe_names_are_valid_and_differ() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = GlintPaths::under_root(tmp.path()).unwrap();
        let a = Endpoint::generate(&paths);
        let b = Endpoint::generate(&paths);
        if cfg!(windows) {
            assert!(valid_pipe_name(a.name()), "{a}");
            assert_ne!(a, b);
        } else {
            assert_eq!(Path::new(a.name()), paths.ipc_socket_file());
        }
    }

    #[test]
    fn records_naming_foreign_endpoints_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = GlintPaths::under_root(tmp.path()).unwrap();
        let transport = Transport::current().as_str();
        for name in [
            "evil",
            "glint-ipc-XYZ",
            "glint-ipc-0123456789abcdef0",
            "/tmp/other.sock",
        ] {
            let text = format!("{RECORD_HEADER}\ntransport={transport}\nname={name}\n");
            let error = Endpoint::decode(&text, &paths).unwrap_err();
            assert_eq!(error.code(), ErrorCode::EndpointInvalid, "{name}");
        }
        let error = Endpoint::decode("something else\n", &paths).unwrap_err();
        assert_eq!(error.code(), ErrorCode::EndpointInvalid);
    }

    #[test]
    fn a_generated_record_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = GlintPaths::under_root(tmp.path()).unwrap();
        paths.ensure_dirs().unwrap();
        let endpoint = Endpoint::generate(&paths);
        endpoint.write_record(&paths).unwrap();
        assert_eq!(Endpoint::read(&paths).unwrap(), endpoint);
        remove_record(&paths, &endpoint);
        assert_eq!(
            Endpoint::read(&paths).unwrap_err().code(),
            ErrorCode::Unavailable
        );
    }
}
