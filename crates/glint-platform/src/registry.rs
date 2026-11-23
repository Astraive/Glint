//! The frozen v1 capability registry (CAP-005).
//!
//! [`REGISTRY`] is the single source of truth for which capability IDs exist
//! in SDK v1. Every other component derives its accepted set from it:
//!
//! - `glint-package` refuses a manifest that declares an unregistered ID
//!   (`GLINT_PACKAGE_UNKNOWN_CAPABILITY`);
//! - `glint-surfaces` catalog IDs are checked against it by tests;
//! - at runtime the [`crate::CapabilityService`] refuses an adapter report
//!   that contains an unregistered ID (`adapter-failure`), a snapshot reports
//!   any unregistered ID as `unknown-capability`, and the activation gate
//!   refuses a theme that declares one, required or optional.
//!
//! [`CapabilityId::parse`] stays a *syntax* check; [`lookup`] and
//! [`require_registered`] answer whether an ID is in the registry.
//!
//! # Freeze rules
//!
//! Version 1 is frozen ([`REGISTRY_VERSION`]). Within v1 an ID is never
//! removed, renamed or re-purposed, and its [`Access`] never changes. New IDs
//! may be **added** through an ADR (see `docs/spec/capabilities-v1.md`,
//! *Adding a capability*); `tests/registry.rs` pins the full ID list so an
//! accidental edit fails CI. Generated outputs (`packages/types/src/
//! capabilities.ts`, the table in `docs/spec/capabilities-v1.md`) are
//! drift-checked against this module.

use std::fmt;
use std::fmt::Write as _;

use crate::capabilities::{CapabilityId, IntegrationEffect, Namespace};
use crate::error::{CapabilityError, CapabilityErrorCode};

/// Version of the frozen capability registry.
pub const REGISTRY_VERSION: u32 = 1;

/// Read/write split: what using a capability does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Access {
    /// Observes state, including change notifications for it. Changes
    /// nothing.
    Read,
    /// Changes state or performs an action (an OS setting, focus, power).
    Write,
    /// Shows Glint-owned UI or uses a host window mechanism. Neither reads
    /// user data nor changes OS settings. Every `surface.*` and `host.*` ID.
    Present,
}

impl Access {
    /// Every access kind, in declaration order.
    pub const ALL: &'static [Access] = &[Access::Read, Access::Write, Access::Present];

    /// Stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Access::Read => "read",
            Access::Write => "write",
            Access::Present => "present",
        }
    }
}

impl fmt::Display for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Privacy and impact class. It informs the permission prompt and the
/// diagnostics redaction policy; it never grants anything by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Sensitivity {
    /// No personal data and no effect outside Glint (surfaces, host
    /// mechanisms, coarse device state such as the volume level).
    Low,
    /// A reversible change to OS state other apps observe (volume,
    /// brightness, focus, playback), or state that hints at user activity
    /// (workspace names, paired device names).
    Moderate,
    /// Personal data (window titles and app names, the Wi-Fi SSID, which
    /// reveals location, media metadata) or a disruptive or persistent change
    /// (sleep, shutdown, radio toggles, OS appearance). Redact in diagnostics.
    High,
}

impl Sensitivity {
    /// Every class, in declaration order.
    pub const ALL: &'static [Sensitivity] =
        &[Sensitivity::Low, Sensitivity::Moderate, Sensitivity::High];

    /// Stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Sensitivity::Low => "low",
            Sensitivity::Moderate => "moderate",
            Sensitivity::High => "high",
        }
    }
}

impl fmt::Display for Sensitivity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One registered capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapabilitySpec {
    /// The capability ID, e.g. `system.audio.write`.
    pub id: &'static str,
    /// Read, write or present.
    pub access: Access,
    /// Privacy and impact class.
    pub sensitivity: Sensitivity,
    /// Whether using it affects Glint UI or an OS-managed setting.
    pub effect: IntegrationEffect,
    /// What the capability covers.
    pub summary: &'static str,
    /// Per-platform notes: what is evidenced on Windows 11 and what is only
    /// planned elsewhere. Availability is always reported by the host
    /// adapter at runtime, never assumed from this text.
    pub platform_notes: &'static str,
}

