//! Typed IPC errors with stable `GLINT_IPC_*` codes.

use std::fmt;
use std::io;

use serde::{Deserialize, Serialize};

/// Stable, machine-readable IPC error codes.
///
/// The string form ([`ErrorCode::as_str`], `GLINT_IPC_*`) is the wire
/// contract. Codes are only ever added, never renamed or reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ErrorCode {
    /// No API version both sides support, or a request carried a version
    /// other than the negotiated one.
    #[serde(rename = "GLINT_IPC_VERSION_UNSUPPORTED")]
    VersionUnsupported,
    /// The request `type` is not one this server knows. There is no generic
    /// command or shell request to fall back to.
    #[serde(rename = "GLINT_IPC_UNKNOWN_REQUEST")]
    UnknownRequest,
    /// The frame is not valid JSON, or not a valid envelope or request.
    #[serde(rename = "GLINT_IPC_MALFORMED_FRAME")]
    MalformedFrame,
    /// The frame's declared length is over the limit; nothing was read.
    #[serde(rename = "GLINT_IPC_FRAME_TOO_LARGE")]
    FrameTooLarge,
    /// A request arrived before the `hello` handshake.
    #[serde(rename = "GLINT_IPC_HANDSHAKE_REQUIRED")]
    HandshakeRequired,
    /// A frame that is valid on its own arrived where the protocol does not
    /// allow it (a second `hello`, a mismatched `request_id`, an event on a
    /// request connection).
    #[serde(rename = "GLINT_IPC_PROTOCOL_VIOLATION")]
    ProtocolViolation,
    /// The OS refused access to the endpoint (another user's core, or a
    /// restricted caller).
    #[serde(rename = "GLINT_IPC_PERMISSION_DENIED")]
    PermissionDenied,
    /// No core is reachable: no endpoint record, nothing listening, or the
    /// connection closed.
    #[serde(rename = "GLINT_IPC_UNAVAILABLE")]
    Unavailable,
    /// No answer within the client's deadline. The connection is then
    /// unusable; reconnect.
    #[serde(rename = "GLINT_IPC_TIMEOUT")]
    Timeout,
    /// The server is at its connection limit.
    #[serde(rename = "GLINT_IPC_BUSY")]
    Busy,
    /// The requested data does not exist yet (the core has not finished
    /// booting) or no longer exists.
    #[serde(rename = "GLINT_IPC_NOT_READY")]
    NotReady,
    /// The core is shutting down.
    #[serde(rename = "GLINT_IPC_SHUTTING_DOWN")]
    ShuttingDown,
    /// A subscription delta did not follow the last sequence number; the
    /// client's snapshot is stale and it must resubscribe.
    #[serde(rename = "GLINT_IPC_SEQUENCE_GAP")]
    SequenceGap,
    /// The endpoint could not be created because something already holds
    /// its name (a squatter, or a stale socket that could not be replaced).
    #[serde(rename = "GLINT_IPC_ENDPOINT_IN_USE")]
    EndpointInUse,
    /// The endpoint record is missing fields, malformed or names an
    /// endpoint Glint would never create.
    #[serde(rename = "GLINT_IPC_ENDPOINT_INVALID")]
    EndpointInvalid,
    /// A transport read or write failed.
    #[serde(rename = "GLINT_IPC_IO")]
    Io,
    /// The server could not build a valid response (for example one over
    /// the response size limit).
    #[serde(rename = "GLINT_IPC_INTERNAL")]
    Internal,
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::VersionUnsupported,
        ErrorCode::UnknownRequest,
        ErrorCode::MalformedFrame,
        ErrorCode::FrameTooLarge,
        ErrorCode::HandshakeRequired,
        ErrorCode::ProtocolViolation,
        ErrorCode::PermissionDenied,
        ErrorCode::Unavailable,
        ErrorCode::Timeout,
        ErrorCode::Busy,
        ErrorCode::NotReady,
        ErrorCode::ShuttingDown,
        ErrorCode::SequenceGap,
        ErrorCode::EndpointInUse,
        ErrorCode::EndpointInvalid,
        ErrorCode::Io,
        ErrorCode::Internal,
    ];

    /// The stable string form, e.g. `GLINT_IPC_UNKNOWN_REQUEST`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::VersionUnsupported => "GLINT_IPC_VERSION_UNSUPPORTED",
            ErrorCode::UnknownRequest => "GLINT_IPC_UNKNOWN_REQUEST",
            ErrorCode::MalformedFrame => "GLINT_IPC_MALFORMED_FRAME",
            ErrorCode::FrameTooLarge => "GLINT_IPC_FRAME_TOO_LARGE",
            ErrorCode::HandshakeRequired => "GLINT_IPC_HANDSHAKE_REQUIRED",
            ErrorCode::ProtocolViolation => "GLINT_IPC_PROTOCOL_VIOLATION",
            ErrorCode::PermissionDenied => "GLINT_IPC_PERMISSION_DENIED",
            ErrorCode::Unavailable => "GLINT_IPC_UNAVAILABLE",
            ErrorCode::Timeout => "GLINT_IPC_TIMEOUT",
            ErrorCode::Busy => "GLINT_IPC_BUSY",
            ErrorCode::NotReady => "GLINT_IPC_NOT_READY",
            ErrorCode::ShuttingDown => "GLINT_IPC_SHUTTING_DOWN",
            ErrorCode::SequenceGap => "GLINT_IPC_SEQUENCE_GAP",
            ErrorCode::EndpointInUse => "GLINT_IPC_ENDPOINT_IN_USE",
            ErrorCode::EndpointInvalid => "GLINT_IPC_ENDPOINT_INVALID",
            ErrorCode::Io => "GLINT_IPC_IO",
            ErrorCode::Internal => "GLINT_IPC_INTERNAL",
        }
    }

    /// The category this code always carries.
    pub const fn category(self) -> ErrorCategory {
        match self {
            ErrorCode::VersionUnsupported | ErrorCode::UnknownRequest => ErrorCategory::Unsupported,
            ErrorCode::PermissionDenied => ErrorCategory::PermissionDenied,
            ErrorCode::Unavailable
            | ErrorCode::Busy
            | ErrorCode::NotReady
            | ErrorCode::ShuttingDown
            | ErrorCode::EndpointInUse => ErrorCategory::Unavailable,
            ErrorCode::Timeout => ErrorCategory::Timeout,
            ErrorCode::MalformedFrame
            | ErrorCode::FrameTooLarge
            | ErrorCode::HandshakeRequired
            | ErrorCode::ProtocolViolation
            | ErrorCode::SequenceGap
            | ErrorCode::EndpointInvalid
            | ErrorCode::Io
            | ErrorCode::Internal => ErrorCategory::Failure,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The broad class of an error. Unsupported, permission-denied,
/// unavailable, timeout and failure are always distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ErrorCategory {
    /// The request or version is not supported.
    Unsupported,
    /// The caller is not allowed.
    PermissionDenied,
    /// The core, or the data asked for, is not available now.
    Unavailable,
    /// A deadline passed.
    Timeout,
    /// Anything else.
    Failure,
}

impl ErrorCategory {
    /// A stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCategory::Unsupported => "unsupported",
            ErrorCategory::PermissionDenied => "permission-denied",
            ErrorCategory::Unavailable => "unavailable",
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::Failure => "failure",
        }
    }
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An IPC failure as it travels on the wire and as the client library
/// returns it: a stable code, its category and a message (not stable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct IpcError {
    /// The stable code.
    pub code: ErrorCode,
    /// The category (always [`ErrorCode::category`] for errors this crate
    /// creates).
    pub category: ErrorCategory,
    /// Human-readable detail (not stable).
    pub message: String,
}

impl IpcError {
    /// An error with `code`'s own category.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            category: code.category(),
            message: message.into(),
        }
    }

    /// Map a transport error: refused access, a missing endpoint and a
    /// closed connection keep their own codes.
    pub(crate) fn from_io(context: &str, error: &io::Error) -> Self {
        let code = match error.kind() {
            io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            io::ErrorKind::NotFound
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof => ErrorCode::Unavailable,
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ErrorCode::Timeout,
            io::ErrorKind::AddrInUse => ErrorCode::EndpointInUse,
            _ => ErrorCode::Io,
        };
        Self::new(code, format!("{context}: {error}"))
    }

    /// The stable code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The category.
    pub fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): {}", self.code, self.category, self.message)
    }
}

impl std::error::Error for IpcError {}
