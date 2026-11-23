//! The portable surface catalog: stable surface kinds, their capability IDs
//! and their placement and behaviour defaults.
//!
//! Every row is portable: nothing here depends on an operating system. Which
//! placements a given host can actually realise is a host capability reported
//! by the surface backend (SRF-002), never assumed from this table.

use std::fmt;

use serde::{Serialize, Serializer};

use crate::behavior::{DismissTrigger, FocusPolicy};
use crate::placement::{Anchor, PlacementKind};

/// Prefix of author-defined surface IDs (`custom:<namespace>.<name>`).
pub const CUSTOM_PREFIX: &str = "custom:";

/// Capability every custom surface requires (`surface.custom`).
pub const CUSTOM_SURFACE_CAPABILITY: &str = "surface.custom";

/// Longest accepted surface ID, in bytes (`custom:` plus a 128-byte namespace).
pub const MAX_SURFACE_ID_LEN: usize = CUSTOM_PREFIX.len() + 128;

/// The fourteen standard surface kinds from the plan's surface contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum StandardSurface {
    /// `panel`: pinned desktop toolbar.
    Panel,
    /// `dock`: running and pinned apps.
    Dock,
    /// `launcher`: searchable app picker.
    Launcher,
    /// `control-center`: quick settings.
    ControlCenter,
    /// `notification`: toasts for Glint-visible events.
    Notification,
    /// `notification-center`: notification history inside Glint.
    NotificationCenter,
    /// `desktop`: background canvas and widgets.
    Desktop,
    /// `calendar`: date and time popup.
    Calendar,
    /// `power-menu`: sleep and shutdown options.
    PowerMenu,
    /// `workspace-switcher`: workspace UI.
    WorkspaceSwitcher,
    /// `window-switcher`: window selection overlay.
    WindowSwitcher,
    /// `volume-osd`: volume feedback.
    VolumeOsd,
    /// `brightness-osd`: brightness feedback.
    BrightnessOsd,
    /// `media-overlay`: track metadata and playback.
    MediaOverlay,
}

impl StandardSurface {
    /// Every standard kind, in catalog order.
    pub const ALL: &'static [StandardSurface] = &[
        StandardSurface::Panel,
        StandardSurface::Dock,
        StandardSurface::Launcher,
        StandardSurface::ControlCenter,
        StandardSurface::Notification,
        StandardSurface::NotificationCenter,
        StandardSurface::Desktop,
        StandardSurface::Calendar,
        StandardSurface::PowerMenu,
        StandardSurface::WorkspaceSwitcher,
        StandardSurface::WindowSwitcher,
        StandardSurface::VolumeOsd,
        StandardSurface::BrightnessOsd,
        StandardSurface::MediaOverlay,
    ];

    /// The stable surface ID, e.g. `control-center`.
    pub const fn as_str(self) -> &'static str {
        self.entry().id
    }

    /// Looks up a standard kind by its ID.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == id)
    }

    /// This kind's catalog row.
    pub const fn entry(self) -> &'static CatalogEntry {
        &CATALOG[self as usize]
    }
}

impl fmt::Display for StandardSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A validated author-defined surface ID, `custom:<namespace>.<name>`.
///
/// The part after `custom:` follows the package ID rules: at least two
/// dot-separated segments of lowercase `a-z`, `0-9` and inner hyphens, at
/// most 128 bytes. An unnamespaced ID such as `custom:clock` is refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomSurfaceId(String);

impl CustomSurfaceId {
    /// Validates a full `custom:` ID. Returns `None` when it is not one.
    pub fn parse(id: &str) -> Option<Self> {
        let rest = id.strip_prefix(CUSTOM_PREFIX)?;
        is_namespaced_id(rest).then(|| Self(id.to_owned()))
    }

    /// The full ID, including the `custom:` prefix.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The namespaced part after `custom:`, e.g. `dev.example.clock`.
    pub fn namespace(&self) -> &str {
        &self.0[CUSTOM_PREFIX.len()..]
    }
}

impl fmt::Display for CustomSurfaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The kind of a surface: one of the standard kinds, or an author-defined
/// `custom:` surface. Serializes as its ID string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SurfaceKind {
    /// A standard kind from the catalog.
    Standard(StandardSurface),
    /// An author-defined, namespaced surface.
    Custom(CustomSurfaceId),
}

impl SurfaceKind {
    /// The surface ID string (`panel`, `custom:dev.example.clock`).
    pub fn as_str(&self) -> &str {
        match self {
            SurfaceKind::Standard(kind) => kind.as_str(),
            SurfaceKind::Custom(id) => id.as_str(),
        }
    }

