// GENERATED FILE - DO NOT EDIT.
// Source: crates/glint-surfaces (catalog, placement, behavior, error codes). PROPOSED format.
// Regenerate: GLINT_UPDATE_TYPES=1 cargo test -p glint-surfaces --test typegen
// `cargo test` fails when this file differs from the generator output.

import type { CapabilityId } from "./capabilities.ts";

/** Stability label of the surface definition format. */
export const SURFACE_FORMAT_STATUS = "proposed";

/** The standard surface kinds, in catalog order. */
export const STANDARD_SURFACE_KINDS = [
  "panel",
  "dock",
  "launcher",
  "control-center",
  "notification",
  "notification-center",
  "desktop",
  "calendar",
  "power-menu",
  "workspace-switcher",
  "window-switcher",
  "volume-osd",
  "brightness-osd",
  "media-overlay",
] as const;
export type StandardSurfaceKind = (typeof STANDARD_SURFACE_KINDS)[number];

/** Author-defined surface ID: `custom:<namespace>.<name>` (at least two dot-separated segments). */
export type CustomSurfaceId = `custom:${string}.${string}`;

/** A surface ID: a standard kind or a custom ID. */
export type SurfaceKind = StandardSurfaceKind | CustomSurfaceId;

/** Placement kinds. */
export const PLACEMENT_KINDS = [
  "edge",
  "floating",
  "desktop",
  "overlay",
  "popup",
] as const;
export type PlacementKind = (typeof PLACEMENT_KINDS)[number];

/** Nine-point anchors on the target monitor's work area. */
export const ANCHORS = [
  "top-left",
  "top",
  "top-right",
  "left",
  "center",
  "right",
  "bottom-left",
  "bottom",
  "bottom-right",
] as const;
export type Anchor = (typeof ANCHORS)[number];

/** Anchors an `edge` placement accepts. */
export const EDGE_ANCHORS = [
  "top",
  "bottom",
  "left",
  "right",
] as const;
export type EdgeAnchor = (typeof EDGE_ANCHORS)[number];

/** Monitor targets. */
export const MONITOR_TARGETS = [
  "primary",
  "active",
  "all",
] as const;
export type MonitorTarget = (typeof MONITOR_TARGETS)[number];

/** Keyboard focus policies. */
export const FOCUS_POLICIES = [
  "none",
  "non-activating",
  "on-demand",
  "exclusive",
] as const;
export type FocusPolicy = (typeof FOCUS_POLICIES)[number];

/** Dismissal triggers. */
export const DISMISS_TRIGGERS = [
  "escape",
  "outside-click",
] as const;
export type DismissTrigger = (typeof DISMISS_TRIGGERS)[number];

/** Numeric limits enforced by the parser. Sizes are logical pixels. */
export const SURFACE_LIMITS = {
  maxLogicalPx: 16384,
  maxMarginPx: 1024,
  minAutoHideMs: 250,
  maxAutoHideMs: 60000,
  maxEntrypointLength: 512,
  maxSurfaceIdLength: 135,
} as const;

/** `surfaces.<id>.placement` as written in glint.json. */
export interface SurfacePlacementInput {
  kind?: PlacementKind;
  anchor?: Anchor;
  monitor?: MonitorTarget;
  width?: number;
  height?: number;
  minWidth?: number;
  maxWidth?: number;
  minHeight?: number;
  maxHeight?: number;
  margin?: number;
}

/** `surfaces.<id>.behavior` as written in glint.json. */
export interface SurfaceBehaviorInput {
  focus?: FocusPolicy;
  reserveWorkArea?: boolean;
  dismissOn?: DismissTrigger[];
  autoHideMs?: number;
  clickThrough?: boolean;
}

/** `surfaces.<id>` as written in glint.json. */
export interface SurfaceEntryInput {
  entry: string;
  placement?: SurfacePlacementInput;
  behavior?: SurfaceBehaviorInput;
}

