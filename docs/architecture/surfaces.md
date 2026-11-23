# Surfaces: catalog and definition format

Status: **proposed** (SRF-001). This page specifies the portable surface
catalog and the `placement` and `behavior` objects of a theme manifest's
`surfaces.<id>` entries. It follows the
[surface contract](../plan/02-architecture/surfaces-and-layout-engine.md#surface-contract),
[placement semantics](../plan/02-architecture/surfaces-and-layout-engine.md#placement-semantics)
and the [standard surface contract](../plan/docs/superpowers/specs/2026-11-23-glint-platform-design.md#7-standard-surface-contract).
The reference implementation is `crates/glint-surfaces`; the shared case
corpus is [`tests/package-fixtures/surfaces/cases.json`](../../tests/package-fixtures/surfaces/cases.json).
Like the manifest format, everything here may change until stabilized by
tests and an ADR.

## Where validation happens

`glint-schema` parses `glint.json` and checks each surface's ID syntax,
`entry` path and that `placement` and `behavior` are objects; it keeps those
two objects as raw JSON. `glint_surfaces::surface_definitions(&manifest)`
then validates their shape and returns one `SurfaceDefinition` per surface:

```text
SurfaceDefinition { id, kind, entrypoint, placement, behavior, capabilities }
```

A host must call it before mounting any surface. `SurfaceDefinition::parse(id,
&value)` validates a single entry the same way. Checks stop at the first
problem, in this order: ID, object type, unknown fields, `entry`,
`placement`, `behavior`. Unknown keys are rejected at every level (a surface
is a security-sensitive section, see
[manifest compatibility](manifest-compatibility.md)).

In v1 a surface's ID is its key in `surfaces` and is also its kind, so a
theme provides at most one surface per kind. `id` and `kind` are separate
fields so profiles can later place several instances of a kind.

## Catalog

Each standard kind requires `surface.<id>`: the host must offer it or the
surface cannot be shown and the profile falls back (SRF-004). Optional
capabilities only enable controls; without them the surface still shows with
those controls disabled. The first placement listed is the default.

| ID | Required | Optional | Placements | Default anchor | Default focus | Default dismiss / auto-hide |
|---|---|---|---|---|---|---|
| `panel` | `surface.panel` | none | edge, floating | top | non-activating | none |
| `dock` | `surface.dock` | `desktop.windows.list`, `desktop.windows.focus` | floating, edge | bottom | non-activating | none |
| `launcher` | `surface.launcher` | none | floating, overlay, popup | center | exclusive | escape, outside-click |
| `control-center` | `surface.control-center` | `system.audio.read/write`, `system.brightness.read/write`, `system.network.read/toggle`, `system.bluetooth.read/toggle` | popup, overlay, floating | top-right | on-demand | escape, outside-click |
| `notification` | `surface.notification` | none | overlay | top-right | none | 5000 ms |
| `notification-center` | `surface.notification-center` | none | popup, overlay | top-right | on-demand | escape, outside-click |
| `desktop` | `surface.desktop` | none | desktop | none | none | none |
| `calendar` | `surface.calendar` | none | popup, overlay | top-right | on-demand | escape, outside-click |
| `power-menu` | `surface.power-menu` | `system.power.sleep`, `system.power.shutdown` | overlay, popup, floating | center | exclusive | escape, outside-click |
| `workspace-switcher` | `surface.workspace-switcher` | `desktop.workspaces.read`, `desktop.workspaces.switch` | overlay, popup, floating | center | exclusive | escape, outside-click |
| `window-switcher` | `surface.window-switcher` | `desktop.windows.list`, `desktop.windows.focus` | overlay | center | exclusive | escape, outside-click |
| `volume-osd` | `surface.volume-osd` | `system.audio.read` | overlay | bottom | none | 1500 ms |
| `brightness-osd` | `surface.brightness-osd` | `system.brightness.read` | overlay | bottom | none | 1500 ms |
| `media-overlay` | `surface.media-overlay` | `system.media.read`, `system.media.control` | overlay, popup, floating | top-right | on-demand | escape, outside-click |
| `custom:<namespace>.<name>` | `surface.custom` | none | any | must declare `placement.kind` | must declare `behavior.focus` | none |

Custom IDs use the package ID rule after `custom:`: at least two
dot-separated lowercase segments (`custom:dev.example.clock`). An
unnamespaced ID such as `custom:clock` is refused
(`GLINT_SURFACE_INVALID_CUSTOM_ID`; `glint-schema` already refuses it as an
invalid key). A well-formed name outside the catalog, such as `taskbar`, is
`GLINT_SURFACE_UNKNOWN_KIND`.

## Entrypoint

`entry` must be a package-local `.html` or `.htm` document: relative,
`/`-separated, no empty, `.` or `..` segments, no `\`, no `:`, no
non-portable characters or Windows device names, at most 512 bytes. Remote
pages, `file://`, `data:` and protocol-relative URLs are never loaded. These
are the `glint-schema` package-path rules; a test runs both parsers on the
same inputs.

## `placement`

| Key | Type | Applies to | Default |
|---|---|---|---|
| `kind` | `edge` \| `floating` \| `desktop` \| `overlay` \| `popup` | all | catalog default; required for custom |
| `anchor` | `top-left` `top` `top-right` `left` `center` `right` `bottom-left` `bottom` `bottom-right` | all but desktop; edge accepts only `top`/`bottom`/`left`/`right` | catalog anchor for the default kind, else `center`; edge without a catalog anchor must declare one |
| `monitor` | `primary` \| `active` \| `all` | all; `all` not for popup | `primary` |
| `width`, `height` | integer 1..16384 (logical px) | see below | none |
| `minWidth`, `maxWidth`, `minHeight`, `maxHeight` | integer 1..16384 | floating, overlay, popup | none |
| `margin` | integer 0..1024 | all but desktop | 0 |

- **edge** spans the anchored edge and needs its thickness: `height` on a
  top/bottom edge, `width` on a left/right edge. `panel` defaults it to 40
  and `dock` to 64; custom surfaces must declare it. The other size and all
  min/max keys are refused. A `panel` with only an `entry` (as in the
  bundled default theme) is therefore a 40 px top edge.
- **desktop** fills the monitor: `anchor`, sizes and `margin` are refused.
- **floating, overlay, popup** may set any size; `min* <= max*` and the
  requested size must lie between them.
- Specific monitors are chosen by the user's profile (stable, privacy-safe
  monitor keys), never by a theme. The layer follows from the kind; exact
  z-order is never promised.

## `behavior`

| Key | Type | Rule | Default |
|---|---|---|---|
| `focus` | `none` \| `non-activating` \| `on-demand` \| `exclusive` | desktop cannot be `exclusive` | catalog; required for custom |
| `reserveWorkArea` | boolean | only `edge` | `false` |
| `dismissOn` | array of unique `escape` \| `outside-click` | not for edge or desktop | catalog (empty for edge and desktop) |
| `autoHideMs` | integer 250..60000 | only overlay and popup | catalog for overlay/popup |
| `clickThrough` | boolean | only desktop and overlay, and `focus` must be `none` | `false` |

On Windows, `reserveWorkArea` maps to an application appbar
([appbar spike](../research/spikes/windows-appbar.md)); on Wayland to a
layer-shell exclusive zone where advertised. Whether a host can honour a
placement or focus policy is a host capability reported by the surface
backend (SRF-002), never assumed from this format.

## Error codes

Every error carries a stable code, an RFC 6901 pointer relative to the
manifest root (e.g. `/surfaces/panel/placement/height`) and a message. Codes
and pointers are stable; message wording is not. Codes are only added.

| Code | Meaning |
|---|---|
| `GLINT_SURFACE_INVALID_ID` | Surface ID is neither a lowercase name nor a `custom:` ID |
| `GLINT_SURFACE_UNKNOWN_KIND` | Well-formed name that is not a standard kind |
| `GLINT_SURFACE_INVALID_CUSTOM_ID` | `custom:` ID without a dotted namespace, or malformed |
| `GLINT_SURFACE_MISSING_FIELD` | Required field absent (`entry`, custom `placement.kind`/`behavior.focus`, edge `anchor` or thickness) |
| `GLINT_SURFACE_UNKNOWN_FIELD` | Key not in the format (first in sorted order) |
| `GLINT_SURFACE_INVALID_TYPE` | Wrong JSON type (including fractional numbers) |
| `GLINT_SURFACE_INVALID_VALUE` | String outside its vocabulary, corner anchor on an edge, duplicate dismiss trigger |
| `GLINT_SURFACE_OUT_OF_RANGE` | Number outside its range |
| `GLINT_SURFACE_INVALID_ENTRYPOINT` | Entry is not a package-local `.html` document |
| `GLINT_SURFACE_UNSUPPORTED_PLACEMENT` | Placement kind not allowed for this surface kind |
| `GLINT_SURFACE_CONFLICT` | Fields contradict each other |

## TypeScript types

`packages/types/src/surfaces.ts` (`@glint/types`) is generated by
`glint_surfaces::typescript_module()`: kinds, vocabularies, input and
resolved interfaces, the catalog, limits and error codes. The resolved
interfaces describe the JSON a host gets from serializing a
`SurfaceDefinition`. `cargo test -p glint-surfaces --test typegen` fails on
drift; regenerate with
`GLINT_UPDATE_TYPES=1 cargo test -p glint-surfaces --test typegen`.

## Decisions where the plan is ambiguous

- The plan's illustrative `"position": "center"` for floating surfaces is
  not accepted: `anchor` is used for every placement kind. The
  `full-programmable.json` manifest fixture (modelled on the plan) passes
  `glint-schema` but is refused here with `GLINT_SURFACE_UNKNOWN_FIELD` at
  `/surfaces/launcher/placement/position`, and a test records that.
- The illustrative definition's surface-level `monitor` and
  `requiredCapabilities` are not manifest fields: `monitor` lives in
  `placement`, and surface capabilities are implied by the catalog.
- The plan's capability list names `surface.notification-overlay` and
  `surface.desktop-widget`; the catalog uses `surface.<id>` for every kind
  (`surface.notification`, `surface.desktop`). Widgets are not surfaces.
- Optional provider capabilities follow each kind's role in the plan's
  surface table; the frozen capability registry (CAP-005) may revise them.