    /// The catalog row for a standard kind; `None` for custom surfaces.
    pub fn catalog_entry(&self) -> Option<&'static CatalogEntry> {
        match self {
            SurfaceKind::Standard(kind) => Some(kind.entry()),
            SurfaceKind::Custom(_) => None,
        }
    }

    /// The capability the host must offer to show this surface at all:
    /// `surface.<kind>` for standard kinds, `surface.custom` otherwise.
    pub fn capability(&self) -> &'static str {
        match self {
            SurfaceKind::Standard(kind) => kind.entry().capability,
            SurfaceKind::Custom(_) => CUSTOM_SURFACE_CAPABILITY,
        }
    }

    /// Whether `placement` is allowed for this kind. Custom surfaces may use
    /// any placement kind.
    pub fn allows(&self, placement: PlacementKind) -> bool {
        match self {
            SurfaceKind::Standard(kind) => kind.entry().placements.contains(&placement),
            SurfaceKind::Custom(_) => true,
        }
    }
}

impl fmt::Display for SurfaceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for SurfaceKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// One row of the standard surface catalog.
///
/// Capability semantics: the host must offer [`CatalogEntry::capability`]
/// or the surface cannot be shown (the profile falls back, SRF-004). Each of
/// [`CatalogEntry::optional_capabilities`] only enables controls when usable;
/// without it the surface still shows with those controls disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct CatalogEntry {
    /// Stable surface ID.
    pub id: &'static str,
    /// Required capability, always `surface.<id>`.
    pub capability: &'static str,
    /// Provider capabilities the surface's role uses when available.
    pub optional_capabilities: &'static [&'static str],
    /// Allowed placement kinds; the first is the default.
    pub placements: &'static [PlacementKind],
    /// Anchor used when the default placement kind is used without one.
    pub default_anchor: Option<Anchor>,
    /// Focus policy used when `behavior.focus` is omitted.
    pub default_focus: FocusPolicy,
    /// Dismissal triggers used when `behavior.dismissOn` is omitted.
    pub default_dismiss: &'static [DismissTrigger],
    /// Auto-hide delay used when `behavior.autoHideMs` is omitted.
    pub default_auto_hide_ms: Option<u32>,
    /// One-line role, from the plan's surface contract.
    pub summary: &'static str,
}

impl CatalogEntry {
    /// The placement kind used when `placement.kind` is omitted.
    pub fn default_placement(&self) -> PlacementKind {
        self.placements[0]
    }
}

use DismissTrigger::{Escape, OutsideClick};
use PlacementKind::{Desktop as DesktopLayer, Edge, Floating, Overlay, Popup};

const LIGHT_DISMISS: &[DismissTrigger] = &[Escape, OutsideClick];

