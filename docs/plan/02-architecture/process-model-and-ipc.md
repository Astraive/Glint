# Daemon, shell, IPC, event bus and concurrency

## Session boot sequence

1. Autostart launches Glint daemon in the current user session (not as elevated privileged OS service).
2. Daemon acquires per-user single-instance lock; initializes storage and reads merged config.
3. Runs adapter discovery: OS, session type (Wayland/X11), compositor, DE, displays, permissions and available system providers.
4. Loads last known-good profile and validates installed theme package locks/hashes.
5. Starts surface host(s) appropriate to OS/session, with restricted token and IPC socket/pipe.
6. Hosts register capabilities; daemon provides resolved surface definitions, allowed SDK scope and tokenized asset roots.
7. Glint shell displays core surfaces; failures return to built-in fallback and leave native desktop intact.
8. Settings app/CLI can connect/disconnect independently.

## IPC design

Use typed messages, explicitly versioned. Candidate transport: Unix domain sockets for Linux/macOS and named pipes or equivalent local IPC for Windows. Tauri commands may handle manager -> daemon forwarding, but do not expose daemon privileges to imported theme webviews.

```rust
// Illustrative API contract, not production code.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum GlintRequest {
    GetCapabilities,
    ListThemes,
    PreviewTheme { package_id: String },
    ApplyProfile { profile_id: String, expected_revision: u64 },
    SetVolume { percent: u8 },
    ShowSurface { name: String },
}

pub struct ResponseEnvelope<T> {
    pub request_id: String,
    pub api_version: u32,
    pub result: Result<T, GlintError>,
}
```

Responses should be bounded and timeout-aware. Distinguish a failed request from `permission-denied` and unsupported OS operation. Avoid accepting arbitrary command names or shell invocations through IPC.

## Theme SDK access bridge

- Renderer gets a short-lived capability token scoped to package ID/version, surface instance and explicitly allowed API methods.
- `glint://assets/...` or a strict local custom protocol serves **only** validated immutable package assets.
- SDK invokes a small allowlisted message protocol, not all Tauri commands.
- Core rechecks every request against current grants, platform capabilities, surface identity and rate limits.
- Token rotation/invalidations occur on theme unload/update/revocation.
- Use no broad `fs.read`, `exec`, `shell`, `http.fetch` by default. Any network capability is separate and scoped.

## Event model

Normalized events such as:

| Event | Payload | Producer | Handling |
|---|---|---|---|
| `audio.volume.changed` | percent, mute state | Audio adapter | Debounced updates |
| `display.topology.changed` | monitor list | Display backend | Recalculate layout |
| `network.state.changed` | connectivity, label (if allowed) | Network provider | Privacy filtering |
| `workspaces.changed` | workspace list, active ID | Compositor adapter | Capability-scoped |
| `theme.profile.changed` | active profile revision | Daemon | Reload affected surfaces |
| `theme.permission.revoked` | package and grant | Permission manager | Tear down subscriptions |
| `renderer.crashed` | surface, reason | Host supervisor | Fallback/report |

Events carry monotonic sequence IDs and current snapshots so clients can recover after reconnect without assuming all intermediate events are delivered. Providers are **shared** across widgets and active only while subscribed.

## Concurrency and performance

- Tokio async IO for file/network/IPC and providers where appropriate; native UI threads remain controlled by native event loops.
- Limit blocking/COM work to appropriate threads; GTK/AppKit operations to their main threads as needed.
- Bound queue sizes, coalesce high-frequency system events and stop polling when a panel is hidden.
- Avoid one webview per clock/CPU widget; one renderer per panel or bounded group is preferable.
- Distinguish *instant UI actions* from *slow OS transitions*. UI should optimistically show a pending spinner only when safe, then confirm after the daemon acknowledges.

## Protocol evolution

- Version `apiVersion` and SDK semver. `glint.json` format version independent of SDK version.
- Prefer additive changes with feature negotiation; old themes receive stable shims or clean incompatibility warnings.
- Maintain golden JSON fixtures and generated types from one source of truth.
- Include diagnostics: request trace ID, package, surface, platform, capability and error code; redact Wi-Fi SSIDs or other user data where necessary.
