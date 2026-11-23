# `glint` monorepo layout and package contracts

## Reference repository structure

```text
glint/
├── apps/
│   ├── desktop/                 # Tauri 2 manager, React/TS frontend
│   ├── daemon/                  # per-user Rust background service binary
│   ├── shell/                   # native surface hosts/renderer supervisors
│   └── cli/                     # glint CLI; consumer and creator commands
├── crates/
│   ├── glint-core/
│   ├── glint-platform/          # traits, normalized capabilities/errors
│   ├── glint-windows/
│   ├── glint-linux/
│   ├── glint-macos/
│   ├── glint-surfaces/
│   ├── glint-renderer/
│   ├── glint-theme/
│   ├── glint-package/
│   ├── glint-sources/
│   ├── glint-registry/
│   ├── glint-installer/
│   ├── glint-storage/
│   ├── glint-system/
│   ├── glint-events/
│   ├── glint-permissions/
│   ├── glint-ipc/
│   ├── glint-assets/
│   └── glint-schema/
├── packages/
│   ├── sdk/                     # @glint/sdk and React hooks
│   ├── theme-runtime/           # host bridge and bootstrap
│   ├── theme-schema/            # JSON schema/TS helpers
│   ├── ui/                      # optional component library
│   ├── types/                   # generated contracts
│   └── create-glint-theme/      # creator template package
├── platform/
│   ├── windows/                # packaging, native assets, OS scripts
│   ├── linux/                  # desktop files, layer/DE assets, packaging
│   └── macos/                  # plist, entitlement, app bundle assets
├── themes/
│   └── default/                # bundled fallback theme
├── examples/
│   ├── declarative-theme/
│   ├── react-theme/
│   └── widget/
├── schemas/
├── docs/
│   └── superpowers/
│       ├── specs/
│       └── plans/
├── scripts/
├── tests/
│   ├── integration/
│   ├── package-fixtures/
│   └── e2e/
├── Cargo.toml
├── pnpm-workspace.yaml
└── README.md
```

## Separation of responsibilities

`crates/` contains compiled Rust source. `platform/` contains integration resources, packaging and scripts—not a duplicate implementation of `glint-{windows,linux,macos}`. `packages/` contains JS/TS workspace modules and build templates, not platform-native trusted operations. `apps/` contains binaries and their very thin bootstrap. `themes/default` is a proper package using the public SDK where feasible, forcing Glint to dogfood its own extension APIs.

## Rust trait contracts

- `PlatformAdapter`: discovery and access to capability-specific traits; no giant method bag.
- `WallpaperBackend`: get/set/restore wallpaper per display, with clear restore capability.
- `SurfaceBackend`: create/show/hide/destroy native surface; exposes placement support.
- `AudioBackend`, `NetworkBackend`, `WorkspaceBackend`, `ApplicationBackend`: read/actions and watcher subscriptions with error semantics.
- `PackageSource`: resolve/download/read package into staging with source/provenance.
- `PackageValidator`: archive + manifest integrity and compatibility.
- `ProfileResolver`: produce a desired composition plan independent of OS.

## CI/repository policy

- Rust format/lint/test; TypeScript typecheck/lint/test; schema generation consistency.
- Feature-gated platform compilation and matrix builds on Windows/macOS/Linux.
- No platform-specific crates referenced from platform-independent core modules.
- Golden fixture tests shared between Rust package parser and TypeScript schema.
- Lock dependencies, check SBOM/license compatibility and sign release artifacts when project is production-ready.

## Example end-to-end component chain

`glint-desktop` UI calls `glint-ipc` -> `glint-core.apply_profile()` -> `glint-theme.resolve()` -> `glint-platform` capabilities -> `glint-surfaces` host registry -> `glint-renderer` with `@glint/sdk` bridge. Source management happens via `glint-sources` / `glint-package` / `glint-installer`, not from React.

## Repo boundaries

Separate repo `glint-library` is a distribution dataset, not a Git submodule dependency. Glint should compile/run with its built-in default theme even if `glint-library` is offline or deleted. Registry config is a default URL, not a hardcoded installer assumption.
