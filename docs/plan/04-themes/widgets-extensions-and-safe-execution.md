# Widgets, extensions and sandboxing strategy

## Widget model

A widget is a configurable unit displayed inside a surface. Examples: clocks/calendars, battery, system graphs, network indicators, weather, media, a 13x13 glyph grid or larger 25x25 dot-matrix widget. Multiple widgets on one desktop surface should share rendering resources and system subscriptions.

## Built-in widgets

- Clock/date, calendar, system stat strip, application launcher grid, audio slider and media control.
- These use trusted SDK provider APIs rather than making independent OS-specific calls.
- Widgets expose schema-driven settings so the manager can generate a properties inspector.
- A widget's responsiveness and accessibility remain the responsibility of Glint runtime + package author.

## Third-party widgets

Supported as separately packaged `kind: widget` or bundled in a theme. A widget package declares exposed component entrypoints and configuration schema, required capabilities and optional permissions. Widget code runs in the same restricted class of renderer as programmable themes; trust may require isolated process per untrusted publisher, but renderer count must be controlled.

## Extension tiers

1. **Data-only** tokens, asset packs and layouts — no dynamic execution; safest.
2. **Sandboxed UI** compiled JS/CSS using approved SDK operations — programmable themes/widgets, explicit permissions.
3. **Sandboxed WASM logic (future)** for more involved computation; WASM alone does not solve resource or permission sandboxing; host calls still restricted.
4. **Native integrations** — project-maintained signed/trusted platform code installed through Glint updates, **not** arbitrary theme-provided binaries.

## Security design review

Potential hazards: JS running with unintended local file/native IPC access; archive path traversal; Git history/symlink tricks; remote script injection; manipulated registry metadata; malicious CSS visual spoofing; overlay phishing/lockscreen imitation; infinite render loops/CPU miners; API request floods; native integration abuse; stored credentials in exported profiles.

Recommended mitigations:

- Isolated renderer process or per-publisher trust group; sandbox host according to OS.
- Strict `glint://` asset protocol and CSP; disable navigation/external remote script by default.
- Distinct IPC security tokens per renderer/surface and a narrow action allowlist.
- Permission categories bound to package ID/version and user profile; re-request after privilege expansion.
- Timeouts, memory/process limits, rate limits, crash counters and safe-mode.
- No arbitrary `process`, shell, file, registry edit, HTTP access or native addon bridge.
- Validate archive symlinks, path canonicalization, duplicates, size and compression ratios before extracting.
- Official gallery code review/automated scanning is additional defense, never a substitute for runtime sandbox.

## Notification/lock screen spoofing

Avoid giving ordinary themes full-screen secure-looking prompts on OS login layers or capturing credentials. Distinguish Glint-owned overlays from trusted OS dialogs visually/through permissions. Require explicit opt-in for always-on-top and keyboard capture, with an OS-wide recovery shortcut.

## Permissions UI

During install/activation show: publisher, provenance, local/network/OS control permissions, reasons for each, supported OS, signed/unsigned status, how to revoke. Do not claim that `official` is cryptographically verified until a signing infrastructure actually exists.