impl CapabilitySpec {
    /// The ID's namespace.
    pub fn namespace(&self) -> Namespace {
        self.capability_id().namespace()
    }

    /// The ID as a validated [`CapabilityId`].
    pub fn capability_id(&self) -> CapabilityId {
        // Every registry ID is checked by `tests/registry.rs`.
        CapabilityId::parse(self.id).unwrap_or_else(|e| panic!("registry id {}: {e}", self.id))
    }
}

use Access::{Present, Read, Write};
use IntegrationEffect::{GlintOwnedUi as Glint, OsManagedSetting as Os};
use Sensitivity::{High, Low, Moderate};

const SURFACE_NOTE: &str = "Glint-owned window; which placements the host can realise is reported by the surface backend (SRF-002).";

const fn surface(id: &'static str, summary: &'static str) -> CapabilitySpec {
    CapabilitySpec {
        id,
        access: Present,
        sensitivity: Low,
        effect: Glint,
        summary,
        platform_notes: SURFACE_NOTE,
    }
}

const fn spec(
    id: &'static str,
    access: Access,
    sensitivity: Sensitivity,
    effect: IntegrationEffect,
    summary: &'static str,
    platform_notes: &'static str,
) -> CapabilitySpec {
    CapabilitySpec {
        id,
        access,
        sensitivity,
        effect,
        summary,
        platform_notes,
    }
}

