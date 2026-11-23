# `glint-library` — official public repository, not a gatekeeper

## Confirmed design

Glint consists of at least **two independent repositories**:

1. `glint` — application, daemon, shell/runtime, platform adapters, SDK, packer and CLI.
2. `glint-library` — **public official theme library** curated and operated by the Glint maintainers. It is configured as the default registry source. It is *not* where everyone must publish.

Independent creators are free to host themes on their own GitHub or other Git hosts and distribute `.glint` releases without submitting to `glint-library`.

## Suggested `glint-library` layout

```text
glint-library/
├── registry.json
├── themes/
│   ├── mono/
│   │   ├── glint.json
│   │   ├── dist/
│   │   ├── assets/
│   │   └── preview.png
│   └── carbon/
├── collections/
│   ├── featured.json
│   └── minimal.json
├── previews/
├── schemas/
├── scripts/
├── .github/workflows/
│   ├── validate-themes.yml
│   └── build-registry.yml
├── CONTRIBUTING.md
├── LICENSE
└── README.md
```

If the official library grows large, **recommend** publishing prebuilt `.glint` release assets or sharded registries to avoid bloating Git history. Do not bind the package format to this repo's internal tree structure.

## Registry metadata example

```json
{
  "schemaVersion": 1,
  "registryId": "official",
  "name": "Glint Official Library",
  "packages": [
    {
      "id": "official.mono",
      "kind": "theme",
      "name": "Mono",
      "version": "1.0.0",
      "path": "themes/mono",
      "preview": "themes/mono/preview.png",
      "platforms": ["windows", "linux", "macos"],
      "integrity": "sha256:<generated-by-CI>"
    }
  ]
}
```

Registry metadata is an **index**, not a source of runtime truth. The installed package's `glint.json` must independently validate. CI should generate index fields from manifests to prevent staleness and enforce required artifact hashes.

## Default config in Glint

```toml
[registry]
default = "official"
auto_sync = true
cache_ttl = "24h"

[registries.official]
type = "git"
url = "https://github.com/<your-org>/glint-library.git"
branch = "main"

[themes]
auto_update = false
allow_external_git = true
allow_local_packages = true
```

`<your-org>` is a placeholder because the actual organization was not specified in this conversation. Glint should ship this as a **read-only bundled default**; user overrides go in a user-level config, not into installed app resources.

## Library management governance

- Public READ access to metadata and packages; community PR contributions via review.
- Require semantic versioning, manifest validation, screenshot/license checks, size limits and no install scripts.
- `featured`/`official`/`verified` have **defined meanings**; do not misrepresent a contributed theme as created by the team.
- Process removals for malicious/infringing themes; installed local packages should keep functioning offline unless users revoke them.
- README teaches independent publication too, to avoid a de facto gatekeeper.
- Use signed tags/checksums where possible, but don't rely on GitHub stars as an author security signal.

## Cache and browsing

Glint can fetch a small `registry.json` and previews without a full clone. Direct Git access may use sparse checkout/partial clone; Git hosting and credentials vary. Keep a source abstraction and fallback HTTP/download mechanisms. Metadata refresh is not the same as automatic theme upgrading.

## API for custom registries

`glint registry add my-themes https://github.com/user/collection.git` adds a discoverable index. A plain Git repo with only a root `glint.json` is a **theme source**, not automatically a multi-theme registry. Support explicit branch/ref and optional subpath when needed.

## Unresolved

- True repo URL/name for the Glint official org.
- Whether `glint-library` stores fully built theme assets or only manifests plus release URLs.
- Registry index signing/trust revocation.
- License for official themes and contribution agreement.
