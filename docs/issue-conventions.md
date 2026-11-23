# Backlog and issue conventions

> **Publication state:** Canonical authoring records stay external in `../.issues/`, even after an authorized batch is published. Remote identities and verification belong in the separate publication ledger. Stable IDs such as `PRJ-001` and `QA-M0` are not GitHub issue numbers.

## External backlog isolation and publication status

Planning records (`issue.json`, `CONTENT.md`, coverage slices, and derived reports) live outside both Git repositories in `../.issues/` so raw backlog inventories are never committed or pushed. Tracked repository files hold only static registry definitions under [`.github/project-management/`](../.github/project-management/project.json), validation/publication tooling, and these summary guides.

| Surface | Repository tracked (`.github/project-management/`) | External workspace (`../.issues/`) |
|---|---|---|
| Purpose | Static labels, milestone goals and gates, project field and view schema | Canonical ticket records, coverage slices, derived reports |
| Ticket bodies and membership | Excluded intentionally | Stored in `../.issues/glint/<ID>/` and `../.issues/glint-library/<ID>/` |
| Publication status | Infrastructure configuration only via [configure-github.ps1](../scripts/configure-github.ps1) | Authorized batches via [publish-issues.ps1](../scripts/publish-issues.ps1) with explicit milestone scope and account checks; actual results recorded in the publication ledger |

See [backlog validation](backlog-validation.md), [backlog publication](backlog-publication.md), and [GitHub setup](github-setup.md) for isolation rules, validation guarantees, and project configuration details.

## Stable ticket identifiers and hierarchy

Tickets use globally unique uppercase `PREFIX-NNN` (or `PREFIX-EPIC`, `ROADMAP-001`, `QA-M*`, `LQA-L*`) identifiers organized into a tree (`roadmap -> epic -> task | gate`):

- **Roadmaps (`kind:roadmap`):** `ROADMAP-001` (`glint`, `M0`) and `LIB-ROADMAP-001` (`glint-library`, `L0`).
- **Epics (`kind:epic`):** one `<PREFIX>-EPIC` per domain prefix.
- **Atomic implementation tasks (`kind:task`):** sized `S`/`M`/`L` (at most 5 person-days each).
- **Milestone gates (`kind:gate`):** `QA-M0` through `QA-M5` for `glint` and `LQA-L0` through `LQA-L3` for `glint-library`, one evidence-bearing gate per milestone.

Prefixes in `glint` map to one area label each: `PRJ` repository, `SPK` feasibility, `SCH` manifest-schema, `PKG` package-format, `SRC` sources, `INS` installer, `COR` core, `CAP` capabilities, `PRF` profiles, `SRF` surfaces, `RND` renderer, `SDK` sdk, `THM` default-theme, `CLI` cli, `CRT` creator-tooling, `MGR` manager-ui, `WIN` windows-host, `LNX` linux-host, `MAC` macos-host, `REG` registry, `SEC` security, `A11Y` accessibility, `PERF` performance, `DOC` documentation, `REL` release-engineering (`REL-009` owns dependency freshness), `QA` quality. In `glint-library`: `LIB` library-foundation, `CAT` catalog, `LTH` official-themes, `LCOM` community, `LLEG` licensing, `LQA` library-quality, `LREL` library-release.

`PRJ-001` is "Establish repository governance and GitHub infrastructure".

Hard execution blockers use explicit ticket links under `Depends On` (`depends_on` in `issue.json`), while `Related Issues` captures non-blocking soft context only.

## Shared label taxonomy

Both repositories share the canonical labels defined in [`.github/project-management/labels.json`](../.github/project-management/labels.json) (mirrored to `../.issues/labels.json`):

| Label family | Values and conventions |
|---|---|
| `priority:*` | `P0` (critical/milestone-blocking), `P1` (required), `P2` (standard), `P3` (optional/deferred) |
| `size:*` | `S`, `M`, `L`, and `XL` (**containers only**; atomic tasks must be `<= L`, at most 5 person-days) |
| `type:*` | `epic`, `feature`, `task`, `bug`, `enhancement`, `chore`, `documentation`, `test`, `research`, `security`, `refactor`, `performance`, `maintenance`, `duplicate` |
| `platform:*` | `all`, `windows`, `macos`, `linux`, `wayland`, `x11`, `hyprland`, `kde`, `gnome`, `sway`, `cli` (each ticket carries the exact applicable targets; never collapsed to one OS) |
| `kind:*` | `roadmap`, `epic`, `task`, `gate` (kept distinct from `type:*`) |
| `path:*` | `path:critical` marks tickets on the longest hard-dependency chain |
| `area:*` | One per prefix area listed above |
| other | `breaking`, `dependencies`, `github_actions` |

Label colors avoid green hues to match the Glint brand (neutral graphite, silver and white).

## Milestones and project fields

`glint` milestones `M0` through `M5` (Foundation and Feasibility; Core Package and Safe Preview; Programmable Surface Runtime; Platform Shell Integration; Official Library and Open Distribution; Power-User and Ecosystem) map to the phases in [roadmap and milestones](plan/06-engineering/roadmap-milestones-and-success.md). `glint-library` milestones are `L0` through `L3`.

The planned GitHub Project is titled "Glint — Master Development Roadmap" and covers `Astraive/Glint` and `Astraive/glint-library`. Its fields include `Status`, `Priority`, `Size`, `Work type`, `Kind`, `Area`, `Platform`, `Phase`, `Blocked`, `Target release` and `Effort`, alongside native Repository, Milestone, Assignees and parent/sub-issue fields. GitHub reserves `Type` for native issue types, so the detailed classification uses **Work type**.

- **Status values:** `Backlog`, `Ready`, `In Progress`, `In Review`, `Blocked`, `Done`.
- **Definition of Ready:** acceptance criteria and ownership are clear, required assets and fixtures are available, all hard blockers in `depends_on` are `Done`, and atomic size is `<= L`.
- **Definition of Done:** all acceptance evidence is reviewed and attached; milestone gates additionally require named owner sign-off and zero unresolved P0, security, or destructive-data defects.

## PR title and commit conventions

Enforced by [check-conventions.py](../scripts/check-conventions.py) and `.githooks/commit-msg`:

- **Required format:** `type(scope): concise subject [TICKET-ID]`
  - Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.
  - Lowercase kebab-case scope required (for example `docs(plan): fix archive links [DOC-001]`).
- **`Co-authored-by:` trailers are allowed.** Contributors still commit under their own identity.
- **Squash merges:** the PR title becomes the commit subject on `main`.
- **Dependabot PRs:** the maintainer renames the title to include `[REL-009]` (or a dedicated upgrade/security ticket) before merge.
- **Partial work vs. closure:** reference the ticket ID (`Refs [PRJ-001]`) with exact observed command evidence and affected platforms; do not claim full ticket or gate completion from partial or mocked checks.
