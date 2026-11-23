# Glint glossary and conceptual model

**Glint-managed surface:** Window/desktop UI region whose content and lifecycle are owned by Glint (panel, popup launcher, OSD, control-center, notification overlay, custom desktop area). Author controls its DOM/layout; Glint controls host placement, focus, input, and permissions.

**OS-owned surface:** UI managed by Windows Explorer/DWM, GNOME Shell, KDE Plasma, macOS Dock/Menu Bar/Notification Center, a login manager, or an arbitrary app. Glint can only influence it through documented/supported integrations; the theme runtime has no direct ownership.

**Theme:** Versioned package that specifies tokens, assets, surface implementations, layouts and optional supported integration requests.

**Declarative theme:** Data-only theme; modifies presets/tokens/layouts of shipped UI components. Requires no code compilation for the creator.

**Programmable theme:** Theme with prebuilt JS/CSS UI bundles (recommended v1: React/TypeScript source), running inside restricted renderers. Consumers need no Node toolchain.

**Widget:** Self-contained visual module mounted within a surface, e.g., a clock, weather card or music player. Several widgets may share one renderer.

**Surface host:** Native OS window/layer application managing position, z-order, display and input for theme-rendered UI. Can be multiple host processes or windows.

**Platform adapter:** Rust code implementing a specific OS or compositor capability: wallpaper, audio, brightness, workspaces, app enumeration, panels, notifications etc.

**Capability:** A runtime guarantee/degree of support for one operation; more precise than a single bool (supported, read-only, unavailable, needs permission, experimental, etc.).

**Permission:** A user-granted rule allowing a theme/extension access to approved SDK methods, data or external network/file resources.

**Profile:** User's saved set of chosen theme versions, surface overrides, widget layouts, monitor mappings and native adaptation settings; can mix themes.

**Official library (`glint-library`):** Project-maintained public Git repository holding official curated themes and an index. **Optional to use**.

**Registry:** Index of available themes and metadata. Official library is one registry; other registries may be independent.

**Theme source:** One of official/custom registry, direct Git URL/ref/subpath, local source directory, local `.glint`, or downloadable `.glint` artifact.

**`.glint` package:** Portable, versioned package envelope. Recommended v1: ZIP format, root `glint.json`, compiled assets and no implicit online dependencies.

**Provider:** Daemon-managed system data source (network, audio, workspaces, media) with a shared subscription/refcount rather than one native watcher per widget.

**Theme SDK:** Namespaced, permission-aware API accessible to JS UI components through a narrow IPC bridge.

**Snapshot / restore point:** Journal of previous Glint state and selected OS state, used for best-effort rollback. Does not imply an atomic OS transaction.

**Preview mode:** Restricted renderer with simulated system data, safe display-only surface and no native setting changes.

**Fallback:** Built-in default implementation used when a theme does not provide a surface or a host cannot safely realize it on that OS.

**Integration adapter:** An optional module that applies compatible native system theming (e.g., KDE global themes, gtk theme preference), distinct from executing theme code.

**Per-user daemon:** Background process bound to the logged-in user's desktop session, not a privileged machine-wide service by default.

**Nothing-inspired:** A visual reference: monochrome, sharp geometry, glyph/dot motifs. It is not a license to redistribute trademarks, licensed fonts, sounds or vendor system assets.