/// The frozen v1 registry, grouped by namespace (surface, system, desktop,
/// native, host) and in a fixed order. Append-only within v1.
pub const REGISTRY: &[CapabilitySpec] = &[
    // ---- surface.*: one per standard surface kind, plus custom surfaces ----
    surface(
        "surface.panel",
        "Show the panel surface (pinned desktop toolbar).",
    ),
    surface(
        "surface.dock",
        "Show the dock surface (running and pinned apps).",
    ),
    surface(
        "surface.launcher",
        "Show the launcher surface (searchable app picker).",
    ),
    surface(
        "surface.control-center",
        "Show the Glint control center surface (quick settings); it never replaces the OS one.",
    ),
    surface(
        "surface.notification",
        "Show toast notifications for Glint-visible events.",
    ),
    surface(
        "surface.notification-center",
        "Show the notification history surface inside Glint.",
    ),
    surface(
        "surface.desktop",
        "Show the desktop surface (background canvas and widgets).",
    ),
    surface("surface.calendar", "Show the calendar popup surface."),
    surface(
        "surface.power-menu",
        "Show the power menu surface; the actions need `system.power.*`.",
    ),
    surface(
        "surface.workspace-switcher",
        "Show the workspace switcher surface.",
    ),
    surface(
        "surface.window-switcher",
        "Show the window switcher overlay surface.",
    ),
    surface("surface.volume-osd", "Show the volume on-screen display."),
    surface(
        "surface.brightness-osd",
        "Show the brightness on-screen display.",
    ),
    surface("surface.media-overlay", "Show the media overlay surface."),
    surface(
        "surface.custom",
        "Show author-defined `custom:<namespace>.<name>` surfaces.",
    ),
    // ---- system.*: system state and actions -------------------------------
    spec(
        "system.audio.read",
        Read,
        Low,
        Glint,
        "Read the default output device's volume and mute state, and receive change and device-change events.",
        "Windows: supported (Core Audio endpoint volume, SPK-010 evidence); no device is unavailable. Linux (PipeWire/PulseAudio), macOS: planned.",
    ),
    spec(
        "system.audio.write",
        Write,
        Moderate,
        Os,
        "Set the default output device's volume and mute state.",
        "Windows: supported without a prompt for unpackaged processes (SPK-010); a denied write is permission-required. Linux, macOS: planned.",
    ),
    spec(
        "system.brightness.read",
        Read,
        Low,
        Glint,
        "Read display brightness.",
        "Windows: not yet evidenced; external displays are often hardware-unavailable. Linux, macOS: planned.",
    ),
    spec(
        "system.brightness.write",
        Write,
        Moderate,
        Os,
        "Set display brightness.",
        "Windows: not yet evidenced; external displays are often hardware-unavailable. Linux, macOS: planned.",
    ),
    spec(
        "system.network.read",
        Read,
        Low,
        Glint,
        "Read connectivity level, interface type and cost, and receive change events. Excludes the SSID.",
        "Windows: supported (NLM and NetworkInformation, SPK-010). Linux (NetworkManager), macOS: planned.",
    ),
    spec(
        "system.network.toggle",
        Write,
        High,
        Os,
        "Turn a network radio (Wi-Fi) on or off.",
        "Windows: unavailable in SPK-010 (no toggle path; Windows.Devices.Radios not evaluated). Linux, macOS: planned.",
    ),
    spec(
        "system.network.ssid",
        Read,
        High,
        Glint,
        "Read the connected Wi-Fi SSID. Reveals location; separate from system.network.read and redacted in diagnostics.",
        "Windows: supported when location consent allows it, otherwise permission-required (SPK-010). Linux, macOS: planned.",
    ),
    spec(
        "system.bluetooth.read",
        Read,
        Moderate,
        Glint,
        "Read Bluetooth radio state and paired device names.",
        "Windows: not yet evidenced. Linux (BlueZ), macOS: planned.",
    ),
    spec(
        "system.bluetooth.toggle",
        Write,
        High,
        Os,
        "Turn the Bluetooth radio on or off.",
        "Windows: not yet evidenced. Linux, macOS: planned.",
    ),
    spec(
        "system.battery.read",
        Read,
        Low,
        Glint,
        "Read battery level, charging state and estimates, and receive change events.",
        "Windows: supported (GetSystemPowerStatus, PowerManager, SPK-010); a desktop without a battery is unavailable, not an error. Linux (UPower), macOS: planned.",
    ),
    spec(
        "system.power.source.read",
        Read,
        Low,
        Glint,
        "Read whether the device runs on AC power.",
        "Windows: supported (ACLineStatus, SPK-010), also on desktops without a battery. Linux, macOS: planned.",
    ),
    spec(
        "system.power.sleep",
        Write,
        High,
        Os,
        "Put the device to sleep.",
        "Windows: not yet evidenced. Linux, macOS: planned.",
    ),
    spec(
        "system.power.shutdown",
        Write,
        High,
        Os,
        "Shut down or restart the device.",
        "Windows: not yet evidenced. Linux, macOS: planned.",
    ),
    spec(
        "system.media.read",
        Read,
        High,
        Glint,
        "Read the current media session's track metadata and playback state.",
        "Windows: not yet evidenced (system media transport controls). Linux (MPRIS), macOS: planned.",
    ),
    spec(
        "system.media.control",
        Write,
        Moderate,
        Os,
        "Play, pause and skip in the current media session.",
        "Windows: not yet evidenced. Linux (MPRIS), macOS: planned.",
    ),
    // ---- desktop.*: windows and workspaces --------------------------------
    spec(
        "desktop.windows.list",
        Read,
        High,
        Glint,
        "List top-level windows with titles and app names, and receive window events.",
        "Windows: not yet evidenced. Linux: compositor-dependent. macOS: needs the accessibility permission (permission-required).",
    ),
    spec(
        "desktop.windows.focus",
        Write,
        Moderate,
        Os,
        "Bring a listed window to the foreground.",
        "Windows: not yet evidenced (foreground-lock rules apply). Linux: compositor-dependent. macOS: needs the accessibility permission.",
    ),
    spec(
        "desktop.workspaces.read",
        Read,
        Moderate,
        Glint,
        "List workspaces (virtual desktops) and the active one, and receive change events.",
        "Windows: not yet evidenced (virtual desktops have no public enumeration API). Linux: compositor-dependent. macOS: planned.",
    ),
    spec(
        "desktop.workspaces.switch",
        Write,
        Moderate,
        Os,
        "Switch the active workspace.",
        "Windows: not yet evidenced. Linux: compositor-dependent. macOS: planned.",
    ),
    // ---- native.*: changes to OS-managed appearance -----------------------
    spec(
        "native.wallpaper.set",
        Write,
        High,
        Os,
        "Set the desktop wallpaper per display, with restore.",
        "Windows: planned (IDesktopWallpaper). Linux: desktop-dependent. macOS: planned.",
    ),
    spec(
        "native.system-theme.apply",
        Write,
        High,
        Os,
        "Apply OS light/dark mode or accent colour.",
        "Windows: planned. Linux: desktop-dependent. macOS: planned.",
    ),
    spec(
        "native.taskbar.modify",
        Write,
        High,
        Os,
        "Change supported OS taskbar settings (auto-hide, alignment).",
        "Windows: planned, supported settings only; the native taskbar is never replaced. Linux, macOS: unsupported or desktop-dependent.",
    ),
    spec(
        "native.status-item.create",
        Write,
        Moderate,
        Os,
        "Create a notification-area (tray) or menu-bar status item.",
        "Windows: planned (notification area). Linux (StatusNotifierItem), macOS (menu bar): planned.",
    ),
    // ---- host.*: host window mechanisms -----------------------------------
    spec(
        "host.layer-shell",
        Present,
        Low,
        Glint,
        "Place surfaces with the Wayland layer-shell protocol.",
        "Linux Wayland only, when the compositor advertises it; unsupported-platform on Windows and macOS.",
    ),
    spec(
        "host.appbar",
        Present,
        Low,
        Glint,
        "Reserve a screen edge as an application desktop toolbar.",
        "Windows only (SHAppBarMessage); unsupported-platform elsewhere.",
    ),
    spec(
        "host.desktop-layer",
        Present,
        Low,
        Glint,
        "Place a surface on the desktop layer, below normal windows.",
        "Windows: planned (WorkerW). Linux: layer-shell background or X11 desktop type. macOS: planned.",
    ),
    spec(
        "host.transparent-overlay",
        Present,
        Low,
        Glint,
        "Show transparent, optionally click-through overlay windows.",
        "Windows: planned (layered windows). Linux, macOS: planned.",
    ),
    spec(
        "host.global-hotkey",
        Present,
        Moderate,
        Glint,
        "Register system-wide keyboard shortcuts that open Glint surfaces.",
        "Windows: planned (RegisterHotKey). Linux: portal or compositor-dependent. macOS: planned.",
    ),
];

