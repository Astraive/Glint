//! The composition resolver (PRF-001).

use std::collections::{BTreeMap, BTreeSet};

use glint_schema::{
    CONTRAST_REQUIREMENTS, ColorScheme, Profile, Rgba, Surface, TOKEN_REGISTRY, ThemeRef,
    TokenErrorCode, TokenKind, TokenValue, Wallpaper, contrast_ratio, validate_token_value,
};
use glint_surfaces::SurfaceDefinition;
use serde_json::{Map, Value};

use crate::diagnostic::{Diagnostic, ResolveCode, child_pointer};
use crate::input::{HostEnvironment, ThemeInput, ThemeLookup, ThemeProvider};
use crate::plan::{
    Fallback, Layer, MonitorOverride, ResolvedPlan, ResolvedSurface, ResolvedToken,
    ResolvedWallpaper, SurfaceStatus, WallpaperSource,
};

/// The asset name a theme uses for its wallpaper.
pub const WALLPAPER_ASSET: &str = "wallpaper";

/// The value a duration token takes under reduced motion.
pub const REDUCED_MOTION_DURATION: &str = "0ms";

/// Resolve `profile` into a [`ResolvedPlan`].
///
/// Precedence, lowest first (see [`Layer`]): the bundled `default_theme`,
/// the profile's base theme, per-surface override themes, the user's token
/// overrides and wallpaper, per-monitor overrides, then `host` restrictions,
/// which always win. The function is pure and deterministic: the same inputs
/// always give an equal plan, with diagnostics in the same order. It never
/// fails; whatever it cannot honour is reported as a [`Diagnostic`] and the
/// next lower layer applies.
///
/// Themes are only *read*: a surface's entrypoint is always a path inside
/// the release that declares the surface, and naming a theme in an override
/// never pulls code, tokens or other surfaces from it.
pub fn resolve<P: ThemeProvider + ?Sized>(
    profile: &Profile,
    themes: &P,
    default_theme: &ThemeInput,
    host: &HostEnvironment,
) -> ResolvedPlan {
    let mut diagnostics = Vec::new();
    let mut restrictions = Vec::new();

    let mut base_failure = None;
    let base = match &profile.base_theme {
        None => None,
        Some(theme) => match lookup(themes, theme, "/baseTheme", None, &mut diagnostics) {
            Ok(input) => Some(input),
            Err(reason) => {
                base_failure = Some((label(theme), reason));
                None
            }
        },
    };

    let requested_base = RequestedBase {
        theme: base,
        failure: base_failure,
    };
    let (mut surfaces, raw) = resolve_surfaces(
        profile,
        themes,
        &requested_base,
        default_theme,
        host,
        &mut diagnostics,
        &mut restrictions,
    );
    if let Some((theme, reason)) = &requested_base.failure {
        diagnostics.push(Diagnostic::new(
            ResolveCode::FallbackToDefault,
            "/baseTheme",
            None,
            format!(
                "{theme} is unusable ({reason}): tokens and wallpaper come from the default theme"
            ),
        ));
    }
    let (color_scheme, tokens) = resolve_tokens(
        profile,
        base,
        default_theme,
        host,
        &mut diagnostics,
        &mut restrictions,
    );
    let wallpaper = resolve_wallpaper(profile, base, default_theme, &mut diagnostics);
    apply_monitors(profile, &mut surfaces, &raw, &mut diagnostics);

    diagnostics.extend(restrictions);
    ResolvedPlan {
        profile: profile.id.clone(),
        color_scheme,
        tokens,
        surfaces,
        wallpaper,
        diagnostics,
    }
}

fn label(theme: &ThemeRef) -> String {
    format!("{}@{}", theme.key.id(), theme.version)
}

/// Look a reference up, reporting why it cannot be used. The error is the
/// stable fallback reason.
fn lookup<'a, P: ThemeProvider + ?Sized>(
    themes: &'a P,
    theme: &ThemeRef,
    pointer: &str,
    surface: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<&'a ThemeInput, String> {
    let label = label(theme);
    match themes.lookup(theme) {
        ThemeLookup::Available(input)
            if input.release.key == theme.key && input.release.version == theme.version =>
        {
            Ok(input)
        }
        ThemeLookup::Available(input) => {
            diagnostics.push(Diagnostic::new(
                ResolveCode::ThemeMismatch,
                pointer,
                surface,
                format!("asked for {label} but was given {}", input.label()),
            ));
            Err("mismatch".to_owned())
        }
        ThemeLookup::Unavailable(reason) => {
            diagnostics.push(Diagnostic::new(
                ResolveCode::ThemeUnavailable,
                pointer,
                surface,
                format!("{label} is unavailable ({reason}); its layer is skipped"),
            ));
            Err(reason.as_str().to_owned())
        }
    }
}

