//! Glint's surface shell (COR-001).
//!
//! In the single-process MVP (ADR 0001) the shell is a library hosted by the
//! core process next to `glint-daemon`, composed by `glint-core`. It opens a
//! theme's resolved surfaces through a [`SurfaceBackend`] and closes them,
//! releasing every work-area reservation, on shutdown.
//!
//! The shell has no dependency on `glint-daemon`, `glint-storage` or
//! `glint-installer`: it receives a [`PresentRequest`] (plain data) and holds
//! no handle that could write profile or package state, so the daemon stays
//! the single writer. A crash in the shell's surfaces therefore cannot
//! corrupt installed state, and moving the shell into its own process
//! (COR-007) changes only how [`ShellService`] calls travel.
//!
//! [`HeadlessBackend`] records surfaces without creating windows or
//! touching the OS shell; native backends live in the native host crates.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod backend;
mod shell;

pub use backend::{
    BackendError, BackendErrorKind, BackendSurface, HeadlessBackend, SurfaceBackend,
};
pub use shell::{
    CloseReport, PresentReport, PresentRequest, Shell, ShellError, ShellService, ShellStatus,
    SurfaceFailure,
};
