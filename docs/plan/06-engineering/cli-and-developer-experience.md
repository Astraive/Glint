# Glint CLI contract and developer experience

## User CLI (proposed)

```sh
glint --version
glint doctor
glint status
glint safe-mode
glint theme                   # interactive chooser when TTY
glint theme browse
glint theme search "mono"
glint theme info official/mono
glint theme install official/mono
glint theme install https://github.com/user/my-theme.git
glint theme import ~/Downloads/midnight.glint
glint theme use dev.example.midnight
glint theme list
glint theme update dev.example.midnight
glint theme export dev.example.midnight -o midnight.glint
glint theme remove dev.example.midnight
glint theme revert
glint profile list
glint profile use custom-mix
glint registry list
glint registry add custom https://github.com/user/my-library.git
glint registry sync
glint permissions list
```

All flag names are proposed. Interactive mode is only for terminals; JSON stdout with stable schemas (`--json`) should be available for scripts. Clear nonzero exit codes for unsupported operation, invalid package, insufficient permission and missing daemon.

## Creator CLI

```sh
glint theme create my-theme --template react
cd my-theme
glint theme dev
glint theme build
glint theme validate
glint theme pack
```

Suggested scaffold presets: `declarative`, `react-basic`, `react-full-shell`, `widget`. Explicit React template need not be the only future renderer. No GitHub login in creator commands.

## Dev server

1. Watches manifest, compiled bundles and theme assets.
2. Starts a sandboxed preview host with mock or read-only system data, no OS mutation by default.
3. Supports HMR at surface entrypoint level and preserves widget state where practical.
4. Shows panel/launcher/control center in virtual monitor layouts, DPI scaling and different OS capability profiles.
5. Offers inspector for package ID, surface mapping, host size, active grants and provider events.
6. Warns if theme needs capabilities absent from target OS profile.

## SDK version negotiation

`glint theme validate` compares `compatibility.glint`, `compatibility.sdk`, `entrypointVersion` and manifest version; labels future/unsupported syntax precisely. Build output should include predictable hashed chunk filenames and a generated manifest of entrypoints that package validation can check.

## Diagnostics

`glint doctor` should display OS/session/compositor, app version, daemon/shell state, system providers, renderer backend, Wayland protocol support, installed schema versions, active package locks and warnings. Provide `--redact` by default and opt-in verbose diagnostics. Never dump access tokens/SSIDs/window titles without explicit confirmation.

## GUI equivalence

Every consumer CLI operation should be accessible in manager UI (installed theme lists, source install, drag/drop import, export, update, revert), though creator build/debug tools may remain CLI-centric. GUI and CLI use the same daemon APIs for consistent state.

## Developer docs examples

Maintain runnable examples for: basic clock widget; panel with workspaces; control center with read-only and writable fallback; Linux Hyprland-specific integration; macOS-safe launcher; Windows appbar; local `.glint` import; multi-registry install; offline export; installing signed/unsigned packages with permission displays.