/// The registry entry for `id`, or `None` when it is not registered.
pub fn lookup(id: &str) -> Option<&'static CapabilitySpec> {
    REGISTRY.iter().find(|spec| spec.id == id)
}

/// Whether `id` is in the frozen registry.
pub fn is_registered(id: &str) -> bool {
    lookup(id).is_some()
}

/// The registry entry for `id`, or an `unknown-capability` error. The error
/// carries the parsed ID when `id` is at least syntactically valid.
pub fn require_registered(id: &str) -> Result<&'static CapabilitySpec, CapabilityError> {
    lookup(id).ok_or_else(|| {
        let (parsed, detail) = match CapabilityId::parse(id) {
            Ok(parsed) => (
                Some(parsed),
                format!("{id:?} is not in the v{REGISTRY_VERSION} capability registry"),
            ),
            Err(e) => (None, format!("{id:?} is not a capability id: {e}")),
        };
        CapabilityError::new(CapabilityErrorCode::UnknownCapability, parsed, detail)
    })
}

/// Every registered ID, in registry order.
pub fn registered_ids() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|spec| spec.id)
}

/// Repository-relative path of the generated TypeScript module.
pub const TYPESCRIPT_PATH: &str = "packages/types/src/capabilities.ts";

/// Repository-relative path of the capability spec whose table is generated
/// by [`markdown_table`].
pub const SPEC_PATH: &str = "docs/spec/capabilities-v1.md";

/// Marker lines around the generated table in [`SPEC_PATH`].
pub const SPEC_TABLE_MARKERS: (&str, &str) = (
    "<!-- BEGIN GENERATED: capability registry (crates/glint-platform/src/registry.rs) -->",
    "<!-- END GENERATED: capability registry -->",
);

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn effect_str(effect: IntegrationEffect) -> &'static str {
    match effect {
        IntegrationEffect::GlintOwnedUi => "glint-owned-ui",
        IntegrationEffect::OsManagedSetting => "os-managed-setting",
    }
}

fn const_union(out: &mut String, doc: &str, constant: &str, type_name: &str, words: &[&str]) {
    writeln!(out, "/** {doc} */").unwrap();
    writeln!(out, "export const {constant} = [").unwrap();
    for word in words {
        writeln!(out, "  {},", quote(word)).unwrap();
    }
    writeln!(out, "] as const;").unwrap();
    writeln!(
        out,
        "export type {type_name} = (typeof {constant})[number];\n"
    )
    .unwrap();
}

