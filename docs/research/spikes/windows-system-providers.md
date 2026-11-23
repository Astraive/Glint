# Spike: Windows audio, network and battery providers (SPK-010)

Status: **code spike complete**.
Result: **pass**. Every provider reports a precise status on one Windows 11
laptop, and the restart and stale-subscription checks behave predictably.
Desktop-without-battery, device removal and real network changes could not be
produced on this machine; see [Not verified](#not-verified).

This answers the Windows half of
[Spike 6: native provider adapters](../../plan/08-research/feasibility-spikes.md#spike-6-native-provider-adapters)
for audio, network and battery. Linux and macOS providers are folded into
LNX-004 and MAC-008 because they need real hardware. Workspaces are not part of
this ticket. The spike is throwaway: it lives in
[`spikes/windows-system-providers`](../../../spikes/windows-system-providers/src/main.rs),
is never shipped and is never imported by product code.

## Environment

| Item | Value |
|---|---|
| OS | Windows 11 Home Single Language, version 10.0.26300 (build 26300) |
| Hardware | Laptop (`PCSystemType` 2) with one internal battery, on AC during the runs |
| Audio | One active render endpoint (2 channels, -63.5 to 0 dB in 0.5 dB steps, 51 steps, hardware volume + mute); one capture endpoint; 22 non-active render endpoints (disabled / not present / unplugged) |
| Network | Wi-Fi (Realtek RTL8852BE) with internet; also a Tailscale tunnel and an Npcap loopback adapter; 58 stored connection profiles |
| Location privacy | Allowed at machine, user and unpackaged-app level (relevant to SSID access) |
| Rust | 1.95.0 (pinned), edition 2024, MSVC target |
| Code revision | branch for SPK-010, rebased on `b41b7a7` |

## Crates and versions

| Crate | Version | Why |
|---|---|---|
| `windows` | 0.62.2 | Win32 COM (Core Audio, Network List Manager, `GetSystemPowerStatus`, registry) and WinRT (`NetworkInformation`, `PowerManager`, `Battery`) |
| `windows-core` | 0.62.2 | Direct dependency because `#[implement]` (callback objects) expands to `windows_core::` paths |
| `serde_json` | 1.0.154 | JSON report |

All three were already in `Cargo.lock` (the appbar spike uses the same
`windows` minor through `webview2-com`), so the workspace still builds one copy
and the [third-party inventory](../../legal/third-party-inventory.md) is
unchanged. The Windows dependencies sit under
`[target.'cfg(windows)'.dependencies]`. On other hosts the binary compiles to a
stub that prints `windows-only spike: unsupported on this platform` and exits 2
(checked locally with `cargo clippy --target x86_64-linux-android`; Linux and
macOS results come from CI).

## API choices

| Provider | Read | Change events | Notes |
|---|---|---|---|
| Audio | `IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eConsole)` -> `IMMDevice::Activate(IAudioEndpointVolume)` -> `GetMasterVolumeLevelScalar`, `GetMute` | `IAudioEndpointVolumeCallback` (`RegisterControlChangeNotify`); `IMMNotificationClient` (`RegisterEndpointNotificationCallback`) for device add/remove/state and default-device changes | Write: `SetMasterVolumeLevelScalar` / `SetMute` with a spike-owned event-context GUID |
| Network | Win32 Network List Manager: `INetworkListManager::GetConnectivity`, `IsConnectedToInternet` | `INetworkListManagerEvents::ConnectivityChanged` through `IConnectionPointContainer` / `IConnectionPoint::Advise` | Both stacks were probed |
| Network details | WinRT `Windows.Networking.Connectivity.NetworkInformation::GetInternetConnectionProfile`: connectivity level, cost type, IANA interface type, WLAN/WWAN flags, `WlanConnectionProfileDetails::GetConnectedSsid` | `NetworkInformation::NetworkStatusChanged` | The SSID is never printed, only whether it was readable and its length |
| Battery | Win32 `GetSystemPowerStatus` (no COM); WinRT `PowerManager` (status, supply, percent, discharge time, energy saver); `Battery::AggregateBattery().GetReport()` (mWh capacities, charge rate) | `PowerManager::{BatteryStatus,PowerSupplyStatus,RemainingChargePercent,EnergySaverStatus}Changed`; `Battery::ReportUpdated` | `RegisterPowerSettingNotification` needs an HWND or service handle and was not used |

## How to run

```powershell
# never changes volume: writes back the value it just read
cargo run -p spike-windows-system-providers -- --smoke 5
# opt-in: lowers the master level by 0.01 scalar and restores it at once,
# so volume callbacks can be observed
cargo run -p spike-windows-system-providers -- --smoke 5 --nudge
```

`--smoke N` (1-120 s) prints one JSON report with the capability table, each
provider's current values, the callbacks seen in the N-second window, the
restart and stale-subscription results, the apartment checks and timings.
Exit codes: 0 report produced and no provider hit a failure or timeout, 1
otherwise, 2 bad arguments or unsupported platform.

The volume is re-read after every write and restored if it differs. In both
runs below all six writes read back the original value (`unchanged_after:
true`, `restored: false`); in `--nudge` mode the level was away from the
original for 0.5-6.3 ms per write.

## Availability mapping

Statuses use a spike-local copy of the proposed
[`Availability`](../../plan/02-architecture/capabilities-permissions-and-errors.md#proposed-capability-representation)
enum. Errors use a typed error with five kinds: `unsupported`,
`permission-denied`, `unavailable`, `timeout`, `failure`. Only the first three
turn into a status; timeouts and failures stay errors, so the spike never
reports a fake success.

### Observed on this machine

| Capability | Status | Evidence |
|---|---|---|
| `provider.audio` | supported | read and write both work |
| `system.audio.read` | supported | level 0.189, not muted |
| `system.audio.write` | supported | `SetMasterVolumeLevelScalar` accepted (same-value and nudge modes), no elevation or prompt |
| `system.audio.events` | supported | registration ok; with `--nudge` our write arrived in 0.35-0.97 ms |
| `system.audio.device-events` | supported | registration ok; no device change was made, so 0 callbacks |
| `provider.network` | read-only | no write capability is offered |
| `system.network.read` | supported | NLM flags `0x42`: IPv4 internet, IPv6 no traffic |
| `system.network.events` | supported | NLM `Advise` and `NetworkStatusChanged` registered; no network change was made |
| `system.network.details` | supported | level `internet-access`, interface `wifi` (IANA 71), cost `unrestricted` |
| `system.network.ssid` | supported | SSID readable (length 4) with location consent `Allow` |
| `system.network.toggle` | unavailable | no toggle path in this provider; `Windows.Devices.Radios` not evaluated |
| `provider.battery` | read-only | no write capability exists |
| `system.battery.read` | supported | `GetSystemPowerStatus` 78 %, on AC, not charging (flag 1); `PowerManager` `idle`; aggregate report 41 290 / 52 600 mWh |
| `system.battery.events` | supported | 5 WinRT subscriptions registered; no change happened in the window |
| `system.power.source.read` | supported | `ACLineStatus` 1 |

### Mapping rules (unit-tested in `src/model.rs`)

| Input | Maps to |
|---|---|
| `GetSystemPowerStatus.BatteryFlag & 128` (no system battery) | `system.battery.read` = **unavailable** ("no system battery"), not an error; `on_ac` is still reported, so `system.power.source.read` stays supported on a desktop |
| `BatteryFlag == 255` | unavailable with a different reason ("battery status unknown"), no values |
| `BatteryLifePercent == 255`, `BatteryLifeTime == 0xFFFFFFFF` | value absent (`null`), never 255 % |
| WinRT `BatteryStatus.NotPresent` (0) | unavailable; Discharging / Idle / Charging = supported |
| `E_NOTFOUND` from `GetDefaultAudioEndpoint` (no render device) | `system.audio.*` = unavailable |
| `AUDCLNT_E_DEVICE_INVALIDATED`, `AUDCLNT_E_SERVICE_NOT_RUNNING`, `ERROR_DEVICE_NOT_CONNECTED` | unavailable |
| Activation error on an endpoint whose `DEVICE_STATE` is not ACTIVE | unavailable whatever the HRESULT (see [device removal](#device-removal-stand-in)) |
| `E_ACCESSDENIED` | permission-required (for example a denied write) |
| `REGDB_E_CLASSNOTREG`, `CLASS_E_CLASSNOTAVAILABLE`, `E_NOINTERFACE`, `E_NOTIMPL` | `unsupported` error, reported as unavailable with an "unsupported" reason |
| `RPC_E_TIMEOUT`, `ERROR_TIMEOUT`, no callback within the deadline | `timeout` error (no status) |
| `CO_E_NOTINITIALIZED`, `RPC_E_WRONG_THREAD`, anything else | `failure` error (no status): a programming error must not look like missing hardware |
| WLAN profile with SSID readable | `system.network.ssid` supported |
| WLAN profile, SSID empty, location consent `Deny` at any level | permission-required |
| Not a WLAN profile | `system.network.ssid` unavailable |
| Provider: read supported + write supported / read supported, no write | supported / read-only |

## Apartment and threading findings

| Check | Observed |
|---|---|
| Win32 COM before any `CoInitializeEx` in the process | `CoCreateInstance(MMDeviceEnumerator)` and `CoCreateInstance(NetworkListManager)` both fail with `CO_E_NOTINITIALIZED` (`0x800401F0`) |
| WinRT on the same uninitialized thread | `PowerManager::BatteryStatus` **succeeds** (1.6-4 ms): windows-rs retries `RoGetActivationFactory` with `CoIncrementMTAUsage` when it sees `CO_E_NOTINITIALIZED`. Afterwards the thread reports `MTA (implicit)`: windows-rs created the process MTA as a side effect |
| Main thread `CoInitializeEx(COINIT_MULTITHREADED)` | MTA; every provider works there |
| Uninitialized thread once the process has an MTA | `MTA (implicit)`; Core Audio read and NLM creation both succeed |
| `COINIT_APARTMENTTHREADED` thread (first STA, so it is the main STA) | Core Audio and NLM activate, read, register and unregister fine; WinRT reads work |
| Where `IAudioEndpointVolumeCallback` runs | Always on Core Audio worker threads in the implicit MTA, never on the registering thread, MTA or STA. The STA thread got its 4 callbacks **without pumping messages** (0 messages dispatched) |
| `IMMNotificationClient` | Registered from the MTA; not triggered (no device change), so its thread is not observed. Microsoft documents the same worker-thread delivery |
| NLM events | `Advise` works from MTA and STA. Delivery was not triggered. For an STA sink COM marshals events through the STA's message queue, so a provider must either run NLM on an MTA thread or pump messages |
| WinRT events (`NetworkStatusChanged`, `PowerManager`, `Battery.ReportUpdated`) | windows-rs delegates are agile; registration works from any apartment. No event fired in the window |
| Callback concurrency | Volume callbacks for one subscription arrived on two different worker threads in one run. The handler must be thread-safe, quick and must not call back into the endpoint while holding a lock |

**Recommendation.** Give `glint-system` one dedicated provider thread that calls
`CoInitializeEx(COINIT_MULTITHREADED)` once and owns every Core Audio and NLM
object. Treat every callback as arriving on an arbitrary thread: copy the
values, push them onto a bounded channel to the provider thread and return. Do
not depend on windows-rs creating the MTA implicitly; initialize it on purpose.
Never use the WebView2 / UI STA for providers.

## Restart and stale-subscription evidence

All numbers come from the `--nudge` run unless marked. Same-value writes
produce no callbacks, so they cannot prove delivery.

### Audio

| Check | Result |
|---|---|
| Writing back the same level and mute | Accepted, but **no `OnNotify`** at all (timeout after 1.5 s in the default run). Windows only notifies real changes |
| Each real change | **Two** `OnNotify` calls: one with our event-context GUID and one with a different, non-null context about 1 ms later (another process, probably the shell's volume UI, writing the same value). Filtering by context GUID removes our own echo, not every echo |
| Callback A after `UnregisterControlChangeNotify` + a real change | 0 callbacks in the next 500 ms |
| Unregister the same volume callback twice | Second call returns `S_OK` (silent) |
| Unregister the same `IMMNotificationClient` twice | Second call returns `E_NOTFOUND` (`0x80070490`) |
| Callback C registered, then every reference to its `IAudioEndpointVolume` released **without** unregistering | 0 callbacks for later changes: releasing the endpoint object ends its subscriptions (no leaked delivery observed) |
| "Restart": release the enumerator, create everything again | 2.8-6.7 ms; same default endpoint; new callback B receives the change (0.75 ms); A still receives nothing |
| Pre-restart `IAudioEndpointVolume` kept as a stale handle | Still reads the right level: the handle is bound to the endpoint, not to the enumerator |
| Callbacks for any observe subscription after the main teardown + a real change | 0 for all six subscriptions |

#### Device removal stand-in

Physically removing the default device was not possible. Instead the spike
activates `IAudioEndpointVolume` on non-active render endpoints:

| Endpoint state | Result | Generic mapping | Mapping with endpoint state |
|---|---|---|---|
| NOTPRESENT (4) | `AUDCLNT_E_DEVICE_INVALIDATED` | unavailable | unavailable |
| NOTPRESENT (4) | `ERROR_PATH_NOT_FOUND` (`0x80070003`) | **failure** | unavailable |
| UNPLUGGED (8) | activation and read **succeed** | supported | supported |

So drivers return different HRESULTs for a gone device, and an unplugged jack
still yields a working volume object. The product must look at
`DEVICE_STATE` / `OnDeviceStateChanged` rather than at HRESULTs alone, and must
not treat "activation succeeded" as "device usable".

### Network

| Check | Result |
|---|---|
| `Unadvise` with the same cookie twice | Second call fails with `OLE_E_NOCONNECTION` (`0x80040004`, not the documented `CONNECT_E_NOCONNECTION`) |
| `Unadvise` through a connection point after its manager was released | Same `OLE_E_NOCONNECTION` |
| Recreate the manager and `Advise` again | 4.4-19 ms; **the new cookie equals the old one (1)**. A stale cookie can therefore name somebody else's live subscription; only the owner of the subscription may unadvise it |
| `RemoveNetworkStatusChanged` twice, and with a token that was never issued | All return `S_OK`: WinRT removal cannot detect stale tokens |

### Battery

| Check | Result |
|---|---|
| Remove each of the 5 WinRT subscriptions twice | All `S_OK` |
| Unsubscribe, resubscribe (0.02-0.03 ms), unsubscribe | All succeed; a fresh aggregate report reads `idle` afterwards |

## Timings (debug build)

| Operation | Time |
|---|---|
| Open default render endpoint (enumerator + endpoint + activate) | 5.8-8.0 ms |
| Read level + mute | 0.05-0.14 ms |
| NLM create / connectivity read | 1.0-2.2 ms / 5.0-5.3 ms |
| WinRT internet profile + details (includes SSID) | 63-82 ms |
| `GetSystemPowerStatus` / `PowerManager` reads / aggregate report | 0.01 / 0.02 / 0.13 ms |
| Whole smoke run with a 5 s window | 8.5-13.6 s |

The WinRT network profile is the only slow read; it should never run on a UI
thread and should be refreshed on events, not polled.

## Not verified

- **Desktop without a battery.** This machine has one, so flag 128 and
  `NotPresent` were never returned by the OS. The mapping is covered by unit
  tests only.
- **No default render device** (`E_NOTFOUND`) and **removal of the active
  device** at runtime (`AUDCLNT_E_DEVICE_INVALIDATED` on an object already in
  use, `OnDeviceRemoved` / `OnDefaultDeviceChanged` with a null ID). Needs a
  USB or Bluetooth headset plugged and unplugged by a person.
- **Audio service restart** (`AudioSrv` stop/start needs elevation and
  interrupts the user's audio): whether existing objects return
  `AUDCLNT_E_SERVICE_NOT_RUNNING` and whether subscriptions survive.
- **Real network changes**: NLM `ConnectivityChanged`, `NetworkStatusChanged`
  delivery, their thread and their latency, Wi-Fi off/on, airplane mode,
  metered networks, captive portals.
- **Real battery changes**: unplugging AC, charge-percent ticks, energy saver
  toggling. No battery event fired during the runs.
- **SSID with location denied.** Consent was `Allow`; changing a privacy
  setting was out of bounds, so `permission-required` for SSID is a unit-tested
  mapping, not an observation.
- **Permission-required for audio or battery.** None occurred: unpackaged
  desktop processes read and write volume and read battery state without a
  prompt. Packaged (MSIX) or AppContainer hosts were not tried.
- **Mute toggling** was never exercised (it would silence the user's audio).
  `SetMute` with the current value was accepted.
- Sleep/resume, user switching, Windows 10, ARM64 and multi-endpoint
  default-role changes were not tested.

## Recommendations

**CAP-\* (capability model).**
- Keep `Availability` and the five error kinds exactly as distinct as here:
  only unsupported, permission-denied and unavailable become statuses.
- Report audio, network and battery per sub-capability (`read`, `write`,
  `events`, `device-events`, `details`, `ssid`, `power.source.read`) and
  derive the provider status from read + write; `read-only` is a derived
  value, not something an API returns.
- `system.network.ssid` needs its own capability and privacy scope, separate
  from `system.network.read`, and a permission-required state driven by the
  location consent.
- Battery absence is a normal `unavailable` with a reason, and must emit a
  capability change event if a battery appears or disappears.

**`glint-system` (shared providers).**
- One MTA provider thread, explicit `CoInitializeEx`, all callbacks forwarded
  over a bounded channel and coalesced (`audio.volume.changed` is debounced in
  the [event model](../../plan/02-architecture/process-model-and-ipc.md#event-model)).
- Audio: dedupe by value, because each change arrives twice; tag
  Glint's own writes with a per-process context GUID and drop that echo; do not
  expect a callback for a write that changes nothing, so update the cached
  snapshot from the write itself.
- Audio device changes: on `OnDefaultDeviceChanged` (eRender, eConsole) or any
  `AUDCLNT_E_DEVICE_INVALIDATED`, unregister from the old endpoint, re-open the
  default endpoint and re-register; treat a null default ID as
  `unavailable`. Map activation errors with the endpoint state.
- Network: use NLM for connectivity and its event (cheap, works without a
  profile) and WinRT `NetworkInformation` for details and the SSID, fetched off
  the UI thread on change.
- Battery: `GetSystemPowerStatus` for the snapshot (cheapest, no COM, clean
  "no battery" flag) and `PowerManager` / `Battery.ReportUpdated` for events.
- Subscriptions are owned objects: keep the interface, cookie or token
  together with the object it came from, unsubscribe exactly once, and treat a
  second unsubscribe as a no-op whatever the API returns (`S_OK`, `E_NOTFOUND`
  or `OLE_E_NOCONNECTION`). Never reuse NLM cookies across manager instances.

**WIN-\* (Windows host).**
- Put these adapters in `glint-windows` (`audio.rs`, `network.rs`, `power.rs`
  in the [proposed layout](../../plan/03-platforms/windows-architecture.md#proposed-adapter-structure))
  with the threading model above.
- Volume writes need no elevation for an unpackaged desktop process; re-check
  if Glint ships as MSIX.
- Run a human hardware pass for the [not verified](#not-verified) items:
  headset plug/unplug, Wi-Fi off/on, AC unplug, energy saver, location consent
  off.
- `Windows.Devices.Radios` (Wi-Fi / Bluetooth toggles) needs its own spike
  before `system.network.toggle` can be anything but unavailable.
