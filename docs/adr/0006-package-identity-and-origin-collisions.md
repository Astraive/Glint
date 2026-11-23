# ADR 0006: Package identity and cross-origin collision policy

- **Status:** Proposed
- **Date:** 2026-11-23
- **Ticket:** SCH-004
- **Supersedes:** none
- **Superseded by:** none
- **Plan label affected:** RESEARCH REQUIRED ("decide in an ADR": source-prefixed alias versus global immutable ID)
- **Plan sections affected:** [Package identifiers](../plan/04-themes/manifest-and-glint-package-spec.md#package-identifiers), [Source provenance](../plan/05-distribution/sources-and-download-strategies.md#source-provenance), [Version/lock policy](../plan/05-distribution/installer-lifecycle-and-updates.md#versionlock-policy), [Open design choices requiring evidence](../plan/06-engineering/risks-feasibility-and-tradeoffs.md#open-design-choices-requiring-evidence) (question 3)

> **Maintainer sign-off needed.** This ADR is *Proposed*. The reference rules
> are implemented as pure, side-effect-free types in `crates/glint-schema`
> (`src/identity.rs`, tests in `tests/identity.rs`) so that the policy can be
> reviewed against running tests, but nothing installs packages yet and no
> installer may treat this ADR as binding until it is accepted. The points
> marked **[sign-off]** below are the choices a maintainer must confirm or
> change.

## Context

The plan fixes three facts and leaves one choice open:

- A package is identified by (`kind`, `id`); the version is separate.
- One `id` may have several installed versions; a profile activates one.
- Two sources that publish the same `id` must **not** become equivalent or
  trusted duplicates; provenance has to be tracked. The plan offers two
  policies, "source-prefixed install alias" and "global immutable ID", and
  asks for an ADR.
- A hash lock keeps installs repeatable even when a Git branch moves.

The [threat model](../security/threat-model.md) turns this into concrete
hazards owned by SCH-004 and INS-004: B1.5 (same `id` from another origin
silently replaces an installed package), B5.2 (a package inherits another
package's grants by reusing its `id`), B8.5 (an update arrives from a
different source) and B3.6 (identity shown from the self-declared `author`).

Other contracts already key on `id` alone: profile references
(`baseTheme.id`, surface overrides), custom surface IDs
(`custom:<package-id>` in `glint.json`), the per-package renderer asset
origin and the planned grant store. Any policy that lets two packages with the
same `id` be installed side by side forces every one of those contracts to
become origin-qualified.

Still unknown: signing (no signature scheme exists yet), how registries prove
namespace ownership, and whether kinds other than `theme` will exist.

## Decision

### 1. Three levels of identity

| Level | Fields | Used for |
|---|---|---|
| **Package key** | (`kind`, `id`) | Profile references, surface IDs, the "is this installed?" question |
| **Installation** | package key + **origin** | Grants, updates, trust and display badges |
| **Release** | installation + `version` + `digest` | Lock files, the content-addressed store, rollback |

`kind` is part of the key: `theme:dev.example.a` and a future
`widget:dev.example.a` are different packages. **[sign-off]** Registries
should still refuse to reuse one `id` across kinds, because custom surface
IDs and renderer origins are not yet kind-qualified.

### 2. Origin

An origin is the canonical place a package came from. It has exactly one of
four forms, compared as exact strings after canonicalization:

| Form | Canonical text | Source type |
|---|---|---|
| Registry | `registry:https://host/path` (the registry's index URL, not its local name) | `ThemeSource::Registry` |
| Git | `git:https://host/path[#subpath]` | `ThemeSource::Git` |
| Artifact | `artifact:https://host/path` | `ThemeSource::ArtifactUrl` |
| Local | `local` | `.glint` file or local directory import |

A registry origin is its URL rather than its configured name, so renaming a
registry entry in `config.toml` does not change identity and repointing a
name at a different URL does change it.

Canonicalization only merges spellings that are certainly the same resource:
the scheme and host are lowercased and one trailing `/` is dropped.
Everything else is kept as written, including path case, percent-encoding and
a `.git` suffix. When in doubt two origins are **different**; the cost of a
false difference is one disambiguation prompt, the cost of a false match is a
silent trust transfer. Only `https` is accepted; URLs with credentials
(`user:pass@`), a query, a fragment, whitespace, or `.`/`..` segments are
refused, so secrets never reach identity or lock files. A Git subpath follows
the package-path rules of `glint.json`.

**[sign-off]** All local imports share the single origin `local`: importing a
file is always an explicit user action, and a local import has no upstream to
update from. A local import of an `id` installed from a remote origin is still
a collision.

### 3. Policy: global ID with one origin per key

This ADR chooses a **global immutable ID pinned to one origin per Glint
installation**, and rejects source-prefixed aliases for v1:

1. At most one origin may be installed for a package key at any time. All
   installed versions of that key come from that origin.
2. Installing a key that is not installed is allowed (`New`).
3. Installing a release whose origin, version and digest all equal an
   installed release is a no-op (`AlreadyInstalled`); importing the same
   package twice is idempotent.
4. Installing a new version from the **same** origin adds a version
   (`AdditionalVersion`); older versions stay until no profile lock needs them
   or the user removes them. Activation is a separate, explicit step.
5. Installing a version that is already installed from the same origin with
   a **different digest** is refused (`DigestConflict`). A published version
   is immutable; a different digest means a moved tag, a re-published
   artifact or tampering. The installed release is never overwritten.
6. Installing a key from a **different origin** is refused
   (`OriginCollision`) whatever the version or digest, even when the digests
   are equal: identical bytes from another origin still carry another
   origin's update channel and trust. The installed package is left
   untouched.
7. An update is only ever looked for at the recorded origin (B8.5). An
   "update" offered by another origin is case 6.
8. The only way past an `OriginCollision` is an explicit user decision,
   presented with both origins and digests: **keep** the installed origin
   (default) or **replace** it. **[sign-off]** Replacing an origin is
   treated as uninstalling one package and installing another: no grant,
   approval or update channel carries over, the new origin's permissions are
   requested from scratch, profile locks that pin the old origin fail
   verification until the user re-locks them, and old-origin releases are
   kept until no lock references them.
9. Grants are bound to the installation (key and origin) and to the
   version and digest they were approved for, never to `id` alone (B5.2).
   The grant store itself is CAP-004's.
10. Displayed identity comes from the origin, never from the manifest's
    `author` field (B3.6).

### 4. Hash locks

A lock entry pins a release: `kind`, `id`, `version`, `origin` and
`digest` (`sha256:` plus 64 lowercase hex digits of the verified archive).
All five must match an installed release for the lock to resolve; a lock
entry without a digest is invalid. A lock never stores a branch name or a
version range, so a moving Git branch or a re-published registry entry cannot
change what a locked profile loads. The profile lock format is SCH-006's.

### Test cases for INS-004

Each row is implemented as a unit test of `classify_install` in
`crates/glint-schema/tests/identity.rs`; INS-004 must reproduce them against
the real installed inventory, with the installed state checked unchanged
after every refusal.

| # | Installed | Candidate | Expected outcome |
|---|---|---|---|
| 1 | nothing | `theme:dev.example.a` 1.0.0 from `registry:https://r.example/index` | `New` |
| 2 | that release | the same release again | `AlreadyInstalled` (idempotent) |
| 3 | 1.0.0 from origin R | 1.1.0 from R | `AdditionalVersion`; 1.0.0 kept |
| 4 | 1.0.0 from R, digest D1 | 1.0.0 from R, digest D2 | `DigestConflict`; D1 kept |
| 5 | 1.0.0 from R | 2.0.0 from Git origin G | `OriginCollision` |
| 6 | 1.0.0 from R, digest D1 | 1.0.0 from G, digest D1 | `OriginCollision` (equal bytes are not a trusted duplicate) |
| 7 | 1.0.0 from R | 1.0.0 from `local` | `OriginCollision` |
| 8 | `local` 1.0.0 | `local` 1.1.0 | `AdditionalVersion` |
| 9 | `theme:dev.example.a` | `widget:dev.example.a` (future kind) | `New` (kind is part of the key) |
| 10 | Git `https://Host.example/u/r/` | Git `https://host.example/u/r` | same origin |
| 11 | Git `https://host.example/u/r` | Git `https://host.example/u/r.git` | different origins, so `OriginCollision` |
| 12 | (none) | origin `https://user:token@host.example/r` | refused before classification: invalid origin |
| 13 | releases of `dev.example.b` only | `dev.example.a` | `New` (other keys are irrelevant) |

## Consequences

- Profiles, custom surface IDs, renderer origins and grants can keep keying
  on `id`, because one installation never holds two origins for one key.
- Users who want the "same" theme from two sources cannot have both at once;
  they must pick one. This is the main cost and the reason the alias model
  stays a candidate for a later ADR.
- Installs are repeatable: lock entries pin origin and digest, and a version
  is immutable per origin.
- INS-004 implements the inventory, the refusal paths and the disambiguation
  prompt on top of `classify_install`; CAP-004 binds grants to installations;
  MGR-004/MGR-005 display origin-derived identity; SCH-006 uses
  `PackageRelease` as its lock entry.
- Code changed with this ADR: `crates/glint-schema` (`identity` module and
  tests), [module boundaries](../architecture/module-boundaries.md) and the
  changelog. No plan document is edited.

## Alternatives considered

- **Source-prefixed install aliases** (`community/carbon` and
  `friends/carbon` installed side by side). Allows coexistence, but every
  `id`-keyed contract (profile references, `custom:<id>` surfaces, renderer
  origins, grants, CLI `glint theme use <id>`) would need an alias, and the
  alias depends on the user's local registry names, so profiles would stop
  being portable between machines. Deferred: it can be added later without
  breaking this policy, because today's single-origin state is the special
  case of one alias per key.
- **Global ID, first origin trusted, others merged as mirrors.** Simplest,
  but it is exactly the trusted-duplicate assumption the plan forbids (B1.5,
  B8.5).
- **Global ID, last install wins.** Silent replacement; rejected for the same
  reason and because it destroys the known-good package.
- **Identity by digest only.** Repeatable, but there would be no stable
  name for profiles or updates, and every release would look like a new
  package to the user.
- **Origin by registry name** (`registry:community`). A user editing the
  registry URL behind a name would move trust silently; the URL is used
  instead.

## Migration notes

No released format exists, so nothing migrates. This ADR adds rules for the
installed inventory and the profile lock (SCH-006); it does not change
`glint.json` v1 (`id` syntax and `kind` are unchanged). If an alias model is
adopted later it needs its own ADR that supersedes section 3.