type RawSurfaces<'a> = BTreeMap<String, &'a Surface>;

/// The base theme as requested: the usable release, or why it is not.
struct RequestedBase<'a> {
    theme: Option<&'a ThemeInput>,
    /// `(id@version, reason)` when the profile names a base theme that
    /// cannot be used.
    failure: Option<(String, String)>,
}

fn resolve_surfaces<'a, P: ThemeProvider + ?Sized>(
    profile: &Profile,
    themes: &'a P,
    requested_base: &RequestedBase<'a>,
    default_theme: &'a ThemeInput,
    host: &HostEnvironment,
    diagnostics: &mut Vec<Diagnostic>,
    restrictions: &mut Vec<Diagnostic>,
) -> (BTreeMap<String, ResolvedSurface>, RawSurfaces<'a>) {
    let base = requested_base.theme;
    let mut ids: BTreeSet<&str> = default_theme
        .manifest
        .surfaces
        .keys()
        .map(String::as_str)
        .collect();
    if let Some(base) = base {
        ids.extend(base.manifest.surfaces.keys().map(String::as_str));
    }
    ids.extend(profile.surfaces.keys().map(String::as_str));

    let mut resolved = BTreeMap::new();
    let mut raw = BTreeMap::new();
    for id in ids {
        let override_pointer = child_pointer("/surfaces", id);
        let overridden = profile
            .surfaces
            .get(id)
            .map(|theme| lookup(themes, theme, &override_pointer, Some(id), diagnostics));
        // The first layer the profile asked for that could not be used:
        // (layer, theme, reason, pointer).
        let mut failure: Option<(Layer, String, String, String)> = None;
        if let (Some(Err(reason)), Some(theme)) = (&overridden, profile.surfaces.get(id)) {
            failure = Some((
                Layer::SurfaceOverride,
                label(theme),
                reason.clone(),
                override_pointer.clone(),
            ));
        }
        let mut candidates: Vec<(Layer, &ThemeInput, &str)> = Vec::with_capacity(3);
        if let Some(Ok(theme)) = &overridden {
            candidates.push((Layer::SurfaceOverride, *theme, &override_pointer));
        }
        if let Some(base) = base {
            candidates.push((Layer::BaseTheme, base, "/baseTheme"));
        }
        candidates.push((Layer::Default, default_theme, ""));

        let mut winner = None;
        for (layer, theme, pointer) in candidates {
            if layer == Layer::Default
                && failure.is_none()
                && let Some((base_label, reason)) = &requested_base.failure
            {
                failure = Some((
                    Layer::BaseTheme,
                    base_label.clone(),
                    reason.clone(),
                    "/baseTheme".to_owned(),
                ));
            }
            let Some(surface) = theme.manifest.surfaces.get(id) else {
                if layer == Layer::SurfaceOverride {
                    failure.get_or_insert_with(|| {
                        (
                            layer,
                            theme.label(),
                            "not-provided".to_owned(),
                            pointer.to_owned(),
                        )
                    });
                    diagnostics.push(Diagnostic::new(
                        ResolveCode::SurfaceNotProvided,
                        pointer,
                        Some(id),
                        format!("{} does not declare the {id} surface", theme.label()),
                    ));
                }
                continue;
            };
            match SurfaceDefinition::from_manifest_surface(id, surface) {
                Ok(definition) => {
                    winner = Some((layer, theme, surface, definition));
                    break;
                }
                Err(err) => {
                    failure.get_or_insert_with(|| {
                        (
                            layer,
                            theme.label(),
                            "invalid".to_owned(),
                            pointer.to_owned(),
                        )
                    });
                    diagnostics.push(Diagnostic::new(
                        ResolveCode::SurfaceInvalid,
                        pointer,
                        Some(id),
                        format!(
                            "{} declares an invalid {id} surface: {} {} (at {})",
                            theme.label(),
                            err.code().as_str(),
                            err.message(),
                            err.pointer()
                        ),
                    ));
                }
            }
        }
        let Some((layer, theme, surface, definition)) = winner else {
            if overridden.is_some() {
                diagnostics.push(Diagnostic::new(
                    ResolveCode::SurfaceUnresolved,
                    override_pointer.as_str(),
                    Some(id),
                    format!("no layer provides the {id} surface; it is not shown"),
                ));
            }
            continue;
        };
        let fallback = failure.filter(|(requested, ..)| layer < *requested).map(
            |(requested, theme_label, reason, pointer)| {
                let code = if layer == Layer::BaseTheme {
                    ResolveCode::FallbackToBase
                } else {
                    ResolveCode::FallbackToDefault
                };
                diagnostics.push(Diagnostic::new(
                    code,
                    pointer,
                    Some(id),
                    format!(
                        "the {id} surface comes from {} ({layer}) because {theme_label} is \
                         unusable ({reason})",
                        theme.label()
                    ),
                ));
                Fallback {
                    requested,
                    theme: theme_label,
                    reason,
                    to: layer,
                }
            },
        );
        let status = match definition
            .capabilities
            .required
            .iter()
            .find(|capability| host.unusable_capabilities.contains(capability.as_str()))
        {
            None => SurfaceStatus::Active,
            Some(capability) => {
                let pointer = if overridden.is_some() {
                    override_pointer.as_str()
                } else {
                    ""
                };
                restrictions.push(Diagnostic::new(
                    ResolveCode::SurfaceBlocked,
                    pointer,
                    Some(id),
                    format!(
                        "the host cannot offer {capability}, which the {id} surface requires; \
                         it is not shown"
                    ),
                ));
                SurfaceStatus::Blocked {
                    capability: capability.as_str().to_owned(),
                }
            }
        };
        raw.insert(id.to_owned(), surface);
        resolved.insert(
            id.to_owned(),
            ResolvedSurface {
                id: id.to_owned(),
                layer,
                theme: theme.release.clone(),
                definition,
                monitors: BTreeMap::new(),
                status,
                fallback,
            },
        );
    }
    (resolved, raw)
}