/// The full text of [`TYPESCRIPT_PATH`].
pub fn typescript_module() -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED FILE - DO NOT EDIT.\n\
         // Source: crates/glint-platform/src/registry.rs (frozen v1 capability registry, CAP-005).\n\
         // Regenerate: GLINT_UPDATE_TYPES=1 cargo test -p glint-platform --test registry\n\
         // `cargo test` fails when this file differs from the generator output.\n\n",
    );
    writeln!(
        out,
        "/** Version of the frozen capability registry. */\nexport const CAPABILITY_REGISTRY_VERSION = {REGISTRY_VERSION};\n"
    )
    .unwrap();
    let namespaces: Vec<&str> = Namespace::ALL.iter().map(|n| n.as_str()).collect();
    const_union(
        &mut out,
        "Capability namespaces.",
        "CAPABILITY_NAMESPACES",
        "CapabilityNamespace",
        &namespaces,
    );
    let ids: Vec<&str> = registered_ids().collect();
    const_union(
        &mut out,
        "Every v1 capability ID, in registry order. Any other ID is unknown and refused.",
        "CAPABILITY_IDS",
        "CapabilityId",
        &ids,
    );
    let access: Vec<&str> = Access::ALL.iter().map(|a| a.as_str()).collect();
    const_union(
        &mut out,
        "Read/write split: `read` observes, `write` changes state, `present` shows Glint-owned UI or uses a host mechanism.",
        "CAPABILITY_ACCESS",
        "CapabilityAccess",
        &access,
    );
    let sensitivity: Vec<&str> = Sensitivity::ALL.iter().map(|s| s.as_str()).collect();
    const_union(
        &mut out,
        "Privacy and impact classes.",
        "CAPABILITY_SENSITIVITIES",
        "CapabilitySensitivity",
        &sensitivity,
    );
    out.push_str(
        "/** Whether using a capability affects Glint-owned UI or an OS-managed setting. */\n\
         export type IntegrationEffect = \"glint-owned-ui\" | \"os-managed-setting\";\n\n\
         /** One registry entry. */\n\
         export interface CapabilitySpec {\n  \
           namespace: CapabilityNamespace;\n  \
           access: CapabilityAccess;\n  \
           sensitivity: CapabilitySensitivity;\n  \
           effect: IntegrationEffect;\n  \
           summary: string;\n  \
           platformNotes: string;\n\
         }\n\n\
         /** The frozen v1 capability registry. */\n\
         export const CAPABILITY_REGISTRY: { readonly [K in CapabilityId]: CapabilitySpec } = {\n",
    );
    for spec in REGISTRY {
        writeln!(out, "  {}: {{", quote(spec.id)).unwrap();
        writeln!(out, "    namespace: {},", quote(spec.namespace().as_str())).unwrap();
        writeln!(out, "    access: {},", quote(spec.access.as_str())).unwrap();
        writeln!(
            out,
            "    sensitivity: {},",
            quote(spec.sensitivity.as_str())
        )
        .unwrap();
        writeln!(out, "    effect: {},", quote(effect_str(spec.effect))).unwrap();
        writeln!(out, "    summary: {},", quote(spec.summary)).unwrap();
        writeln!(out, "    platformNotes: {},", quote(spec.platform_notes)).unwrap();
        writeln!(out, "  }},").unwrap();
    }
    out.push_str("};\n");
    out
}

/// The generated registry table embedded in [`SPEC_PATH`] between
/// [`SPEC_TABLE_MARKERS`] (markers not included).
pub fn markdown_table() -> String {
    let mut out = String::from(
        "| ID | Namespace | Access | Sensitivity | Effect | Summary | Platform notes |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for spec in REGISTRY {
        writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} | {} |",
            spec.id,
            spec.namespace(),
            spec.access,
            spec.sensitivity,
            effect_str(spec.effect),
            spec.summary.replace('|', "\\|"),
            spec.platform_notes.replace('|', "\\|"),
        )
        .unwrap();
    }
    out
}
