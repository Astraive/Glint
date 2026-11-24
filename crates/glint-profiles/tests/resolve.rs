//! Resolution fixtures for the profile composition resolver (PRF-001).
//!
//! Run with `cargo test -p glint-profiles --test resolve`. Set
//! `GLINT_BLESS=1` to rewrite the golden plans in `tests/fixtures/`.

mod common;

use std::collections::BTreeSet;
use std::fs;

use common::*;
use glint_profiles::{
    Diagnostic, HostEnvironment, Layer, ResolveCode, ResolvedPlan, Severity, SurfaceStatus,
    ThemeInput, ThemeLookup, ThemeProvider, ThemeSet, WallpaperSource, resolve,
};
use glint_schema::{CONTRAST_REQUIREMENTS, ColorScheme, TOKEN_REGISTRY, ThemeRef, TokenKind};
use glint_surfaces::Anchor;
use serde_json::json;

fn all_themes() -> ThemeSet {
    ThemeSet::new()
        .with(mono())
        .with(glassdock())
        .with(notifs())
}

fn plain() -> HostEnvironment {
    HostEnvironment::new()
}

fn codes(plan: &ResolvedPlan) -> Vec<(ResolveCode, String)> {
    plan.diagnostics
        .iter()
        .map(|d| (d.code, d.pointer.clone()))
        .collect()
}

fn find(plan: &ResolvedPlan, code: ResolveCode) -> &Diagnostic {
    plan.diagnostics
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("no {code} in {:#?}", plan.diagnostics))
}

fn with_base(extra: serde_json::Value) -> serde_json::Value {
    let mut doc = json!({
        "schemaVersion": 1,
        "id": "test-mix",
        "name": "Test mix",
        "baseTheme": { "id": "dev.example.mono", "version": "1.0.0" },
    });
    for (key, value) in extra.as_object().unwrap() {
        doc[key] = value.clone();
    }
    doc
}

// ------------------------------------------------------------ layers 1-3

#[test]
fn without_a_base_theme_the_default_theme_supplies_everything() {
    let p = profile(json!({ "schemaVersion": 1, "id": "default", "name": "Default" }));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert!(plan.diagnostics.is_empty(), "{:#?}", plan.diagnostics);
    assert_eq!(plan.surfaces.keys().collect::<Vec<_>>(), ["panel"]);
    let panel = &plan.surfaces["panel"];
    assert_eq!(panel.layer, Layer::Default);
    assert_eq!(panel.theme.key.id(), "official.default");
    assert_eq!(panel.definition.entrypoint, "surfaces/panel/index.html");
    assert_eq!(plan.tokens.len(), TOKEN_REGISTRY.len());
    assert!(plan.tokens.values().all(|t| t.layer == Layer::Default));
    assert_eq!(plan.color_scheme, ColorScheme::Dark);
    assert_eq!(plan.wallpaper, None, "the default theme ships no wallpaper");
}

#[test]
fn the_base_theme_supplies_tokens_wallpaper_and_its_surfaces() {
    let plan = resolve(
        &profile(with_base(json!({}))),
        &all_themes(),
        &default_theme(),
        &plain(),
    );
    assert!(plan.diagnostics.is_empty(), "{:#?}", plan.diagnostics);
    for id in ["panel", "dock"] {
        assert_eq!(plan.surfaces[id].layer, Layer::BaseTheme, "{id}");
        assert_eq!(plan.surfaces[id].theme.key.id(), "dev.example.mono");
    }
    assert_eq!(plan.surfaces["panel"].definition.placement.height, Some(36));
    assert_eq!(plan.tokens["background.default"].value, "#000000");
    assert!(plan.tokens.values().all(|t| t.layer == Layer::BaseTheme));
    let wallpaper = plan.wallpaper.as_ref().unwrap();
    assert_eq!(wallpaper.layer, Layer::BaseTheme);
    assert!(matches!(
        &wallpaper.source,
        WallpaperSource::ThemeAsset { path, .. } if path == "assets/mono.png"
    ));
}

