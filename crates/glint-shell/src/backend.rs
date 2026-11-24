//! Surface backends: where the shell's surfaces actually appear.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use glint_surfaces::{PlacementKind, SurfaceDefinition};

/// The broad class of a backend failure, matching Glint's error
/// distinctions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BackendErrorKind {
    /// This backend cannot host the surface on this platform or session.
    Unsupported,
    /// The OS refused (for example a compositor permission).
    PermissionDenied,
    /// The backend is not available right now.
    Unavailable,
    /// The backend did not answer in time.
    Timeout,
    /// Anything else.
    Failure,
}

impl BackendErrorKind {
    /// A stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            BackendErrorKind::Unsupported => "unsupported",
            BackendErrorKind::PermissionDenied => "permission-denied",
            BackendErrorKind::Unavailable => "unavailable",
            BackendErrorKind::Timeout => "timeout",
            BackendErrorKind::Failure => "failure",
        }
    }
}

/// A backend failure for one surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError {
    /// The class of failure.
    pub kind: BackendErrorKind,
    /// Human-readable detail (not stable).
    pub message: String,
}

impl BackendError {
    /// A new error.
    pub fn new(kind: BackendErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)
    }
}

impl std::error::Error for BackendError {}

/// What a backend reports for a surface it opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct BackendSurface {
    /// Whether the surface reserved part of the desktop work area (an edge
    /// panel registered as an appbar or exclusive layer-shell zone). The
    /// reservation must be released when the surface closes.
    pub reserves_work_area: bool,
}

impl BackendSurface {
    /// A surface report.
    pub fn new(reserves_work_area: bool) -> Self {
        Self { reserves_work_area }
    }
}

/// A host for the shell's surfaces. Native hosts (Win32 appbars and
/// WebView2, layer-shell, AppKit panels) implement it in the native host
/// crates; [`HeadlessBackend`] implements it without touching the OS.
pub trait SurfaceBackend {
    /// Short backend name for diagnostics, e.g. `headless`.
    fn name(&self) -> &str;

    /// Create and show a surface.
    ///
    /// # Errors
    ///
    /// A [`BackendError`]; the shell reports it and carries on with the
    /// other surfaces.
    fn open(&mut self, definition: &SurfaceDefinition) -> Result<BackendSurface, BackendError>;

    /// Close a surface opened by [`open`](Self::open) and release its work
    /// area reservation, if any.
    ///
    /// # Errors
    ///
    /// A [`BackendError`] when the surface or its reservation could not be
    /// released.
    fn close(&mut self, id: &str) -> Result<(), BackendError>;
}

#[derive(Debug, Default)]
struct HeadlessState {
    open: BTreeMap<String, bool>,
    fail_open: BTreeMap<String, BackendError>,
    fail_close: BTreeMap<String, BackendError>,
    opened_total: usize,
}

/// A backend that records surfaces without creating any window, appbar or
/// work-area reservation; the OS shell is never modified.
///
/// It is what the core runs with in CI and until the native hosts exist.
/// Clones share state, so a test keeps one clone to inspect what the shell
/// opened and closed, and can script failures.
#[derive(Debug, Clone, Default)]
pub struct HeadlessBackend {
    state: Arc<Mutex<HeadlessState>>,
}

impl HeadlessBackend {
    /// An empty backend.
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, HeadlessState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// IDs of the surfaces open now, sorted.
    pub fn open_surfaces(&self) -> Vec<String> {
        self.state().open.keys().cloned().collect()
    }

    /// Work-area reservations a native host would hold now: one per open
    /// `edge` surface. The headless backend reserves nothing on the real
    /// desktop; this is the bookkeeping a native host must mirror.
    pub fn reservations(&self) -> usize {
        self.state()
            .open
            .values()
            .filter(|reserves| **reserves)
            .count()
    }

    /// Surfaces opened over the backend's lifetime.
    pub fn opened_total(&self) -> usize {
        self.state().opened_total
    }

    /// Make opening `id` fail with `error` (tests).
    pub fn fail_open(&self, id: &str, error: BackendError) {
        self.state().fail_open.insert(id.to_owned(), error);
    }

    /// Make closing `id` fail with `error` (tests).
    pub fn fail_close(&self, id: &str, error: BackendError) {
        self.state().fail_close.insert(id.to_owned(), error);
    }
}

impl SurfaceBackend for HeadlessBackend {
    fn name(&self) -> &str {
        "headless"
    }

    fn open(&mut self, definition: &SurfaceDefinition) -> Result<BackendSurface, BackendError> {
        let mut state = self.state();
        if let Some(error) = state.fail_open.get(&definition.id) {
            return Err(error.clone());
        }
        let reserves = definition.placement.kind == PlacementKind::Edge;
        state.open.insert(definition.id.clone(), reserves);
        state.opened_total += 1;
        Ok(BackendSurface::new(reserves))
    }

    fn close(&mut self, id: &str) -> Result<(), BackendError> {
        let mut state = self.state();
        if let Some(error) = state.fail_close.get(id) {
            return Err(error.clone());
        }
        match state.open.remove(id) {
            Some(_) => Ok(()),
            None => Err(BackendError::new(
                BackendErrorKind::Failure,
                format!("surface {id:?} is not open"),
            )),
        }
    }
}
