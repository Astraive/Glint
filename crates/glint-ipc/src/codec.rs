//! Framing and decoding with size limits and typed failures.
//!
//! A frame is a 4-byte big-endian length followed by that many bytes of
//! UTF-8 JSON. The length is checked against the limit before anything is
//! allocated or read, so an oversized frame costs four bytes. A zero
//! length is malformed.
//!
//! [`decode_client_frame`] tells an unknown request `type`
//! (`GLINT_IPC_UNKNOWN_REQUEST`) apart from a broken frame
//! (`GLINT_IPC_MALFORMED_FRAME`) and keeps the `request_id` whenever it can
//! be read, so the error response can name the request.

use std::io::{self, Read, Write};

use serde::Serialize;
use serde_json::Value;

use crate::error::{ErrorCode, IpcError};
use crate::protocol::ServerFrame;
use crate::protocol::{ClientFrame, GlintRequest, Hello, MAX_REQUEST_ID_LEN, RequestEnvelope};

/// The largest request frame a server reads (64 KiB). Requests are small
/// typed values; nothing legitimate comes close.
pub const MAX_REQUEST_BYTES: u32 = 64 * 1024;

/// The largest frame a server sends and a client reads (1 MiB).
pub const MAX_RESPONSE_BYTES: u32 = 1024 * 1024;

/// Why a frame could not be read.
#[derive(Debug)]
#[non_exhaustive]
pub enum FrameError {
    /// The declared length is over the limit. Nothing after the length was
    /// read, so the stream is no longer in sync and must be closed.
    TooLarge {
        /// The declared length.
        declared: u32,
        /// The limit.
        limit: u32,
    },
    /// The declared length is zero.
    Empty,
    /// The stream ended inside a frame.
    Truncated,
    /// A transport error.
    Io(io::Error),
}

impl FrameError {
    /// The typed error to report.
    pub fn to_ipc(&self) -> IpcError {
        match self {
            FrameError::TooLarge { declared, limit } => IpcError::new(
                ErrorCode::FrameTooLarge,
                format!("frame of {declared} bytes is over the {limit}-byte limit"),
            ),
            FrameError::Empty => IpcError::new(ErrorCode::MalformedFrame, "empty frame"),
            FrameError::Truncated => {
                IpcError::new(ErrorCode::Unavailable, "connection closed inside a frame")
            }
            FrameError::Io(error) => IpcError::from_io("transport", error),
        }
    }
}

/// Read one frame of at most `limit` bytes. `Ok(None)` is a clean end of
/// stream at a frame boundary.
///
/// # Errors
///
/// A [`FrameError`].
pub fn read_frame(reader: &mut impl Read, limit: u32) -> Result<Option<Vec<u8>>, FrameError> {
    let mut header = [0_u8; 4];
    let mut filled = 0;
    while filled < header.len() {
        match reader.read(&mut header[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(FrameError::Truncated),
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(eof_or_io(error)),
        }
    }
    let declared = u32::from_be_bytes(header);
    if declared == 0 {
        return Err(FrameError::Empty);
    }
    if declared > limit {
        return Err(FrameError::TooLarge { declared, limit });
    }
    let mut body = vec![0_u8; declared as usize];
    reader.read_exact(&mut body).map_err(eof_or_io)?;
    Ok(Some(body))
}

fn eof_or_io(error: io::Error) -> FrameError {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        FrameError::Truncated
    } else {
        FrameError::Io(error)
    }
}

/// Serialize `value` into one frame (length prefix included) of at most
/// `limit` bytes of JSON.
///
/// # Errors
///
/// `GLINT_IPC_INTERNAL` when the value does not serialize or is over the
/// limit.
pub fn encode_frame(value: &impl Serialize, limit: u32) -> Result<Vec<u8>, IpcError> {
    let body = serde_json::to_vec(value)
        .map_err(|error| IpcError::new(ErrorCode::Internal, format!("encode: {error}")))?;
    let len = u32::try_from(body.len())
        .ok()
        .filter(|len| *len <= limit)
        .ok_or_else(|| {
            IpcError::new(
                ErrorCode::Internal,
                format!(
                    "message of {} bytes is over the {limit}-byte limit",
                    body.len()
                ),
            )
        })?;
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// Write an already-encoded frame and flush.
///
/// # Errors
///
/// The transport error.
pub fn write_frame(writer: &mut impl Write, frame: &[u8]) -> io::Result<()> {
    writer.write_all(frame)?;
    writer.flush()
}

/// A client frame that could not be decoded, with the request ID when it
/// could be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// The `request_id`, when the envelope had a valid one.
    pub request_id: Option<String>,
    /// What is wrong.
    pub error: IpcError,
}