#[test]
fn an_override_replaces_only_its_surface_and_imports_nothing_else() {
    let p = profile(with_base(json!({
        "surfaces": { "dock": { "theme": "dev.example.glassdock", "version": "1.1.0" } }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert!(plan.diagnostics.is_empty(), "{:#?}", plan.diagnostics);

    let dock = &plan.surfaces["dock"];
    assert_eq!(dock.layer, Layer::SurfaceOverride);
    assert_eq!(dock.theme, glassdock().release);
    assert_eq!(dock.definition.entrypoint, "glass/dock.html");
    assert_eq!(dock.definition.placement.anchor, Some(Anchor::Bottom));

    // The override theme also ships a panel, tokens and a wallpaper: none of
    // them is used, because the profile only named its dock.
    assert_eq!(plan.surfaces["panel"].theme.key.id(), "dev.example.mono");
    assert_eq!(
        plan.surfaces["panel"].definition.entrypoint,
        "mono/panel.html"
    );
    assert_eq!(plan.tokens["background.default"].value, "#000000");
    assert!(plan.tokens.values().all(|t| {
        t.theme
            .as_ref()
            .is_some_and(|r| r.key.id() == "dev.example.mono")
    }));
    assert!(matches!(
        &plan.wallpaper.as_ref().unwrap().source,
        WallpaperSource::ThemeAsset { theme, .. } if theme.key.id() == "dev.example.mono"
    ));
    // Every entrypoint belongs to the release that declares it.
    for surface in plan.surfaces.values() {
        let owner = [mono(), glassdock(), default_theme()]
            .into_iter()
            .find(|t| t.release == surface.theme)
            .unwrap();
        assert_eq!(
            owner.manifest.surfaces[&surface.id].entry,
            surface.definition.entrypoint
        );
    }
}

#[test]
fn an_override_can_add_a_surface_no_base_declares() {
    let p = profile(with_base(json!({
        "surfaces": { "notification": { "theme": "dev.example.notifs", "version": "0.4.0" } }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert!(plan.diagnostics.is_empty(), "{:#?}", plan.diagnostics);
    assert_eq!(
        plan.surfaces.keys().collect::<Vec<_>>(),
        ["dock", "notification", "panel"]
    );
    assert_eq!(plan.surfaces["notification"].layer, Layer::SurfaceOverride);
}

#[test]
fn an_override_theme_without_the_surface_falls_through_with_a_diagnostic() {
    let p = profile(with_base(json!({
        "surfaces": { "dock": { "theme": "dev.example.notifs", "version": "0.4.0" } }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert_eq!(
        codes(&plan),
        [(ResolveCode::SurfaceNotProvided, "/surfaces/dock".to_owned())]
    );
    assert_eq!(plan.diagnostics[0].surface.as_deref(), Some("dock"));
    assert_eq!(plan.diagnostics[0].severity, Severity::Error);
    assert_eq!(plan.surfaces["dock"].layer, Layer::BaseTheme);
}

#[test]
fn a_missing_or_corrupt_override_theme_is_skipped_and_reported() {
    let p = profile(with_base(json!({
        "surfaces": { "dock": { "theme": "dev.example.glassdock", "version": "1.1.0" } }
    })));
    let missing = ThemeSet::new().with(mono());
    let plan = resolve(&p, &missing, &default_theme(), &plain());
    assert_eq!(
        codes(&plan),
        [(ResolveCode::ThemeUnavailable, "/surfaces/dock".to_owned())]
    );
    assert!(plan.diagnostics[0].message.contains("not installed"));
    assert_eq!(plan.surfaces["dock"].layer, Layer::BaseTheme);

    let mut corrupt = all_themes();
    corrupt.mark_corrupt(
        "dev.example.glassdock",
        "1.1.0".parse().unwrap(),
        "digest mismatch",
    );
    let plan = resolve(&p, &corrupt, &default_theme(), &plain());
    assert_eq!(plan.diagnostics[0].code, ResolveCode::ThemeUnavailable);
    assert!(
        plan.diagnostics[0]
            .message
            .contains("corrupt: digest mismatch")
    );
    assert_eq!(plan.surfaces["dock"].layer, Layer::BaseTheme);
}

#[test]
fn a_missing_base_theme_leaves_the_default_theme() {
    let p = profile(with_base(json!({})));
    let plan = resolve(&p, &ThemeSet::new(), &default_theme(), &plain());
    assert_eq!(
        codes(&plan),
        [(ResolveCode::ThemeUnavailable, "/baseTheme".to_owned())]
    );
    assert_eq!(plan.surfaces.keys().collect::<Vec<_>>(), ["panel"]);
    assert_eq!(plan.surfaces["panel"].layer, Layer::Default);
    assert!(plan.tokens.values().all(|t| t.layer == Layer::Default));
    assert!(plan.has_errors());
}

#[test]
fn a_surface_no_layer_provides_is_unresolved() {
    let p = profile(with_base(json!({
        "surfaces": { "custom:dev.example.clock": { "theme": "dev.example.notifs", "version": "0.4.0" } }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert_eq!(
        codes(&plan),
        [
            (
                ResolveCode::SurfaceNotProvided,
                "/surfaces/custom:dev.example.clock".to_owned()
            ),
            (
                ResolveCode::SurfaceUnresolved,
                "/surfaces/custom:dev.example.clock".to_owned()
            ),
        ]
    );
    assert!(!plan.surfaces.contains_key("custom:dev.example.clock"));
}

#[test]
fn an_invalid_surface_definition_falls_through() {
    // `desktop` is not a dock placement: glint-surfaces refuses it.
    let broken = theme(
        "dev.example.broken",
        "1.0.0",
        json!({ "dock": { "entry": "dock.html", "placement": { "kind": "desktop" } } }),
        json!({}),
        None,
    );
    let themes = all_themes().with(broken);
    let p = profile(with_base(json!({
        "surfaces": { "dock": { "theme": "dev.example.broken", "version": "1.0.0" } }
    })));
    let plan = resolve(&p, &themes, &default_theme(), &plain());
    let d = find(&plan, ResolveCode::SurfaceInvalid);
    assert_eq!(d.pointer, "/surfaces/dock");
    assert!(
        d.message.contains("GLINT_SURFACE_UNSUPPORTED_PLACEMENT"),
        "{d}"
    );
    assert_eq!(plan.surfaces["dock"].layer, Layer::BaseTheme);
}

#[test]
fn a_provider_returning_another_release_is_not_trusted() {
    struct Liar(ThemeInput);
    impl ThemeProvider for Liar {
        fn lookup(&self, _: &ThemeRef) -> ThemeLookup<'_> {
            ThemeLookup::Available(&self.0)
        }
    }
    let plan = resolve(
        &profile(with_base(json!({}))),
        &Liar(glassdock()),
        &default_theme(),
        &plain(),
    );
    assert_eq!(
        codes(&plan),
        [(ResolveCode::ThemeMismatch, "/baseTheme".to_owned())]
    );
    assert!(plan.tokens.values().all(|t| t.layer == Layer::Default));
}

// ----------------------------------------------------------------- layer 4

#[test]
fn token_overrides_are_validated_against_the_registry() {
    let p = profile(with_base(json!({
        "tokenOverrides": {
            "background.default": "#0B0C0F",
            "space.md": "14px",
            "font.weight.strong": 700,
            "background.nope": "#000000",
            "surface.obsidian": "#000000",
            "radius.md": "red",
        }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert_eq!(plan.tokens["background.default"].value, "#0b0c0f");
    assert_eq!(plan.tokens["background.default"].layer, Layer::User);
    assert_eq!(plan.tokens["background.default"].theme, None);
    assert_eq!(plan.tokens["space.md"].value, "14px");
    assert_eq!(plan.tokens["font.weight.strong"].value, "700");
    assert_eq!(plan.tokens["radius.md"].layer, Layer::BaseTheme);
    assert!(!plan.tokens.contains_key("background.nope"));
    assert_eq!(plan.tokens.len(), TOKEN_REGISTRY.len());
    assert_eq!(
        codes(&plan),
        [
            (
                ResolveCode::TokenUnknown,
                "/tokenOverrides/background.nope".to_owned()
            ),
            (
                ResolveCode::TokenInvalidValue,
                "/tokenOverrides/radius.md".to_owned()
            ),
            (
                ResolveCode::TokenBrandName,
                "/tokenOverrides/surface.obsidian".to_owned()
            ),
        ]
    );
    assert!(
        plan.diagnostics
            .iter()
            .all(|d| d.severity == Severity::Warning)
    );
}

// ----------------------------------------------------------------- layer 5

#[test]
fn monitor_overrides_refine_placement_and_visibility() {
    let p = profile(with_base(json!({
        "monitors": {
            "primary": { "panel": { "anchor": "bottom", "height": 40 } },
            "secondary": { "dock": { "visible": false } },
            "display-7f3a": { "panel": {} },
        }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert!(plan.diagnostics.is_empty(), "{:#?}", plan.diagnostics);

    let panel = &plan.surfaces["panel"];
    let primary = &panel.monitors["primary"];
    assert_eq!(primary.layer, Layer::Monitor);
    assert!(primary.visible);
    assert_eq!(primary.placement.anchor, Some(Anchor::Bottom));
    assert_eq!(primary.placement.height, Some(40));
    // The surface's own placement is unchanged; only that monitor differs.
    assert_eq!(panel.definition.placement.anchor, Some(Anchor::Top));
    assert_eq!(panel.definition.placement.height, Some(36));
    // An empty override changes nothing and keeps the surface's layer.
    assert_eq!(panel.monitors["display-7f3a"].layer, Layer::BaseTheme);

    let secondary = &plan.surfaces["dock"].monitors["secondary"];
    assert!(!secondary.visible);
    assert_eq!(secondary.layer, Layer::Monitor);
}

#[test]
fn invalid_monitor_overrides_are_ignored_with_a_pointer() {
    let p = profile(with_base(json!({
        "monitors": {
            "primary": {
                "panel": { "edge": "top", "height": 44 },
                "dock": { "visible": "no", "height": 50000 },
                "launcher": { "visible": false },
            }
        }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert_eq!(
        codes(&plan),
        [
            (
                ResolveCode::MonitorOverrideInvalid,
                "/monitors/primary/dock/visible".to_owned()
            ),
            (
                ResolveCode::MonitorOverrideInvalid,
                "/monitors/primary/dock/height".to_owned()
            ),
            (
                ResolveCode::MonitorSurfaceUnknown,
                "/monitors/primary/launcher".to_owned()
            ),
            (
                ResolveCode::MonitorOverrideInvalid,
                "/monitors/primary/panel/edge".to_owned()
            ),
        ]
    );
    // The theme placement stays in force for the monitor.
    let panel = &plan.surfaces["panel"].monitors["primary"];
    assert_eq!(panel.placement, plan.surfaces["panel"].definition.placement);
    assert_eq!(panel.layer, Layer::BaseTheme);
}

// ----------------------------------------------------------------- layer 6

#[test]
fn reduced_motion_always_wins_over_theme_and_user_values() {
    let p = profile(with_base(json!({
        "tokenOverrides": { "motion.duration.fast": "100ms", "motion.duration.slow": "2s" }
    })));
    let host = HostEnvironment::new().with_reduced_motion(true);
    let plan = resolve(&p, &all_themes(), &default_theme(), &host);
    for spec in TOKEN_REGISTRY
        .iter()
        .filter(|s| s.kind == TokenKind::Duration)
    {
        let token = &plan.tokens[spec.name];
        assert_eq!(token.value, "0ms", "{}", spec.name);
        assert_eq!(token.layer, Layer::HostRestriction);
    }
    assert_eq!(
        codes(&plan),
        [
            (
                ResolveCode::OverrideRestricted,
                "/tokenOverrides/motion.duration.fast".to_owned()
            ),
            (
                ResolveCode::OverrideRestricted,
                "/tokenOverrides/motion.duration.slow".to_owned()
            ),
        ]
    );
    assert!(
        plan.diagnostics
            .iter()
            .all(|d| d.severity == Severity::Info)
    );
    // Easing is untouched.
    assert_eq!(
        plan.tokens["motion.easing.standard"].layer,
        Layer::BaseTheme
    );
}

#[test]
fn an_override_that_breaks_contrast_is_reverted() {
    let p = profile(with_base(json!({
        "tokenOverrides": {
            "text.primary": "#050505",
            "accent.default": "#123456",
        }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    let theme_value = mono().tokens.unwrap();
    assert_eq!(
        plan.tokens["text.primary"].value,
        theme_value
            .value(ColorScheme::Dark, "text.primary")
            .unwrap()
    );
    assert_eq!(plan.tokens["text.primary"].layer, Layer::BaseTheme);
    let d = find(&plan, ResolveCode::TokenLowContrast);
    assert_eq!(d.pointer, "/tokenOverrides/text.primary");
    assert_eq!(d.severity, Severity::Warning);
    // Every contrast requirement holds in the final plan.
    assert_contrast(&plan);
}

#[test]
fn a_translucent_override_in_a_contrast_pair_is_reverted() {
    let p = profile(with_base(json!({
        "tokenOverrides": { "surface.default": "#20202080" }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert_eq!(plan.tokens["surface.default"].layer, Layer::BaseTheme);
    assert_eq!(
        find(&plan, ResolveCode::TokenLowContrast).pointer,
        "/tokenOverrides/surface.default"
    );
}

#[test]
fn a_safe_override_is_kept_and_contrast_still_holds() {
    let p = profile(with_base(json!({
        "tokenOverrides": { "background.default": "#0B0C0F", "text.primary": "#FFFFFF" }
    })));
    let plan = resolve(&p, &all_themes(), &default_theme(), &plain());
    assert!(plan.diagnostics.is_empty(), "{:#?}", plan.diagnostics);
    assert_eq!(plan.tokens["text.primary"].value, "#ffffff");
    assert_contrast(&plan);
}

fn assert_contrast(plan: &ResolvedPlan) {
    for req in CONTRAST_REQUIREMENTS {
        let fg = glint_schema::Rgba::parse_hex(&plan.tokens[req.foreground].value).unwrap();
        let bg = glint_schema::Rgba::parse_hex(&plan.tokens[req.background].value).unwrap();
        assert!(
            glint_schema::contrast_ratio(fg, bg) >= req.minimum,
            "{} on {}",
            req.foreground,
            req.background
        );
    }
}

#[test]
fn an_unusable_required_capability_blocks_the_surface_whatever_the_overrides() {
    let p = profile(with_base(json!({
        "surfaces": { "dock": { "theme": "dev.example.glassdock", "version": "1.1.0" } },
        "monitors": { "primary": { "dock": { "visible": true } } },
    })));
    let host = HostEnvironment::new().with_unusable_capability("surface.dock");
    let plan = resolve(&p, &all_themes(), &default_theme(), &host);
    let dock = &plan.surfaces["dock"];
    assert_eq!(
        dock.status,
        SurfaceStatus::Blocked {
            capability: "surface.dock".to_owned()
        }
    );
    assert_eq!(plan.surfaces["panel"].status, SurfaceStatus::Active);
    assert_eq!(
        codes(&plan),
        [(ResolveCode::SurfaceBlocked, "/surfaces/dock".to_owned())]
    );
    assert_eq!(
        plan.to_json()["surfaces"]["dock"]["status"],
        json!({ "state": "blocked", "layer": "host-restriction", "capability": "surface.dock" })
    );
}

#[test]
fn the_colour_scheme_follows_the_host_when_the_tokens_offer_it() {
    let p = profile(with_base(json!({})));
    let light = HostEnvironment::new().with_color_scheme(ColorScheme::Light);
    let plan = resolve(&p, &all_themes(), &default_theme(), &light);
    assert_eq!(plan.color_scheme, ColorScheme::Light);
    assert!(plan.diagnostics.is_empty());
    let mono_tokens = mono().tokens.unwrap();
    assert_eq!(
        plan.tokens["background.default"].value,
        mono_tokens
            .value(ColorScheme::Light, "background.default")
            .unwrap()
    );

    let dark_only = theme(
        "dev.example.mono",
        "1.0.0",
        json!({}),
        json!({}),
        Some(dark_only_tokens()),
    );
    let plan = resolve(
        &p,
        &ThemeSet::new().with(dark_only),
        &default_theme(),
        &light,
    );
    assert_eq!(plan.color_scheme, ColorScheme::Dark);
    assert_eq!(
        codes(&plan),
        [(ResolveCode::SchemeUnavailable, "/baseTheme".to_owned())]
    );
    assert_eq!(plan.diagnostics[0].severity, Severity::Info);
}

#[test]
fn a_base_theme_without_tokens_keeps_the_default_palette() {
    let plain_base = theme(
        "dev.example.mono",
        "1.0.0",
        json!({ "dock": { "entry": "dock.html" } }),
        json!({}),
        None,
    );
    let plan = resolve(
        &profile(with_base(json!({}))),
        &ThemeSet::new().with(plain_base),
        &default_theme(),
        &plain(),
    );
    assert!(plan.diagnostics.is_empty());
    assert!(plan.tokens.values().all(|t| t.layer == Layer::Default));
    assert_eq!(plan.surfaces["dock"].layer, Layer::BaseTheme);
    assert_eq!(plan.surfaces["panel"].layer, Layer::Default);
}

// --------------------------------------------------------------- wallpaper

#[test]
fn wallpaper_precedence() {
    let themes = all_themes();
    let user = resolve(
        &profile(with_base(
            json!({ "wallpaper": { "path": "user-assets/me.png" } }),
        )),
        &themes,
        &default_theme(),
        &plain(),
    );
    let w = user.wallpaper.unwrap();
    assert_eq!(w.layer, Layer::User);
    assert_eq!(
        w.source,
        WallpaperSource::UserFile {
            path: "user-assets/me.png".to_owned()
        }
    );

    let chosen = resolve(
        &profile(with_base(
            json!({ "wallpaper": { "asset": "alt-wallpaper" } }),
        )),
        &themes,
        &default_theme(),
        &plain(),
    );
    let w = chosen.wallpaper.unwrap();
    assert_eq!(w.layer, Layer::User);
    assert!(
        matches!(w.source, WallpaperSource::ThemeAsset { ref path, .. } if path == "assets/mono-alt.png")
    );

    let missing = resolve(
        &profile(with_base(json!({ "wallpaper": { "asset": "nope" } }))),
        &themes,
        &default_theme(),
        &plain(),
    );
    assert_eq!(
        codes(&missing),
        [(ResolveCode::WallpaperAssetMissing, "/wallpaper".to_owned())]
    );
    assert_eq!(missing.wallpaper.unwrap().layer, Layer::BaseTheme);
}

// ------------------------------------------------------------ determinism

fn custom_mix() -> glint_schema::Profile {
    profile(serde_json::from_str(&read(&fixture_path("custom-mix.profile.json"))).unwrap())
}

#[test]
fn resolution_is_deterministic() {
    let p = custom_mix();
    let host = HostEnvironment::new()
        .with_reduced_motion(true)
        .with_unusable_capability("surface.notification");
    let forward = all_themes();
    let backward = ThemeSet::new()
        .with(notifs())
        .with(glassdock())
        .with(mono());
    let first = resolve(&p, &forward, &default_theme(), &host);
    for _ in 0..3 {
        let again = resolve(&p, &backward, &default_theme(), &host);
        assert_eq!(again, first);
        assert_eq!(
            serde_json::to_string(&again.to_json()).unwrap(),
            serde_json::to_string(&first.to_json()).unwrap()
        );
    }
}

#[test]
fn golden_plans_match() {
    let p = custom_mix();
    let cases = [
        ("custom-mix.plan.json", HostEnvironment::new()),
        (
            "custom-mix.restricted.plan.json",
            HostEnvironment::new()
                .with_color_scheme(ColorScheme::Light)
                .with_reduced_motion(true)
                .with_unusable_capability("surface.notification"),
        ),
    ];
    for (name, host) in cases {
        let plan = resolve(&p, &all_themes(), &default_theme(), &host);
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&plan.to_json()).unwrap()
        );
        let path = fixture_path(name);
        if std::env::var_os("GLINT_BLESS").is_some() {
            fs::write(&path, &actual).unwrap();
        }
        let expected = read(&path).replace("\r\n", "\n");
        assert_eq!(
            actual, expected,
            "{name} drifted; rerun with GLINT_BLESS=1 and review"
        );
    }
}

// -------------------------------------------------------------- contracts

#[test]
fn codes_and_layers_are_stable() {
    let mut seen = BTreeSet::new();
    for code in ResolveCode::ALL {
        assert!(code.as_str().starts_with("GLINT_RESOLVE_"), "{code}");
        assert!(seen.insert(code.as_str()), "duplicate {code}");
    }
    let names: Vec<_> = Layer::ALL.iter().map(|l| l.as_str()).collect();
    assert_eq!(
        names,
        [
            "default",
            "base-theme",
            "surface-override",
            "user",
            "monitor",
            "host-restriction"
        ]
    );
    assert!(Layer::ALL.windows(2).all(|w| w[0] < w[1]));
}
