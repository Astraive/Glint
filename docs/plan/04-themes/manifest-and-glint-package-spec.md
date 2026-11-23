# `.glint` package specification — proposed v1

**Status:** RECOMMENDED, not a frozen published standard. Goal: one portable import/export format that works offline and independent of any Git registry.

## Envelope

- Extension: `.glint`.
- Physical container: ZIP archive with regular file entries, UTF-8 paths and a root `glint.json`.
- `schemaVersion: 1`; `kind` value e.g. `theme`, with future potential `widget`, `icon-pack`, `wallpaper-pack`, `profile`.
- Distinct `id` (recommended reverse-DNS or URL-safe namespace), semantic `version`, `name`, `author`, license, compatible Glint SDK/version and compatible operating systems.
- All referenced runtime assets must exist **inside archive**. Installation must not require network. Original repository URL may be stored as metadata but is not required to run package.
- A built programmable theme ships **compiled static assets**. Node/npm are author tools only, not consumer prerequisites.

## Example root manifest

```json
{
  "schemaVersion": 1,
  "kind": "theme",
  "id": "dev.example.midnight",
  "name": "Midnight",
  "version": "1.0.0",
  "author": { "name": "Example Developer" },
  "description": "Monochrome Glint desktop theme",
  "license": "MIT",
  "compatibility": {
    "glint": ">=0.1.0",
    "sdk": "^0.1.0",
    "platforms": ["windows", "linux", "macos"]
  },
  "renderer": { "kind": "glint-web", "entrypointVersion": 1 },
  "requiredCapabilities": ["surface.launcher"],
  "optionalCapabilities": ["desktop.workspaces.read", "system.audio.write"],
  "tokens": "theme/tokens.json",
  "assets": { "wallpaper": "assets/wallpaper.png", "preview": "preview.png" },
  "layouts": { "default": "layouts/default.json" },
  "surfaces": {
    "panel": { "entry": "dist/panel/index.html", "placement": { "anchor": "top", "height": 48 } },
    "launcher": { "entry": "dist/launcher/index.html", "placement": { "kind": "floating", "position": "center" } },
    "control-center": { "entry": "dist/control-center/index.html", "placement": { "kind": "popup", "anchor": "top-right" } }
  }
}
```

The `renderer`, `surfaces` and `compatibility` schema shown is a proposal. Future JSON Schema should forbid unknown security-sensitive fields and clearly specify forward-compatibility handling.

## Package directory

```text
midnight.glint [ZIP]
├── glint.json
├── theme/tokens.json
├── dist/
│   ├── panel/index.html, assets/*
│   ├── launcher/index.html, assets/*
│   └── control-center/index.html, assets/*
├── layouts/default.json
├── assets/wallpaper.png
├── preview.png
├── LICENSE
└── package-integrity.json    # optional integrity metadata
```

## Manifest validation rules

1. Root `glint.json` must parse under supported schema version and have required fields.
2. Paths must be relative, normalized and remain inside package root. Reject `..`, absolute paths, UNC/device paths, symlinks/hardlinks unless verified as inert and safe (recommended: disallow v1).
3. Limit compressed/uncompressed size, file count, entry size, duplicate file names, compression ratio and path depth. Defend against ZIP bombs and namespace collisions/case folding.
4. Verify every entry referenced by manifest and disallow external script dependencies in compiled UI entrypoints by default.
5. Check content hashes and expected count; optionally verify signed publisher provenance in a later trust model.
6. Validate `compatibility`, required features, permissions and surface entrypoint contract before enabling.
7. Reject installation shell scripts and native executables; treat themes as **data and sandboxed UI** only.
8. Include all fonts/icons/images legally licensed for redistribution; official library CI can lint licenses.

## Package identifiers

- Package identity: (`kind`, `id`); version is separate.
- One `id` may have multiple installed versions, only one active in a profile.
- Different registry sources claiming same ID do **not** automatically become equivalent/trusted; provenance and author identity must be tracked. Potential policy: source-prefixed install alias vs global immutable ID; decide in an ADR.
- Hash lock makes installs repeatable even if Git branch updates.

## Import behavior

1. Identify archive by magic bytes, not `.glint` extension alone.
2. Stage in temp directory with safe extraction.
3. Verify manifest/schema and integrity; run static content restrictions.
4. Present author/origin/permissions/compatibility; do not activate silently.
5. Persist immutable package and installation metadata atomically.
6. Open preview or allow applying to current profile.

## Export behavior

Export selected installed theme version with bundled dependencies/assets, stripping local/private tokens, user files or credentials. Optionally export a **profile** as a separate `kind: profile` package that references/embeds themes only with licensing and consent. Standard default export should not capture private state.

## Evolution/versioning

Keep package format `schemaVersion` and runtime `entrypointVersion` independent. Add migrations only when semantics are well defined. Never auto-run a “migration script” from an untrusted package. Maintain fixtures for invalid archives and backward compatibility across Glint versions.
