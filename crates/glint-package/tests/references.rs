//! Manifest references and static content restrictions (PKG-004).
//!
//! Every referenced file and surface HTML entrypoint must exist inside the
//! archive; surfaces must pass `glint-surfaces` validation; capability IDs
//! from the surface catalog are accepted and unknown ones refused; UI files
//! must not load remote content, SVG images must be inert, and native
//! executables and install scripts are refused. Every fixture is generated at
//! test time by `support::build_zip` (nothing is committed) and every
//! rejection must leave the sandbox unchanged.
//!
//! The fixture list and expected codes are documented in
//! `docs/security/package-fixtures.md`. Run with
//! `cargo test -p glint-package --test references`.

mod support;

use glint_package::{
    ErrorCode as C, PROVIDER_CAPABILITIES, PackageError, PackageLimits, inspect_archive_in,
    known_capabilities,
};
use glint_schema::ErrorCode as M;
use glint_surfaces::{CATALOG, SurfaceErrorCode as S};
use support::*;

fn rejected(bytes: &[u8]) -> PackageError {
    let sandbox = Sandbox::new(bytes);
    let before = sandbox.snapshot();
    let error = inspect_archive_in(
        &sandbox.archive,
        &PackageLimits::default(),
        &sandbox.staging_root,
    )
    .expect_err("hostile fixture must be rejected");
    assert_eq!(
        sandbox.snapshot(),
        before,
        "rejection must leave no file behind (error: {error})"
    );
    assert_eq!(
        std::fs::read_dir(&sandbox.staging_root).unwrap().count(),
        0,
        "staging root must be empty after a rejection"
    );
    error
}

fn accepted(bytes: &[u8]) -> glint_schema::ThemeManifest {
    let sandbox = Sandbox::new(bytes);
    let candidate = inspect_archive_in(
        &sandbox.archive,
        &PackageLimits::default(),
        &sandbox.staging_root,
    )
    .unwrap_or_else(|error| panic!("fixture must be accepted: {error}"));
    // Dropping the candidate removes its staging directory before the
    // sandbox goes away.
    candidate.manifest.clone()
}

macro_rules! rejects {
    ($name:ident, $bytes:expr, $code:ident) => {
        #[test]
        fn $name() {
            let error = rejected(&$bytes);
            assert_eq!(error.code(), C::$code, "unexpected error: {error}");
        }
    };
}

macro_rules! rejects_surface {
    ($name:ident, $manifest:expr, $surface_code:ident, $pointer:expr) => {
        #[test]
        fn $name() {
            let error = rejected(&with_manifest(&$manifest));
            assert_eq!(error.code(), C::InvalidSurface, "unexpected error: {error}");
            let surface = error.surface_error().expect("surface diagnostic");
            assert_eq!(surface.code(), S::$surface_code, "{error}");
            assert_eq!(surface.pointer(), $pointer, "{error}");
        }
    };
}

macro_rules! rejects_manifest {
    ($name:ident, $manifest:expr, $manifest_code:ident) => {
        #[test]
        fn $name() {
            let error = rejected(&with_manifest(&$manifest));
            assert_eq!(
                error.code(),
                C::InvalidManifest,
                "unexpected error: {error}"
            );
            assert_eq!(
                error.manifest_error().map(|e| e.code()),
                Some(M::$manifest_code),
                "unexpected manifest code: {error}"
            );
        }
    };
}

