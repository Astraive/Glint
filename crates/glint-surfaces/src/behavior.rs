//! The *proposed* `behavior` object of a surface definition: focus policy,
//! work-area reservation, dismissal, auto-hide and click-through.
//!
//! Like placement, behaviour is a request. Focus rules differ per OS (focus
//! theft prevention on Windows, layer-shell keyboard interactivity on
//! Wayland); a host that cannot honour a policy reports it as a capability
//! error rather than silently changing it (SRF-002).

use serde::Serialize;
use serde_json::{Map, Value};

use crate::catalog::SurfaceKind;
use crate::error::{SurfaceError, SurfaceErrorCode};
use crate::json::{Obj, Result, conflict, type_error, vocabulary};
use crate::placement::{PlacementKind, SurfacePlacement};

/// Shortest accepted auto-hide delay, in milliseconds.
pub const MIN_AUTO_HIDE_MS: u32 = 250;

/// Longest accepted auto-hide delay, in milliseconds.
pub const MAX_AUTO_HIDE_MS: u32 = 60_000;

/// Every key the `behavior` object accepts; anything else is
/// `GLINT_SURFACE_UNKNOWN_FIELD`.
pub const BEHAVIOR_FIELDS: &[&str] = &[
    "focus",
    "reserveWorkArea",
    "dismissOn",
    "autoHideMs",
    "clickThrough",
];

vocab! {
    /// Keyboard focus policy.
    FocusPolicy {
        /// `none`: never takes keyboard focus (OSDs, toasts, desktop).
        None => "none",
        /// `non-activating`: takes pointer input without activating or
        /// stealing focus from the foreground window (panels, docks).
        NonActivating => "non-activating",
        /// `on-demand`: takes keyboard focus when the user interacts with it.
        OnDemand => "on-demand",
        /// `exclusive`: takes keyboard focus while shown (launchers, menus).
        Exclusive => "exclusive",
    }
}

vocab! {
    /// What dismisses a transient surface.
    DismissTrigger {
        /// `escape`: the Escape key.
        Escape => "escape",
        /// `outside-click`: a pointer press outside the surface.
        OutsideClick => "outside-click",
    }
}

/// A validated behaviour request with defaults applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SurfaceBehavior {
    /// Keyboard focus policy.
    pub focus: FocusPolicy,
    /// Reserve the surface's thickness from the monitor work area (edge
    /// placements only; realised with an appbar or layer-shell exclusive
    /// zone where the host offers one).
    pub reserve_work_area: bool,
    /// Dismissal triggers, in declaration order.
    pub dismiss_on: Vec<DismissTrigger>,
    /// Auto-hide delay after the last update (overlay and popup only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_hide_ms: Option<u32>,
    /// Let pointer input pass through (desktop and overlay only, focus `none`).
    pub click_through: bool,
}

/// Parses `behavior` (absent means "all defaults") for a surface of `kind`
/// placed as `placement`. `pointer` is the pointer of the `behavior` member.
pub(crate) fn parse(
    kind: &SurfaceKind,
    placement: &SurfacePlacement,
    value: Option<&Value>,
    pointer: &str,
) -> Result<SurfaceBehavior> {
    let empty = Value::Object(Map::new());
    let obj = Obj::new(value.unwrap_or(&empty), pointer.to_owned())?;
    obj.deny_unknown(BEHAVIOR_FIELDS)?;
    let entry = kind.catalog_entry();
    let at = placement.kind;
    let transient = matches!(at, PlacementKind::Overlay | PlacementKind::Popup);

    let focus = match obj.optional_str("focus")? {
        Some(text) => vocabulary(&obj.child("focus"), text, FocusPolicy::WORDS)?,
        None => match entry {
            Some(entry) => entry.default_focus,
            None => {
                return Err(SurfaceError::new(
                    SurfaceErrorCode::MissingField,
                    obj.child("focus"),
                    "custom surfaces must declare `behavior.focus`",
                ));
            }
        },
    };
    if at == PlacementKind::Desktop && focus == FocusPolicy::Exclusive {
        return Err(conflict(
            obj.child("focus"),
            "a `desktop` surface sits below windows and cannot take exclusive focus",
        ));
    }

    let reserve_work_area = obj.optional_bool("reserveWorkArea")?.unwrap_or(false);
    if reserve_work_area && at != PlacementKind::Edge {
        return Err(conflict(
            obj.child("reserveWorkArea"),
            format!("only `edge` placements can reserve work area, not `{at}`"),
        ));
    }

    let dismiss_on = match obj.get("dismissOn") {
        Some(value) => {
            if matches!(at, PlacementKind::Edge | PlacementKind::Desktop) {
                return Err(conflict(
                    obj.child("dismissOn"),
                    format!("`{at}` surfaces are persistent and cannot be dismissed"),
                ));
            }
            dismiss_triggers(&obj.child("dismissOn"), value)?
        }
        None if matches!(at, PlacementKind::Edge | PlacementKind::Desktop) => Vec::new(),
        None => entry.map_or_else(Vec::new, |entry| entry.default_dismiss.to_vec()),
    };

    let auto_hide_ms = match obj.optional_int("autoHideMs", MIN_AUTO_HIDE_MS, MAX_AUTO_HIDE_MS)? {
        Some(_) if !transient => {
            return Err(conflict(
                obj.child("autoHideMs"),
                format!("only `overlay` and `popup` surfaces auto-hide, not `{at}`"),
            ));
        }
        Some(ms) => Some(ms),
        None if transient => entry.and_then(|entry| entry.default_auto_hide_ms),
        None => None,
    };

    let click_through = obj.optional_bool("clickThrough")?.unwrap_or(false);
    if click_through {
        if !matches!(at, PlacementKind::Desktop | PlacementKind::Overlay) {
            return Err(conflict(
                obj.child("clickThrough"),
                format!("only `desktop` and `overlay` surfaces can be click-through, not `{at}`"),
            ));
        }
        if focus != FocusPolicy::None {
            return Err(conflict(
                obj.child("clickThrough"),
                format!(
                    "a click-through surface cannot take focus; set `focus` to `none`, not `{focus}`"
                ),
            ));
        }
    }

    Ok(SurfaceBehavior {
        focus,
        reserve_work_area,
        dismiss_on,
        auto_hide_ms,
        click_through,
    })
}

fn dismiss_triggers(pointer: &str, value: &Value) -> Result<Vec<DismissTrigger>> {
    let Value::Array(items) = value else {
        return Err(type_error(pointer, "an array of strings", value));
    };
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let item_ptr = format!("{pointer}/{index}");
        let Value::String(text) = item else {
            return Err(type_error(&item_ptr, "a string", item));
        };
        let trigger = vocabulary(&item_ptr, text, DismissTrigger::WORDS)?;
        if out.contains(&trigger) {
            return Err(SurfaceError::new(
                SurfaceErrorCode::InvalidValue,
                item_ptr,
                format!("{text:?} is listed twice"),
            ));
        }
        out.push(trigger);
    }
    Ok(out)
}
