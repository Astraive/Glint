# Decision register, proposal status, unresolved questions

## Confirmed by user discussion (binding product requirements)

| ID | Decision / constraint | Source context |
|---|---|---|
| D01 | Name of project is **Glint**. | Earlier naming discussion |
| D02 | Glint must target **multiple desktop OSes**, particularly Windows and Linux and also macOS. | Initial cross-platform correction and current discussion |
| D03 | Goal is deep theme/desktop customization: panels/taskbar-like surfaces, widgets, notification UI, controls, wallpaper, icons etc. | Original Rust customization request |
| D04 | Glint is a general theme platform, not permanently locked to a Nothing UI look. | Pivot from hardcoded design to marketplace/themes |
| D05 | `glint-library` is a **separate, publicly readable, project-owned official library repository**. | Registry discussion |
| D06 | Glint ships default configuration pointing at the official library. | Registry discussion |
| D07 | Third parties can host independent Git repos; official-library submission is optional. | User's explicit clarification |
| D08 | Standalone portable **`.glint` import/export** must work; no forced Git hosting. | User's explicit clarification |
| D09 | Theme designers must be able to customize **entire Glint-managed screens** rather than only colors or backgrounds. | “how ... write code for screens” |
| D10 | Multi-OS architecture and feature differences must be remembered in every design. | User's repeated reminder |
| D11 | Brand palette avoids green and is neutral, graphite, silver, white. | Earlier color decision |

## Recommended architecture (not yet explicitly locked)

- Rust core plus **per-user daemon**, settings application and independent shell surface hosts.
- `Tauri 2 + React + TypeScript` for management UI; React/TypeScript as **one supported programmable theme authoring mode**; avoid committing all surfaces to Tauri's window abstraction.
- Rust `wry` + platform-specific native window containers where embeddable webviews work; fallback to native surface renderers as needed.
- `@glint/sdk`, `@glint/theme-runtime`, `@glint/ui` (optional), `glint-package`, `glint-registry`, `glint-sources` etc.
- Manifest `glint.json`, `.glint` ZIP archive, format/schema versioning, Git sparse/partial fetching where beneficial.
- Explicit capability negotiation, permission gating, renderer isolation, default fallback theme and profile composition.
- Hyprland as an early *deep shell integration* target alongside a Windows appbar/overlay path, not as the exclusive supported OS.
- Local-first SQLite plus files; no mandatory backend, login or marketplace.
- Developer tool CLI `glint theme dev`, `build`, `pack`, `validate`, `import`, `use`.

## Open product decisions

| ID | Question | Working recommendation | Why it matters |
|---|---|---|---|
| O01 | Can authors target only certain OSes? | **Yes**; platform compatibility and required capabilities are explicit. | Avoid lowest-common-denominator themes. |
| O02 | Is React mandatory? | **No** for declarative themes; React/TS recommended for programmable v1. | Wider creator ecosystem. |
| O03 | Does the application modify native taskbars by default? | No; custom Glint panels first, native modifications only via supported adapters/opt-in. | Stability and user trust. |
| O04 | How are community themes trusted? | Data-only by default; sandboxed programmable themes with strict IPC permissions. | Supply-chain security. |
| O05 | Which Linux session first? | Hyprland Wayland deep support + portable desktop preview; add X11, KDE/GNOME in phases. | Reduce integration surface. |
| O06 | Which first Windows SKU? | Windows 11 initial target; Windows 10 support explicitly evaluated, not assumed. | API support and QA. |
| O07 | macOS minimum version? | Decide after native surface spike. | Deployment compatibility. |
| O08 | `.glint` as theme only or multi-kind? | One container format with `kind`: theme/widget/wallpaper/profile etc. | Extensible ecosystem. |
| O09 | What does “Glint Hub/Studio” mean? | Reserve names for possible UI areas, do not create new repos/products yet. | Scope control. |
| O10 | Theme author native extension API? | No arbitrary native binaries in v1; approved adapters and optional later WASM. | Security. |
| O11 | Which license? | Undecided; core, SDK, library and individual themes may have different licenses. | Community contributions. |
| O12 | Do themes style arbitrary third-party apps? | Only integrations with native, supported theme engines. | Cannot safely restyle every app. |

## Anti-goals / forbidden assumptions

- Do not claim that Windows Control Panel, Windows 11 taskbar, GNOME Shell, macOS Notification Center, login/UAC/Secure Desktop or every third-party application can be fully re-skinned through one universal API.
- Do not require a central theme server, GitHub account, paid marketplace, Node toolchain for users or an always-online connection to apply installed themes.
- Do not execute arbitrary shell scripts, native binaries, privileged code or theme-supplied system hooks from imported packages.
- Do not require a theme to ship Windows/Linux/macOS implementations if its metadata explicitly limits support.
- Do not make `glint-library` the only permitted repository.
- Do not clone the full multi-GB theme library just to list it or install one package.
- Do not use proprietary Nothing branding/assets in official themes without rights; visual inspiration != affiliation.

## Document maintenance contract

Any new decision should update this register and the dependent spec. Changes to public manifests, package IDs, SDK event payloads and capability semantics require an ADR/migration entry before release. Every recommendation that remains unapproved is explicitly labeled RECOMMENDED or RESEARCH REQUIRED elsewhere.