/// The baseline manifest with the `panel` surface object replaced.
fn panel(surface: &str) -> String {
    GOOD_MANIFEST.replace(r#"{ "entry": "dist/panel/index.html" }"#, surface)
}

/// The baseline manifest with the capability lists replaced.
fn capabilities(required: &[&str], optional: &[&str]) -> String {
    let list = |ids: &[&str]| {
        ids.iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    GOOD_MANIFEST
        .replace(
            r#""requiredCapabilities": ["surface.panel"]"#,
            &format!(r#""requiredCapabilities": [{}]"#, list(required)),
        )
        .replace(
            r#""optionalCapabilities": ["system.audio.read"]"#,
            &format!(r#""optionalCapabilities": [{}]"#, list(optional)),
        )
}

fn svg(content: &str) -> Vec<u8> {
    good_plus(vec![RawEntry::deflated("assets/icon.svg", content)])
}

// ---- Surface entrypoints must exist -------------------------------------------

rejects!(
    missing_surface_entrypoint,
    with_manifest(&GOOD_MANIFEST.replace(
        r#""surfaces": {"#,
        r#""surfaces": { "launcher": { "entry": "dist/launcher/index.html" },"#
    )),
    MissingReferencedFile
);
rejects!(
    surface_entrypoint_differs_in_case,
    with_manifest(&panel(r#"{ "entry": "dist/panel/Index.html" }"#)),
    MissingReferencedFile
);
rejects!(
    surface_entrypoint_is_a_directory,
    {
        let mut entries = good_entries();
        entries.push(RawEntry::stored("dist/shell.html/", ""));
        entries.push(RawEntry::stored("dist/shell.html/x.txt", "x"));
        entries[0] = RawEntry::deflated("glint.json", panel(r#"{ "entry": "dist/shell.html" }"#));
        build_zip(&entries)
    },
    MissingReferencedFile
);

#[test]
fn missing_entrypoint_names_the_path() {
    let error = rejected(&with_manifest(&panel(r#"{ "entry": "dist/gone.html" }"#)));
    match &error {
        PackageError::MissingReferencedFile(path) => assert_eq!(path, "dist/gone.html"),
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn missing_entrypoint_remote_script_and_executables_have_distinct_codes() {
    let missing = rejected(&with_manifest(&panel(r#"{ "entry": "dist/gone.html" }"#))).code();
    let remote = rejected(&with_file(
        "dist/panel/index.html",
        r#"<script src="https://cdn.example.com/lib.js"></script>"#,
    ))
    .code();
    let exe = rejected(&good_plus(vec![RawEntry::stored("bin/tool.exe", "x")])).code();
    let sh = rejected(&good_plus(vec![RawEntry::stored("install.sh", "echo")])).code();
    assert_eq!(missing, C::MissingReferencedFile);
    assert_eq!(remote, C::RemoteReference);
    assert_eq!(exe, C::ExecutableContent);
    assert_eq!(sh, C::ExecutableContent);
    assert_ne!(missing, remote);
    assert_ne!(missing, exe);
    assert_ne!(remote, exe);
}

// ---- Remote and file:// page entries, install scripts in the manifest ----------

rejects_manifest!(
    file_url_entrypoint,
    panel(r#"{ "entry": "file:///C:/Users/victim/panel.html" }"#),
    InvalidPath
);
rejects_manifest!(
    data_url_entrypoint,
    panel(r#"{ "entry": "data:text/html,<script>x()</script>.html" }"#),
    InvalidPath
);
rejects_manifest!(
    absolute_entrypoint,
    panel(r#"{ "entry": "/srv/panel/index.html" }"#),
    InvalidPath
);
rejects_manifest!(
    install_script_field,
    GOOD_MANIFEST.replace(
        r#""tokens":"#,
        r#""scripts": { "postinstall": "install.ps1" }, "tokens":"#
    ),
    UnknownField
);

// ---- Surface placement and behaviour ------------------------------------------

rejects_surface!(
    unknown_surface_kind,
    GOOD_MANIFEST.replace(r#""panel": {"#, r#""taskbar": {"#),
    UnknownKind,
    "/surfaces/taskbar"
);
rejects_surface!(
    surface_placement_out_of_range,
    panel(
        r#"{ "entry": "dist/panel/index.html", "placement": { "anchor": "top", "height": 99999 } }"#
    ),
    OutOfRange,
    "/surfaces/panel/placement/height"
);
rejects_surface!(
    surface_placement_unknown_field,
    panel(r#"{ "entry": "dist/panel/index.html", "placement": { "zIndex": 9 } }"#),
    UnknownField,
    "/surfaces/panel/placement/zIndex"
);
rejects_surface!(
    surface_placement_kind_not_allowed,
    panel(r#"{ "entry": "dist/panel/index.html", "placement": { "kind": "desktop" } }"#),
    UnsupportedPlacement,
    "/surfaces/panel/placement/kind"
);
rejects_surface!(
    surface_behavior_conflict,
    panel(r#"{ "entry": "dist/panel/index.html", "behavior": { "clickThrough": true } }"#),
    Conflict,
    "/surfaces/panel/behavior/clickThrough"
);

#[test]
fn surface_check_runs_before_the_entrypoint_existence_check() {
    // Invalid placement and a missing entrypoint: the surface definition is
    // reported first, with the underlying GLINT_SURFACE_* code.
    let error = rejected(&with_manifest(&panel(
        r#"{ "entry": "dist/gone.html", "placement": { "anchor": "top", "height": 0 } }"#,
    )));
    assert_eq!(error.code(), C::InvalidSurface, "{error}");
    assert_eq!(
        error.surface_error().map(|e| e.code().as_str()),
        Some("GLINT_SURFACE_OUT_OF_RANGE")
    );
}

#[test]
fn valid_placement_behavior_and_custom_surface_are_accepted() {
    let manifest = panel(
        r#"{ "entry": "dist/panel/index.html",
             "placement": { "kind": "edge", "anchor": "bottom", "height": 48 },
             "behavior": { "reserveWorkArea": true } },
           "custom:dev.example.clock": { "entry": "dist/panel/index.html",
             "placement": { "kind": "floating", "anchor": "top-right", "width": 200, "height": 80 },
             "behavior": { "focus": "none" } }"#,
    );
    let manifest = accepted(&with_manifest(&manifest));
    assert_eq!(manifest.surfaces.len(), 2);
}

// ---- Capability IDs: catalog accepted, unknown refused (fail closed) ------------

#[test]
fn every_known_capability_is_accepted_by_schema_and_reader() {
    let known: Vec<&str> = known_capabilities().into_iter().collect();
    accepted(&with_manifest(&capabilities(&[], &known)));
    accepted(&with_manifest(&capabilities(&known, &[])));
}

#[test]
fn surface_catalog_capabilities_are_accepted() {
    for entry in CATALOG {
        let mut ids = vec![entry.capability];
        ids.extend(entry.optional_capabilities.iter().copied());
        accepted(&with_manifest(&capabilities(&ids, &[])));
    }
    accepted(&with_manifest(&capabilities(&["surface.custom"], &[])));
}

#[test]
fn provider_capabilities_are_known() {
    let known = known_capabilities();
    for id in PROVIDER_CAPABILITIES {
        assert!(known.contains(id), "{id}");
    }
}

rejects!(
    unknown_surface_capability,
    with_manifest(&capabilities(&["surface.taskbar"], &[])),
    UnknownCapability
);
rejects!(
    unknown_provider_capability_next_to_catalog_ones,
    with_manifest(&capabilities(&["surface.dock"], &["desktop.windows.close"])),
    UnknownCapability
);

// ---- External script/style dependencies in UI files ------------------------------

rejects!(
    script_src_after_slash,
    with_file(
        "dist/panel/index.html",
        r#"<script/src="https://cdn.example.com/lib.js"></script>"#
    ),
    RemoteReference
);
rejects!(
    file_url_script_src,
    with_file(
        "dist/panel/index.html",
        r#"<script src="file:///C:/Windows/evil.js"></script>"#
    ),
    RemoteReference
);
rejects!(
    remote_stylesheet_after_quoted_attribute,
    with_file(
        "dist/panel/index.html",
        r#"<link rel="stylesheet"href="https://fonts.example.com/f.css">"#
    ),
    RemoteReference
);
rejects!(
    anchor_ping_beacon,
    with_file(
        "dist/panel/index.html",
        r##"<a href="#top" ping="https://tracker.example/p">top</a>"##
    ),
    RemoteReference
);
rejects!(
    css_remote_image_set,
    good_plus(vec![RawEntry::deflated(
        "dist/panel/style.css",
        r#"body { background: image-set("https://cdn.example.com/bg.png" 1x); }"#
    )]),
    RemoteReference
);
rejects!(
    css_remote_import_without_url,
    good_plus(vec![RawEntry::deflated(
        "dist/panel/style.css",
        r#"@import "https://fonts.example.com/f.css";"#
    )]),
    RemoteReference
);
rejects!(
    js_remote_websocket,
    with_file(
        "dist/panel/main.js",
        "const s = new WebSocket('wss://c2.example/socket');"
    ),
    RemoteReference
);
rejects!(
    svg_remote_image,
    svg(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="https://cdn.example.com/x.png"/></svg>"#
    ),
    RemoteReference
);
rejects!(
    svg_remote_stylesheet_instruction,
    svg(
        r#"<?xml-stylesheet href="https://cdn.example.com/x.css"?><svg xmlns="http://www.w3.org/2000/svg"/>"#
    ),
    RemoteReference
);

// ---- Active content in SVG images ----------------------------------------------

rejects!(
    svg_script,
    svg(r#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#),
    ActiveContent
);
rejects!(
    svg_foreign_object,
    svg(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><iframe/></foreignObject></svg>"#
    ),
    ActiveContent
);
rejects!(
    svg_event_handler,
    svg(r#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"/>"#),
    ActiveContent
);
rejects!(
    svg_javascript_url,
    svg(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><a href="javascript:alert(1)"><rect/></a></svg>"#
    ),
    ActiveContent
);
rejects!(
    svg_entity_declaration,
    svg(
        r#"<!DOCTYPE svg [<!ENTITY lol "lol">]><svg xmlns="http://www.w3.org/2000/svg">&lol;</svg>"#
    ),
    ActiveContent
);

#[test]
fn inert_svg_is_accepted() {
    accepted(&svg(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" role="img"><title>Icon</title><rect width="10" height="10" fill="#123"/></svg>"##,
    ));
}

#[test]
fn every_new_code_is_listed_and_prefixed() {
    for code in [C::InvalidSurface, C::ActiveContent] {
        assert!(C::ALL.contains(&code));
        assert!(code.as_str().starts_with("GLINT_PACKAGE_"));
    }
}
