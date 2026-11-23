# Theme publishing workflow and creator ecosystem

## Independent publishing is a first-class feature

No Glint-owned account, approval process or GitHub organization membership is needed to publish. Authors choose:

1. **Single-theme Git repository:** put `glint.json` and compiled output at repo root; users install directly from URL/ref.
2. **Multi-theme custom registry:** use `registry.json` plus theme subfolders; users add the registry to Glint.
3. **Portable `.glint` file:** release on any site, send peer-to-peer, install offline.
4. **Official `glint-library` submission:** optional PR for discovery and curation.

These are **distribution formats**, not separate runtime types.

## Creator checklist

- Unique package ID/semver and explicit author/license.
- Every published surface entrypoint built and locally tested.
- `requiredCapabilities` / optional features accurately declared.
- Windows/Linux/macOS claims reflect actual tests or clearly documented fallbacks.
- All runtime assets bundled; package functions offline by default.
- No arbitrary installation scripts or network-loaded executable JS.
- Screenshots show actual running Glint surface, not misleading OS-native screenshots.
- Accessibility checks: keyboard, labels, contrast, reduced motion.
- Include changelog, known limitations, update origin, rollback guidance.
- Sign Git tags or packages when a supported signing scheme exists.

## Official library PR checks

- Validate manifests with supported schema versions.
- Build React theme in clean CI, then ensure emitted package is deterministic enough to compare digests.
- Static audit for restricted APIs and remote script references.
- Inspect images/fonts licenses and total package size.
- Render preview with mocked data across requested viewport sizes.
- Manual reviewer checks theme quality, security concerns and misleading claims.
- Generated index sorted/versioned, package digest updated.

## Creator ergonomics

A `create-glint-theme` scaffold should produce sensible defaults and a working panel/launcher demo. Documentation should include complete recipes for multi-monitor, no-audio hardware, capability failure, custom widgets, system stats, animations, packaged fonts, localization and creating an archive without maintaining a Git repo.

## Marketplace (future, optional)

A commercial “Hub”/marketplace is not necessary for MVP and **must not become a requirement** for independent themes. If introduced, it may be an additional index and purchasing channel, with clear trust/publisher policies and no lock-in to Glint's official library.