fn malformed(request_id: Option<String>, message: impl Into<String>) -> DecodeError {
    DecodeError {
        request_id,
        error: IpcError::new(ErrorCode::MalformedFrame, message),
    }
}

/// True when `id` is an acceptable `request_id`: 1 to
/// [`MAX_REQUEST_ID_LEN`] printable ASCII characters.
pub fn valid_request_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_REQUEST_ID_LEN
        && id.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
}

/// Decode a client frame body.
///
/// # Errors
///
/// A [`DecodeError`]: `GLINT_IPC_UNKNOWN_REQUEST` for a well-formed request
/// of an unknown `type`, `GLINT_IPC_MALFORMED_FRAME` for anything else that
/// is not a valid frame.
pub fn decode_client_frame(body: &[u8]) -> Result<ClientFrame, DecodeError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| malformed(None, format!("not JSON: {error}")))?;
    let Value::Object(mut object) = value else {
        return Err(malformed(None, "frame is not a JSON object"));
    };
    let kind = match object.get("kind") {
        Some(Value::String(kind)) => kind.clone(),
        _ => return Err(malformed(None, "frame has no string `kind`")),
    };
    match kind.as_str() {
        "hello" => {
            object.remove("kind");
            serde_json::from_value::<Hello>(Value::Object(object))
                .map(ClientFrame::Hello)
                .map_err(|error| malformed(None, format!("invalid hello: {error}")))
        }
        "request" => decode_request(object).map(ClientFrame::Request),
        "goodbye" => Ok(ClientFrame::Goodbye),
        other => Err(malformed(None, format!("unknown frame kind {other:?}"))),
    }
}

fn decode_request(
    mut object: serde_json::Map<String, Value>,
) -> Result<RequestEnvelope, DecodeError> {
    let request_id = match object.get("request_id") {
        Some(Value::String(id)) if valid_request_id(id) => id.clone(),
        _ => {
            return Err(malformed(
                None,
                format!(
                    "`request_id` must be 1 to {MAX_REQUEST_ID_LEN} printable ASCII characters"
                ),
            ));
        }
    };
    let id = || Some(request_id.clone());
    let api_version = match object.get("api_version").and_then(Value::as_u64) {
        Some(version) => {
            u32::try_from(version).map_err(|_| malformed(id(), "`api_version` is out of range"))?
        }
        None => return Err(malformed(id(), "`api_version` must be an unsigned integer")),
    };
    let Some(request) = object.remove("request") else {
        return Err(malformed(id(), "missing `request`"));
    };
    let type_name = match request.get("type") {
        Some(Value::String(name)) => name.clone(),
        _ => return Err(malformed(id(), "`request` has no string `type`")),
    };
    if !GlintRequest::TYPES.contains(&type_name.as_str()) {
        return Err(DecodeError {
            request_id: id(),
            error: IpcError::new(
                ErrorCode::UnknownRequest,
                format!("unknown request type {type_name:?}"),
            ),
        });
    }
    let request = serde_json::from_value::<GlintRequest>(request)
        .map_err(|error| malformed(id(), format!("invalid {type_name:?} request: {error}")))?;
    Ok(RequestEnvelope {
        request_id,
        api_version,
        request,
    })
}

/// Decode a server frame body.
///
/// # Errors
///
/// `GLINT_IPC_MALFORMED_FRAME`.
pub fn decode_server_frame(body: &[u8]) -> Result<ServerFrame, IpcError> {
    serde_json::from_slice(body).map_err(|error| {
        IpcError::new(
            ErrorCode::MalformedFrame,
            format!("invalid server frame: {error}"),
        )
    })
}
