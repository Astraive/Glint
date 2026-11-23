//! The *proposed* `placement` object of a surface definition: placement
//! kind, anchor, monitor target, size constraints and margin.
//!
//! This is a request, not a promise: the layout pipeline (SRF-003) resolves
//! it against real monitors and work areas, and a host that cannot realise a
//! placement reports a capability error (SRF-002). Exact z-order is never
//! promised; the layer follows from the placement kind.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::catalog::SurfaceKind;
use crate::error::{SurfaceError, SurfaceErrorCode};
use crate::json::{Obj, Result, conflict, vocabulary};

/// Largest accepted size, in logical (device-independent) pixels.
pub const MAX_LOGICAL_PX: u32 = 16_384;

/// Largest accepted margin, in logical pixels.
pub const MAX_MARGIN_PX: u32 = 1_024;

/// Every key the `placement` object accepts; anything else is
/// `GLINT_SURFACE_UNKNOWN_FIELD`.
pub const PLACEMENT_FIELDS: &[&str] = &[
    "kind",
    "anchor",
    "monitor",
    "width",
    "height",
    "minWidth",
    "maxWidth",
    "minHeight",
    "maxHeight",
    "margin",
];

vocab! {
    /// How a surface is placed, from the plan's placement semantics. The
    /// layer is implied: `desktop` below normal windows, `edge`/`floating`
    /// with normal or topmost windows as the host allows, `overlay` and
    /// `popup` above normal windows as allowed.
    PlacementKind {
        /// `edge`: attached to one monitor edge; may reserve work area.
        Edge => "edge",
        /// `floating`: positioned at an anchor inside the safe bounds.
        Floating => "floating",
        /// `desktop`: fills the monitor below normal windows where supported.
        Desktop => "desktop",
        /// `overlay`: above normal windows as allowed; focusable or not.
        Overlay => "overlay",
        /// `popup`: transient, anchored to its trigger or an anchor point.
        Popup => "popup",
    }
}

vocab! {
    /// A nine-point anchor on the target monitor's work area. `edge`
    /// placements accept only `top`, `bottom`, `left` and `right`.
    Anchor {
        /// `top-left`
        TopLeft => "top-left",
        /// `top`
        Top => "top",
        /// `top-right`
        TopRight => "top-right",
        /// `left`
        Left => "left",
        /// `center`
        Center => "center",
        /// `right`
        Right => "right",
        /// `bottom-left`
        BottomLeft => "bottom-left",
        /// `bottom`
        Bottom => "bottom",
        /// `bottom-right`
        BottomRight => "bottom-right",
    }
}

impl Anchor {
    /// Anchors an `edge` placement may use.
    pub const EDGES: &'static [Anchor] =
        &[Anchor::Top, Anchor::Bottom, Anchor::Left, Anchor::Right];

    /// Whether this anchor names a monitor edge.
    pub fn is_edge(self) -> bool {
        Self::EDGES.contains(&self)
    }

    /// Whether this is the top or bottom edge (the surface spans the width).
    fn is_horizontal_edge(self) -> bool {
        matches!(self, Anchor::Top | Anchor::Bottom)
    }
}

vocab! {
    /// Which monitor(s) a surface targets. Specific monitors are chosen by
    /// the user's profile (stable, privacy-safe monitor keys), never by the
    /// theme; an unknown monitor falls back to the primary one (SRF-003).
    MonitorTarget {
        /// `primary`: the primary monitor (default).
        Primary => "primary",
        /// `active`: the monitor with the focused window or pointer.
        Active => "active",
        /// `all`: one instance per monitor.
        All => "all",
    }
}

/// A validated placement request with defaults applied.
///
/// Sizes are logical pixels. For `edge` placements the thickness (`height`
/// on a top/bottom edge, `width` on a left/right edge) is set and the
/// surface spans the edge; for `desktop` no size, anchor or margin is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SurfacePlacement {
    /// Placement kind.
    pub kind: PlacementKind,
    /// Anchor; `None` only for `desktop`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    /// Monitor target.
    pub monitor: MonitorTarget,
    /// Requested width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Requested height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Minimum width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_width: Option<u32>,
    /// Maximum width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u32>,
    /// Minimum height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_height: Option<u32>,
    /// Maximum height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_height: Option<u32>,
    /// Gap between the surface and its anchor edge(s).
    pub margin: u32,
}