/** A validated placement with defaults applied. */
export interface SurfacePlacement {
  kind: PlacementKind;
  anchor?: Anchor;
  monitor: MonitorTarget;
  width?: number;
  height?: number;
  minWidth?: number;
  maxWidth?: number;
  minHeight?: number;
  maxHeight?: number;
  margin: number;
}

/** A validated behaviour with defaults applied. */
export interface SurfaceBehavior {
  focus: FocusPolicy;
  reserveWorkArea: boolean;
  dismissOn: DismissTrigger[];
  autoHideMs?: number;
  clickThrough: boolean;
}

/** Capabilities a surface implies: `required` must all be usable to show it; `optional` only enable controls. */
export interface SurfaceCapabilities {
  required: CapabilityId[];
  optional: CapabilityId[];
}

/** A validated surface, as the host serializes it. */
export interface SurfaceDefinition {
  id: string;
  kind: SurfaceKind;
  entrypoint: string;
  placement: SurfacePlacement;
  behavior: SurfaceBehavior;
  capabilities: SurfaceCapabilities;
}

/** One row of the standard surface catalog. The first placement is the default. */
export interface SurfaceCatalogEntry {
  capability: CapabilityId;
  optionalCapabilities: readonly CapabilityId[];
  placements: readonly PlacementKind[];
  defaultAnchor: Anchor | null;
  defaultFocus: FocusPolicy;
  defaultDismiss: readonly DismissTrigger[];
  defaultAutoHideMs: number | null;
  defaultThickness: number | null;
  summary: string;
}

/** Capability every custom surface requires. */
export const CUSTOM_SURFACE_CAPABILITY = "surface.custom";