fn resolve_tokens(
    profile: &Profile,
    base: Option<&ThemeInput>,
    default_theme: &ThemeInput,
    host: &HostEnvironment,
    diagnostics: &mut Vec<Diagnostic>,
    restrictions: &mut Vec<Diagnostic>,
) -> (ColorScheme, BTreeMap<String, ResolvedToken>) {
    let source = base
        .and_then(|theme| theme.tokens.as_ref().map(|t| (Layer::BaseTheme, theme, t)))
        .or_else(|| {
            default_theme
                .tokens
                .as_ref()
                .map(|t| (Layer::Default, default_theme, t))
        });
    let mut tokens = BTreeMap::new();
    let scheme = match source {
        None => {
            diagnostics.push(Diagnostic::new(
                ResolveCode::TokensUnavailable,
                "",
                None,
                "neither the base theme nor the default theme supplies tokens",
            ));
            host.color_scheme.unwrap_or(ColorScheme::Dark)
        }
        Some((layer, theme, file)) => {
            let scheme = match host.color_scheme {
                Some(wanted) if file.colors.contains_key(&wanted) => wanted,
                Some(wanted) => {
                    let fallback = file.default_scheme();
                    diagnostics.push(Diagnostic::new(
                        ResolveCode::SchemeUnavailable,
                        if layer == Layer::BaseTheme {
                            "/baseTheme"
                        } else {
                            ""
                        },
                        None,
                        format!(
                            "{} has no {} colours; using {}",
                            theme.label(),
                            wanted.as_str(),
                            fallback.as_str()
                        ),
                    ));
                    fallback
                }
                None => file.default_scheme(),
            };
            for spec in TOKEN_REGISTRY {
                if let Some(value) = file.value(scheme, spec.name) {
                    tokens.insert(
                        spec.name.to_owned(),
                        ResolvedToken {
                            value: value.to_owned(),
                            layer,
                            theme: Some(theme.release.clone()),
                        },
                    );
                }
            }
            scheme
        }
    };
    let theme_values = tokens.clone();

    // Layer 4: user token overrides, validated against the registry.
    let mut overridden: BTreeMap<String, String> = BTreeMap::new();
    for (name, value) in &profile.token_overrides {
        let pointer = child_pointer("/tokenOverrides", name);
        let json = match value {
            TokenValue::String(text) => Value::String(text.clone()),
            TokenValue::Number(number) => Value::Number(number.clone()),
            _ => Value::Null,
        };
        match validate_token_value(name, &json, &pointer) {
            Ok(css) => {
                tokens.insert(
                    name.clone(),
                    ResolvedToken {
                        value: css,
                        layer: Layer::User,
                        theme: None,
                    },
                );
                overridden.insert(name.clone(), pointer);
            }
            Err(err) => {
                let code = match err.code() {
                    TokenErrorCode::UnknownToken => ResolveCode::TokenUnknown,
                    TokenErrorCode::BrandName => ResolveCode::TokenBrandName,
                    _ => ResolveCode::TokenInvalidValue,
                };
                diagnostics.push(Diagnostic::new(
                    code,
                    pointer,
                    None,
                    format!("override ignored: {} {}", err.code(), err.message()),
                ));
            }
        }
    }

    // Layer 6 (safety): an override may not break a contrast requirement.
    // Revert the overrides involved until every pair passes again; theme
    // values passed `parse_tokens`, so this terminates with a safe palette.
    loop {
        let mut reverted = false;
        for requirement in CONTRAST_REQUIREMENTS {
            let names = [requirement.foreground, requirement.background];
            if !names.iter().any(|name| overridden.contains_key(*name)) {
                continue;
            }
            let colour = |name: &str| {
                tokens
                    .get(name)
                    .and_then(|t| Rgba::parse_hex(&t.value))
                    .filter(|c| c.a == 255)
            };
            let (Some(fg), Some(bg)) = (
                colour(requirement.foreground),
                colour(requirement.background),
            ) else {
                if theme_values.contains_key(requirement.foreground)
                    && theme_values.contains_key(requirement.background)
                {
                    // A translucent override in a contrast pair cannot be
                    // checked, so it is not allowed.
                    reverted |= revert(
                        &names,
                        &mut overridden,
                        &mut tokens,
                        &theme_values,
                        diagnostics,
                        &format!(
                            "{} on {} must be opaque to be checked",
                            requirement.foreground, requirement.background
                        ),
                    );
                }
                continue;
            };
            let ratio = contrast_ratio(fg, bg);
            if ratio < requirement.minimum {
                reverted |= revert(
                    &names,
                    &mut overridden,
                    &mut tokens,
                    &theme_values,
                    diagnostics,
                    &format!(
                        "{} on {} would be {ratio:.2}:1, below {}:1",
                        requirement.foreground, requirement.background, requirement.minimum
                    ),
                );
            }
        }
        if !reverted {
            break;
        }
    }

    // Layer 6: reduced motion zeroes every duration.
    if host.reduced_motion {
        for spec in TOKEN_REGISTRY
            .iter()
            .filter(|s| s.kind == TokenKind::Duration)
        {
            if let Some(pointer) = overridden.get(spec.name) {
                restrictions.push(Diagnostic::new(
                    ResolveCode::OverrideRestricted,
                    pointer.as_str(),
                    None,
                    format!(
                        "reduced motion is on: {} is {REDUCED_MOTION_DURATION}",
                        spec.name
                    ),
                ));
            }
            tokens.insert(
                spec.name.to_owned(),
                ResolvedToken {
                    value: REDUCED_MOTION_DURATION.to_owned(),
                    layer: Layer::HostRestriction,
                    theme: None,
                },
            );
        }
    }
    (scheme, tokens)
}