/// The standard surface catalog, in [`StandardSurface::ALL`] order.
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "panel",
        capability: "surface.panel",
        optional_capabilities: &[],
        placements: &[Edge, Floating],
        default_anchor: Some(Anchor::Top),
        default_focus: FocusPolicy::NonActivating,
        default_dismiss: &[],
        default_auto_hide_ms: None,
        summary: "Pinned desktop toolbar",
    },
    CatalogEntry {
        id: "dock",
        capability: "surface.dock",
        optional_capabilities: &["desktop.windows.list", "desktop.windows.focus"],
        placements: &[Floating, Edge],
        default_anchor: Some(Anchor::Bottom),
        default_focus: FocusPolicy::NonActivating,
        default_dismiss: &[],
        default_auto_hide_ms: None,
        summary: "Running and pinned apps",
    },
    CatalogEntry {
        id: "launcher",
        capability: "surface.launcher",
        optional_capabilities: &[],
        placements: &[Floating, Overlay, Popup],
        default_anchor: Some(Anchor::Center),
        default_focus: FocusPolicy::Exclusive,
        default_dismiss: LIGHT_DISMISS,
        default_auto_hide_ms: None,
        summary: "Searchable app picker",
    },
    CatalogEntry {
        id: "control-center",
        capability: "surface.control-center",
        optional_capabilities: &[
            "system.audio.read",
            "system.audio.write",
            "system.brightness.read",
            "system.brightness.write",
            "system.network.read",
            "system.network.toggle",
            "system.bluetooth.read",
            "system.bluetooth.toggle",
        ],
        placements: &[Popup, Overlay, Floating],
        default_anchor: Some(Anchor::TopRight),
        default_focus: FocusPolicy::OnDemand,
        default_dismiss: LIGHT_DISMISS,
        default_auto_hide_ms: None,
        summary: "Quick settings",
    },
    CatalogEntry {
        id: "notification",
        capability: "surface.notification",
        optional_capabilities: &[],
        placements: &[Overlay],
        default_anchor: Some(Anchor::TopRight),
        default_focus: FocusPolicy::None,
        default_dismiss: &[],
        default_auto_hide_ms: Some(5000),
        summary: "Toasts for Glint-visible events",
    },
    CatalogEntry {
        id: "notification-center",
        capability: "surface.notification-center",
        optional_capabilities: &[],
        placements: &[Popup, Overlay],
        default_anchor: Some(Anchor::TopRight),
        default_focus: FocusPolicy::OnDemand,
        default_dismiss: LIGHT_DISMISS,
        default_auto_hide_ms: None,
        summary: "Notification history inside Glint",
    },
    CatalogEntry {
        id: "desktop",
        capability: "surface.desktop",
        optional_capabilities: &[],
        placements: &[DesktopLayer],
        default_anchor: None,
        default_focus: FocusPolicy::None,
        default_dismiss: &[],
        default_auto_hide_ms: None,
        summary: "Background canvas and widgets",
    },
    CatalogEntry {
        id: "calendar",
        capability: "surface.calendar",
        optional_capabilities: &[],
        placements: &[Popup, Overlay],
        default_anchor: Some(Anchor::TopRight),
        default_focus: FocusPolicy::OnDemand,
        default_dismiss: LIGHT_DISMISS,
        default_auto_hide_ms: None,
        summary: "Date and time popup",
    },
    CatalogEntry {
        id: "power-menu",
        capability: "surface.power-menu",
        optional_capabilities: &["system.power.sleep", "system.power.shutdown"],
        placements: &[Overlay, Popup, Floating],
        default_anchor: Some(Anchor::Center),
        default_focus: FocusPolicy::Exclusive,
        default_dismiss: LIGHT_DISMISS,
        default_auto_hide_ms: None,
        summary: "Sleep and shutdown options",
    },
    CatalogEntry {
        id: "workspace-switcher",
        capability: "surface.workspace-switcher",
        optional_capabilities: &["desktop.workspaces.read", "desktop.workspaces.switch"],
        placements: &[Overlay, Popup, Floating],
        default_anchor: Some(Anchor::Center),
        default_focus: FocusPolicy::Exclusive,
        default_dismiss: LIGHT_DISMISS,
        default_auto_hide_ms: None,
        summary: "Workspace UI",
    },
    CatalogEntry {
        id: "window-switcher",
        capability: "surface.window-switcher",
        optional_capabilities: &["desktop.windows.list", "desktop.windows.focus"],
        placements: &[Overlay],
        default_anchor: Some(Anchor::Center),
        default_focus: FocusPolicy::Exclusive,
        default_dismiss: LIGHT_DISMISS,
        default_auto_hide_ms: None,
        summary: "Window selection overlay",
    },
    CatalogEntry {
        id: "volume-osd",
        capability: "surface.volume-osd",
        optional_capabilities: &["system.audio.read"],
        placements: &[Overlay],
        default_anchor: Some(Anchor::Bottom),
        default_focus: FocusPolicy::None,
        default_dismiss: &[],
        default_auto_hide_ms: Some(1500),
        summary: "Volume feedback",
    },
    CatalogEntry {
        id: "brightness-osd",
        capability: "surface.brightness-osd",
        optional_capabilities: &["system.brightness.read"],
        placements: &[Overlay],
        default_anchor: Some(Anchor::Bottom),
        default_focus: FocusPolicy::None,
        default_dismiss: &[],
        default_auto_hide_ms: Some(1500),
        summary: "Brightness feedback",
    },
    CatalogEntry {
        id: "media-overlay",
        capability: "surface.media-overlay",
        optional_capabilities: &["system.media.read", "system.media.control"],
        placements: &[Overlay, Popup, Floating],
        default_anchor: Some(Anchor::TopRight),
        default_focus: FocusPolicy::OnDemand,
        default_dismiss: LIGHT_DISMISS,
        default_auto_hide_ms: None,
        summary: "Track metadata and playback",
    },
];

/// Whether `name` is one lowercase segment: `a-z0-9` with inner single
/// hyphens, starting with a letter, at most 64 bytes. This is the manifest's
/// key rule; well-formed IDs that are not in the catalog are unknown kinds,
/// everything else is an invalid ID.
pub(crate) fn is_name(name: &str) -> bool {
    name.len() <= 64
        && name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && is_id_segment(name)
}

fn is_id_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.starts_with('-')
        && !segment.ends_with('-')
        && !segment.contains("--")
        && segment
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The package ID rule used for custom namespaces: two or more segments,
/// at most 128 bytes.
fn is_namespaced_id(id: &str) -> bool {
    id.len() <= 128 && id.split('.').count() >= 2 && id.split('.').all(is_id_segment)
}