/// Parses `placement` (absent means "all defaults") for a surface of `kind`.
/// `pointer` is the pointer of the `placement` member itself.
pub(crate) fn parse(
    kind: &SurfaceKind,
    value: Option<&Value>,
    pointer: &str,
) -> Result<SurfacePlacement> {
    let empty = Value::Object(Map::new());
    let obj = Obj::new(value.unwrap_or(&empty), pointer.to_owned())?;
    obj.deny_unknown(PLACEMENT_FIELDS)?;

    let entry = kind.catalog_entry();
    let placement = match obj.optional_str("kind")? {
        Some(text) => {
            let placement = vocabulary(&obj.child("kind"), text, PlacementKind::WORDS)?;
            if !kind.allows(placement) {
                return Err(SurfaceError::new(
                    SurfaceErrorCode::UnsupportedPlacement,
                    obj.child("kind"),
                    format!(
                        "`{kind}` surfaces cannot use `{placement}` placement; allowed: {}",
                        allowed_list(kind)
                    ),
                ));
            }
            placement
        }
        None => match entry {
            Some(entry) => entry.default_placement(),
            None => {
                return Err(SurfaceError::new(
                    SurfaceErrorCode::MissingField,
                    obj.child("kind"),
                    "custom surfaces must declare `placement.kind`",
                ));
            }
        },
    };
    let explicit_anchor = obj
        .optional_str("anchor")?
        .map(|text| vocabulary(&obj.child("anchor"), text, Anchor::WORDS))
        .transpose()?;
    let monitor = obj
        .optional_str("monitor")?
        .map(|text| vocabulary(&obj.child("monitor"), text, MonitorTarget::WORDS))
        .transpose()?
        .unwrap_or(MonitorTarget::Primary);
    let size = |key| obj.optional_int(key, 1, MAX_LOGICAL_PX);
    let mut out = SurfacePlacement {
        kind: placement,
        anchor: None,
        monitor,
        width: size("width")?,
        height: size("height")?,
        min_width: size("minWidth")?,
        max_width: size("maxWidth")?,
        min_height: size("minHeight")?,
        max_height: size("maxHeight")?,
        margin: obj.optional_int("margin", 0, MAX_MARGIN_PX)?.unwrap_or(0),
    };
    // The catalog's default anchor belongs to its default placement kind.
    let default_anchor = entry
        .filter(|entry| entry.default_placement() == placement)
        .and_then(|entry| entry.default_anchor);

    match placement {
        PlacementKind::Desktop => {
            for key in [
                "anchor",
                "width",
                "height",
                "minWidth",
                "maxWidth",
                "minHeight",
                "maxHeight",
                "margin",
            ] {
                if obj.get(key).is_some() {
                    return Err(conflict(
                        obj.child(key),
                        format!("`desktop` placement fills the monitor; remove `{key}`"),
                    ));
                }
            }
        }
        PlacementKind::Edge => {
            let anchor = match explicit_anchor.or(default_anchor) {
                Some(anchor) if anchor.is_edge() => anchor,
                Some(anchor) => {
                    return Err(SurfaceError::new(
                        SurfaceErrorCode::InvalidValue,
                        obj.child("anchor"),
                        format!(
                            "`edge` placement anchors to top, bottom, left or right, not {:?}",
                            anchor.as_str()
                        ),
                    ));
                }
                None => {
                    return Err(SurfaceError::new(
                        SurfaceErrorCode::MissingField,
                        obj.child("anchor"),
                        "`edge` placement needs an `anchor`: top, bottom, left or right",
                    ));
                }
            };
            let (thickness, along) = if anchor.is_horizontal_edge() {
                ("height", "width")
            } else {
                ("width", "height")
            };
            for key in [along, "minWidth", "maxWidth", "minHeight", "maxHeight"] {
                if obj.get(key).is_some() {
                    return Err(conflict(
                        obj.child(key),
                        format!(
                            "a `{anchor}` edge surface spans the edge with a fixed `{thickness}`; \
                             remove `{key}`"
                        ),
                    ));
                }
            }
            if obj.get(thickness).is_none() {
                let Some(default) = entry.and_then(|entry| entry.default_thickness) else {
                    return Err(SurfaceError::new(
                        SurfaceErrorCode::MissingField,
                        obj.child(thickness),
                        format!("a `{anchor}` edge surface needs its thickness as `{thickness}`"),
                    ));
                };
                if anchor.is_horizontal_edge() {
                    out.height = Some(default);
                } else {
                    out.width = Some(default);
                }
            }
            out.anchor = Some(anchor);
        }
        PlacementKind::Floating | PlacementKind::Overlay | PlacementKind::Popup => {
            out.anchor = Some(explicit_anchor.or(default_anchor).unwrap_or(Anchor::Center));
            check_axis(&obj, "width", out.width, out.min_width, out.max_width)?;
            check_axis(&obj, "height", out.height, out.min_height, out.max_height)?;
            if placement == PlacementKind::Popup && monitor == MonitorTarget::All {
                return Err(conflict(
                    obj.child("monitor"),
                    "a `popup` is one transient window; use `primary` or `active`",
                ));
            }
        }
    }
    Ok(out)
}

/// `min <= max`, and the requested size inside `[min, max]`.
fn check_axis(
    obj: &Obj<'_>,
    axis: &str,
    size: Option<u32>,
    min: Option<u32>,
    max: Option<u32>,
) -> Result<()> {
    let (min_key, max_key) = if axis == "width" {
        ("minWidth", "maxWidth")
    } else {
        ("minHeight", "maxHeight")
    };
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        return Err(conflict(
            obj.child(max_key),
            format!("`{max_key}` ({max}) is below `{min_key}` ({min})"),
        ));
    }
    if let Some(size) = size
        && (min.is_some_and(|min| size < min) || max.is_some_and(|max| size > max))
    {
        return Err(conflict(
            obj.child(axis),
            format!("`{axis}` ({size}) is outside `{min_key}`..`{max_key}`"),
        ));
    }
    Ok(())
}

fn allowed_list(kind: &SurfaceKind) -> String {
    PlacementKind::ALL
        .iter()
        .filter(|placement| kind.allows(**placement))
        .map(|placement| placement.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
