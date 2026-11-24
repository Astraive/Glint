//! The shell over the headless backend (COR-001): surfaces open in order,
//! failures are reported per surface, and closing releases every surface
//! and work-area reservation. Nothing touches the OS shell.

use glint_shell::{
    BackendError, BackendErrorKind, HeadlessBackend, PresentRequest, Shell, ShellError,
    ShellService, ShellStatus,
};
use glint_surfaces::{SurfaceDefinition, surface_definitions};

/// A panel (edge placement, reserves work area) and a floating calendar.
fn definitions() -> Vec<SurfaceDefinition> {
    let manifest = glint_schema::parse_theme_manifest(
        r#"{
  "schemaVersion": 1,
  "kind": "theme",
  "id": "fixture.shell",
  "name": "Fixture",
  "version": "1.0.0",
  "author": { "name": "Fixture Author" },
  "license": "MIT",
  "compatibility": { "glint": ">=0.0.0", "platforms": ["windows", "linux", "macos"] },
  "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
  "optionalCapabilities": ["surface.panel", "surface.calendar"],
  "surfaces": {
    "panel": { "entry": "panel/index.html" },
    "calendar": { "entry": "calendar/index.html" }
  }
}"#,
    )
    .unwrap();
    surface_definitions(&manifest).unwrap()
}

fn request() -> PresentRequest {
    PresentRequest::new("fixture.shell", "1.0.0", definitions())
}

#[test]
fn present_then_close_releases_every_surface_and_reservation() {
    let backend = HeadlessBackend::new();
    let mut shell = Shell::new(backend.clone());
    assert_eq!(shell.status(), ShellStatus::Idle);

    let report = shell.present(request()).unwrap();
    assert_eq!(report.backend, "headless");
    assert_eq!(report.opened.len(), 2);
    assert!(report.failed.is_empty());
    assert_eq!(report.reservations, 1, "only the edge panel reserves");
    assert_eq!(backend.open_surfaces(), vec!["calendar", "panel"]);
    assert_eq!(backend.reservations(), 1);
    assert_eq!(shell.status(), ShellStatus::Presenting);

    let closed = shell.close_all().unwrap();
    assert!(closed.is_complete());
    assert_eq!(closed.closed.len(), 2);
    assert_eq!(closed.reservations_released, 1);
    assert!(backend.open_surfaces().is_empty());
    assert_eq!(backend.reservations(), 0);
    assert_eq!(shell.status(), ShellStatus::Closed);
}

#[test]
fn closing_runs_in_reverse_opening_order() {
    let mut shell = Shell::new(HeadlessBackend::new());
    let opened = shell.present(request()).unwrap().opened;
    let mut closed = shell.close_all().unwrap().closed;
    closed.reverse();
    assert_eq!(closed, opened);
}

#[test]
fn a_surface_that_fails_to_open_is_reported_and_the_rest_open() {
    let backend = HeadlessBackend::new();
    backend.fail_open(
        "panel",
        BackendError::new(BackendErrorKind::Unsupported, "no appbar here"),
    );
    let mut shell = Shell::new(backend.clone());
    let report = shell.present(request()).unwrap();
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].id, "panel");
    assert_eq!(report.failed[0].error.kind, BackendErrorKind::Unsupported);
    assert_eq!(backend.open_surfaces(), vec!["calendar"]);
    assert_eq!(backend.reservations(), 0);
}

#[test]
fn a_surface_that_fails_to_close_is_reported_and_the_rest_close() {
    let backend = HeadlessBackend::new();
    let mut shell = Shell::new(backend.clone());
    shell.present(request()).unwrap();
    backend.fail_close(
        "calendar",
        BackendError::new(BackendErrorKind::Timeout, "host hung"),
    );
    let error = shell.close_all().unwrap_err();
    assert_eq!(error.code(), "GLINT_SHELL_CLOSE_INCOMPLETE");
    let ShellError::CloseIncomplete(report) = error else {
        panic!("expected CloseIncomplete");
    };
    assert_eq!(report.closed, vec!["panel"]);
    assert_eq!(report.reservations_released, 1);
    assert_eq!(report.failed[0].id, "calendar");
    assert_eq!(shell.status(), ShellStatus::Closed);
}

#[test]
fn presenting_twice_or_after_close_is_refused() {
    let mut shell = Shell::new(HeadlessBackend::new());
    shell.present(request()).unwrap();
    assert_eq!(
        shell.present(request()).unwrap_err().code(),
        "GLINT_SHELL_INVALID_STATE"
    );
    shell.close_all().unwrap();
    assert!(shell.present(request()).is_err());
    // Closing again is harmless.
    assert!(shell.close_all().unwrap().closed.is_empty());
}

#[test]
fn an_empty_presentation_opens_nothing() {
    let backend = HeadlessBackend::new();
    let mut shell = Shell::new(backend.clone());
    let report = shell
        .present(PresentRequest::new("official.default", "0.1.0", Vec::new()))
        .unwrap();
    assert!(report.opened.is_empty());
    assert_eq!(backend.opened_total(), 0);
}

#[test]
fn the_shell_cannot_reach_profile_or_package_state() {
    // The single-writer rule is structural: the shell crate does not depend
    // on the daemon, storage or installer crates.
    let manifest = include_str!("../Cargo.toml");
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("[dev-dependencies]")
        .next()
        .unwrap();
    for forbidden in [
        "glint-daemon",
        "glint-storage",
        "glint-installer",
        "glint-core",
    ] {
        assert!(
            !dependencies.contains(forbidden),
            "glint-shell must not depend on {forbidden}"
        );
    }
}
