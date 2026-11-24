//! The shell's service contract and its in-process implementation.

use std::fmt;

use glint_surfaces::SurfaceDefinition;

use crate::backend::{BackendError, SurfaceBackend};

/// What the shell is asked to present: a theme's resolved surfaces. Plain
/// data, so the request crosses a process boundary unchanged after COR-007.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PresentRequest {
    /// Manifest `id` of the theme.
    pub theme_id: String,
    /// Manifest `version`.
    pub version: String,
    /// Surfaces to open, in order. The daemon has already removed surfaces
    /// whose required capability is unusable.
    pub surfaces: Vec<SurfaceDefinition>,
}

impl PresentRequest {
    /// A request.
    pub fn new(
        theme_id: impl Into<String>,
        version: impl Into<String>,
        surfaces: Vec<SurfaceDefinition>,
    ) -> Self {
        Self {
            theme_id: theme_id.into(),
            version: version.into(),
            surfaces,
        }
    }
}

/// A surface that failed to open or close.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SurfaceFailure {
    /// The surface ID.
    pub id: String,
    /// The backend's error.
    pub error: BackendError,
}

/// What [`ShellService::present`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PresentReport {
    /// The backend's name.
    pub backend: String,
    /// Surfaces opened, in order.
    pub opened: Vec<String>,
    /// Surfaces holding a work-area reservation.
    pub reservations: usize,
    /// Surfaces that failed to open. They stay closed; the desktop keeps
    /// its native shell where they would have been.
    pub failed: Vec<SurfaceFailure>,
}

/// What [`ShellService::close_all`] released.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct CloseReport {
    /// Surfaces closed, in reverse opening order.
    pub closed: Vec<String>,
    /// Work-area reservations released.
    pub reservations_released: usize,
    /// Surfaces the backend failed to close.
    pub failed: Vec<SurfaceFailure>,
}

impl CloseReport {
    /// True when every surface and reservation was released.
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Where the shell is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShellStatus {
    /// Nothing presented.
    Idle,
    /// Surfaces are presented.
    Presenting,
    /// Closed for good.
    Closed,
}

/// Every way a shell call can fail.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShellError {
    /// The call does not fit the shell's status.
    InvalidState {
        /// The call.
        step: &'static str,
        /// The status it found.
        status: ShellStatus,
    },
    /// Some surfaces or reservations could not be released.
    CloseIncomplete(CloseReport),
}

impl ShellError {
    /// A stable code (`GLINT_SHELL_*`).
    pub const fn code(&self) -> &'static str {
        match self {
            ShellError::InvalidState { .. } => "GLINT_SHELL_INVALID_STATE",
            ShellError::CloseIncomplete(_) => "GLINT_SHELL_CLOSE_INCOMPLETE",
        }
    }
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShellError::InvalidState { step, status } => {
                write!(f, "{step} is not allowed while the shell is {status:?}")
            }
            ShellError::CloseIncomplete(report) => {
                let ids: Vec<&str> = report.failed.iter().map(|f| f.id.as_str()).collect();
                write!(f, "surfaces not released: {}", ids.join(", "))
            }
        }
    }
}

impl std::error::Error for ShellError {}

/// The shell's typed contract, driven by the core. In the MVP the core
/// holds a [`Shell`] directly; after the process split (COR-007) an IPC
/// client implements the same trait.
pub trait ShellService {
    /// Open a theme's surfaces. A surface the backend cannot open is
    /// reported in [`PresentReport::failed`], not returned as an error.
    ///
    /// # Errors
    ///
    /// [`ShellError::InvalidState`] unless the shell is idle.
    fn present(&mut self, request: PresentRequest) -> Result<PresentReport, ShellError>;

    /// Where the shell is in its lifecycle.
    fn status(&self) -> ShellStatus;

    /// Close every surface in reverse order and release every work-area
    /// reservation. Safe to call in any status; the shell is closed after.
    ///
    /// # Errors
    ///
    /// [`ShellError::CloseIncomplete`] listing what stayed open.
    fn close_all(&mut self) -> Result<CloseReport, ShellError>;
}

#[derive(Debug, Clone)]
struct OpenSurface {
    id: String,
    reserves_work_area: bool,
}

/// The shell over a [`SurfaceBackend`].
#[derive(Debug)]
pub struct Shell<B> {
    backend: B,
    open: Vec<OpenSurface>,
    status: ShellStatus,
}

impl<B: SurfaceBackend> Shell<B> {
    /// A shell presenting through `backend`.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            open: Vec::new(),
            status: ShellStatus::Idle,
        }
    }

    /// The backend.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// IDs of the surfaces open now, in opening order.
    pub fn open_surfaces(&self) -> Vec<&str> {
        self.open
            .iter()
            .map(|surface| surface.id.as_str())
            .collect()
    }
}

impl<B: SurfaceBackend> ShellService for Shell<B> {
    fn present(&mut self, request: PresentRequest) -> Result<PresentReport, ShellError> {
        if self.status != ShellStatus::Idle {
            return Err(ShellError::InvalidState {
                step: "present",
                status: self.status,
            });
        }
        let mut report = PresentReport {
            backend: self.backend.name().to_owned(),
            opened: Vec::new(),
            reservations: 0,
            failed: Vec::new(),
        };
        for definition in &request.surfaces {
            match self.backend.open(definition) {
                Ok(surface) => {
                    report.opened.push(definition.id.clone());
                    report.reservations += usize::from(surface.reserves_work_area);
                    self.open.push(OpenSurface {
                        id: definition.id.clone(),
                        reserves_work_area: surface.reserves_work_area,
                    });
                }
                Err(error) => report.failed.push(SurfaceFailure {
                    id: definition.id.clone(),
                    error,
                }),
            }
        }
        self.status = ShellStatus::Presenting;
        Ok(report)
    }

    fn status(&self) -> ShellStatus {
        self.status
    }

    fn close_all(&mut self) -> Result<CloseReport, ShellError> {
        let mut report = CloseReport::default();
        while let Some(surface) = self.open.pop() {
            match self.backend.close(&surface.id) {
                Ok(()) => {
                    report.reservations_released += usize::from(surface.reserves_work_area);
                    report.closed.push(surface.id);
                }
                Err(error) => report.failed.push(SurfaceFailure {
                    id: surface.id,
                    error,
                }),
            }
        }
        self.status = ShellStatus::Closed;
        if report.is_complete() {
            Ok(report)
        } else {
            Err(ShellError::CloseIncomplete(report))
        }
    }
}