/// Revert every override among `names` to its theme value. Returns whether
/// anything was reverted.
fn revert(
    names: &[&str],
    overridden: &mut BTreeMap<String, String>,
    tokens: &mut BTreeMap<String, ResolvedToken>,
    theme_values: &BTreeMap<String, ResolvedToken>,
    diagnostics: &mut Vec<Diagnostic>,
    why: &str,
) -> bool {
    let mut any = false;
    for name in names {
        let Some(pointer) = overridden.remove(*name) else {
            continue;
        };
        match theme_values.get(*name) {
            Some(value) => tokens.insert((*name).to_owned(), value.clone()),
            None => tokens.remove(*name),
        };
        diagnostics.push(Diagnostic::new(
            ResolveCode::TokenLowContrast,
            pointer,
            None,
            format!("override of {name} reverted: {why}"),
        ));
        any = true;
    }
    any
}

fn resolve_wallpaper(
    profile: &Profile,
    base: Option<&ThemeInput>,
    default_theme: &ThemeInput,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ResolvedWallpaper> {
    match &profile.wallpaper {
        Some(Wallpaper::UserFile { path }) => {
            return Some(ResolvedWallpaper {
                source: WallpaperSource::UserFile { path: path.clone() },
                layer: Layer::User,
            });
        }
        Some(Wallpaper::ThemeAsset { name }) => {
            if let Some(chosen) = base.and_then(|theme| asset(theme, name, Layer::User)) {
                return Some(chosen);
            }
            let why = match base {
                Some(theme) => format!("{} has no asset named {name:?}", theme.label()),
                None => "no base theme is available".to_owned(),
            };
            diagnostics.push(Diagnostic::new(
                ResolveCode::WallpaperAssetMissing,
                "/wallpaper",
                None,
                format!("wallpaper asset ignored: {why}"),
            ));
        }
        _ => {}
    }
    base.and_then(|theme| asset(theme, WALLPAPER_ASSET, Layer::BaseTheme))
        .or_else(|| asset(default_theme, WALLPAPER_ASSET, Layer::Default))
}

fn asset(theme: &ThemeInput, name: &str, layer: Layer) -> Option<ResolvedWallpaper> {
    theme
        .manifest
        .assets
        .get(name)
        .map(|path| ResolvedWallpaper {
            source: WallpaperSource::ThemeAsset {
                theme: theme.release.clone(),
                name: name.to_owned(),
                path: path.clone(),
            },
            layer,
        })
}

fn apply_monitors(
    profile: &Profile,
    surfaces: &mut BTreeMap<String, ResolvedSurface>,
    raw: &RawSurfaces<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (monitor, overrides) in &profile.monitors {
        let monitor_pointer = child_pointer("/monitors", monitor);
        for (id, object) in overrides {
            let pointer = child_pointer(&monitor_pointer, id);
            let (Some(surface), Some(theme_surface)) = (surfaces.get_mut(id), raw.get(id)) else {
                diagnostics.push(Diagnostic::new(
                    ResolveCode::MonitorSurfaceUnknown,
                    pointer,
                    Some(id),
                    format!("{id} is not in the resolved plan; the override is ignored"),
                ));
                continue;
            };
            let mut placement_keys = object.clone();
            let mut visible = true;
            let mut changed = false;
            if let Some(value) = placement_keys.remove("visible") {
                match value {
                    Value::Bool(flag) => {
                        visible = flag;
                        changed = true;
                    }
                    _ => diagnostics.push(Diagnostic::new(
                        ResolveCode::MonitorOverrideInvalid,
                        child_pointer(&pointer, "visible"),
                        Some(id),
                        "`visible` must be true or false; ignored",
                    )),
                }
            }
            let mut placement = surface.definition.placement.clone();
            if !placement_keys.is_empty() {
                match refine_placement(id, theme_surface, placement_keys) {
                    Ok(refined) => {
                        placement = refined.placement;
                        changed = true;
                    }
                    Err(err) => {
                        let prefix = child_pointer("/surfaces", id);
                        let at = err
                            .pointer()
                            .strip_prefix(&prefix)
                            .and_then(|rest| rest.strip_prefix("/placement"))
                            .map_or_else(|| pointer.clone(), |rest| format!("{pointer}{rest}"));
                        diagnostics.push(Diagnostic::new(
                            ResolveCode::MonitorOverrideInvalid,
                            at,
                            Some(id),
                            format!(
                                "placement override ignored: {} {}",
                                err.code().as_str(),
                                err.message()
                            ),
                        ));
                    }
                }
            }
            let layer = if changed {
                Layer::Monitor
            } else {
                surface.layer
            };
            surface.monitors.insert(
                monitor.clone(),
                MonitorOverride {
                    visible,
                    placement,
                    layer,
                },
            );
        }
    }
}

/// The theme's own placement with the monitor's keys laid over it,
/// validated (with the theme's behaviour) by `glint-surfaces`.
fn refine_placement(
    id: &str,
    theme_surface: &Surface,
    keys: Map<String, Value>,
) -> Result<SurfaceDefinition, glint_surfaces::SurfaceError> {
    let mut placement = theme_surface.placement.clone().unwrap_or_default();
    placement.extend(keys);
    let mut object = Map::new();
    object.insert("entry".into(), Value::String(theme_surface.entry.clone()));
    object.insert("placement".into(), Value::Object(placement));
    if let Some(behavior) = &theme_surface.behavior {
        object.insert("behavior".into(), Value::Object(behavior.clone()));
    }
    SurfaceDefinition::parse(id, &Value::Object(object))
}