/** The standard surface catalog. */
export const SURFACE_CATALOG = {
  panel: {
    capability: "surface.panel",
    optionalCapabilities: [],
    placements: ["edge", "floating"],
    defaultAnchor: "top",
    defaultFocus: "non-activating",
    defaultDismiss: [],
    defaultAutoHideMs: null,
    defaultThickness: 40,
    summary: "Pinned desktop toolbar",
  },
  dock: {
    capability: "surface.dock",
    optionalCapabilities: ["desktop.windows.list", "desktop.windows.focus"],
    placements: ["floating", "edge"],
    defaultAnchor: "bottom",
    defaultFocus: "non-activating",
    defaultDismiss: [],
    defaultAutoHideMs: null,
    defaultThickness: 64,
    summary: "Running and pinned apps",
  },
  launcher: {
    capability: "surface.launcher",
    optionalCapabilities: [],
    placements: ["floating", "overlay", "popup"],
    defaultAnchor: "center",
    defaultFocus: "exclusive",
    defaultDismiss: ["escape", "outside-click"],
    defaultAutoHideMs: null,
    defaultThickness: null,
    summary: "Searchable app picker",
  },
  "control-center": {
    capability: "surface.control-center",
    optionalCapabilities: ["system.audio.read", "system.audio.write", "system.brightness.read", "system.brightness.write", "system.network.read", "system.network.toggle", "system.bluetooth.read", "system.bluetooth.toggle"],
    placements: ["popup", "overlay", "floating"],
    defaultAnchor: "top-right",
    defaultFocus: "on-demand",
    defaultDismiss: ["escape", "outside-click"],
    defaultAutoHideMs: null,
    defaultThickness: null,
    summary: "Quick settings",
  },
  notification: {
    capability: "surface.notification",
    optionalCapabilities: [],
    placements: ["overlay"],
    defaultAnchor: "top-right",
    defaultFocus: "none",
    defaultDismiss: [],
    defaultAutoHideMs: 5000,
    defaultThickness: null,
    summary: "Toasts for Glint-visible events",
  },
  "notification-center": {
    capability: "surface.notification-center",
    optionalCapabilities: [],
    placements: ["popup", "overlay"],
    defaultAnchor: "top-right",
    defaultFocus: "on-demand",
    defaultDismiss: ["escape", "outside-click"],
    defaultAutoHideMs: null,
    defaultThickness: null,
    summary: "Notification history inside Glint",
  },
  desktop: {
    capability: "surface.desktop",
    optionalCapabilities: [],
    placements: ["desktop"],
    defaultAnchor: null,
    defaultFocus: "none",
    defaultDismiss: [],
    defaultAutoHideMs: null,
    defaultThickness: null,
    summary: "Background canvas and widgets",
  },
  calendar: {
    capability: "surface.calendar",
    optionalCapabilities: [],
    placements: ["popup", "overlay"],
    defaultAnchor: "top-right",
    defaultFocus: "on-demand",
    defaultDismiss: ["escape", "outside-click"],
    defaultAutoHideMs: null,
    defaultThickness: null,
    summary: "Date and time popup",
  },
  "power-menu": {
    capability: "surface.power-menu",
    optionalCapabilities: ["system.power.sleep", "system.power.shutdown"],
    placements: ["overlay", "popup", "floating"],
    defaultAnchor: "center",
    defaultFocus: "exclusive",
    defaultDismiss: ["escape", "outside-click"],
    defaultAutoHideMs: null,
    defaultThickness: null,
    summary: "Sleep and shutdown options",
  },
  "workspace-switcher": {
    capability: "surface.workspace-switcher",
    optionalCapabilities: ["desktop.workspaces.read", "desktop.workspaces.switch"],
    placements: ["overlay", "popup", "floating"],
    defaultAnchor: "center",
    defaultFocus: "exclusive",
    defaultDismiss: ["escape", "outside-click"],
    defaultAutoHideMs: null,
    defaultThickness: null,
    summary: "Workspace UI",
  },
  "window-switcher": {
    capability: "surface.window-switcher",
    optionalCapabilities: ["desktop.windows.list", "desktop.windows.focus"],
    placements: ["overlay"],
    defaultAnchor: "center",
    defaultFocus: "exclusive",
    defaultDismiss: ["escape", "outside-click"],
    defaultAutoHideMs: null,
    defaultThickness: null,
    summary: "Window selection overlay",
  },
  "volume-osd": {
    capability: "surface.volume-osd",
    optionalCapabilities: ["system.audio.read"],
    placements: ["overlay"],
    defaultAnchor: "bottom",
    defaultFocus: "none",
    defaultDismiss: [],
    defaultAutoHideMs: 1500,
    defaultThickness: null,
    summary: "Volume feedback",
  },
  "brightness-osd": {
    capability: "surface.brightness-osd",
    optionalCapabilities: ["system.brightness.read"],
    placements: ["overlay"],
    defaultAnchor: "bottom",
    defaultFocus: "none",
    defaultDismiss: [],
    defaultAutoHideMs: 1500,
    defaultThickness: null,
    summary: "Brightness feedback",
  },
  "media-overlay": {
    capability: "surface.media-overlay",
    optionalCapabilities: ["system.media.read", "system.media.control"],
    placements: ["overlay", "popup", "floating"],
    defaultAnchor: "top-right",
    defaultFocus: "on-demand",
    defaultDismiss: ["escape", "outside-click"],
    defaultAutoHideMs: null,
    defaultThickness: null,
    summary: "Track metadata and playback",
  },
} as const satisfies Record<StandardSurfaceKind, SurfaceCatalogEntry>;

/** Stable surface definition error codes. */
export const SURFACE_ERROR_CODES = [
  "GLINT_SURFACE_INVALID_ID",
  "GLINT_SURFACE_UNKNOWN_KIND",
  "GLINT_SURFACE_INVALID_CUSTOM_ID",
  "GLINT_SURFACE_MISSING_FIELD",
  "GLINT_SURFACE_UNKNOWN_FIELD",
  "GLINT_SURFACE_INVALID_TYPE",
  "GLINT_SURFACE_INVALID_VALUE",
  "GLINT_SURFACE_OUT_OF_RANGE",
  "GLINT_SURFACE_INVALID_ENTRYPOINT",
  "GLINT_SURFACE_UNSUPPORTED_PLACEMENT",
  "GLINT_SURFACE_CONFLICT",
] as const;
export type SurfaceErrorCode = (typeof SURFACE_ERROR_CODES)[number];
