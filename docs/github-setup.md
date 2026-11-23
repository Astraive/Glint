# GitHub Repository and Project Setup

This repository keeps a **static configuration definition** in `.github/project-management/`. The versioned files contain label definitions, milestone goals and gate summaries, project field definitions, and project view definitions. They do not contain issue bodies, `issue.json`, coverage or export data, per-issue work items, ticket membership, or derived ticket-ID lists. The expanded planning model lives outside Git in `../.issues`; the issue publisher (see [backlog publication](backlog-publication.md)) reads it for its separate publication workflow. This configuration tool does not invoke the publisher and does not create or update issues, project items, or drafts.

The registries are definitions, not a shadow planning model or a snapshot of project items. Milestone summaries intentionally omit ticket-membership lists, and `effort_days` is `null` until the backlog sizes tickets. `path:critical` is a static derived-label rule, not a committed list of its members. No calendar dates, completed work, native issue relationships, or platform-support claims are inferred by these files.

## What is registered

| Registry | File | Size |
|---|---|---|
| Labels | `.github/project-management/labels.json` | 74 |
| Milestones | `.github/project-management/milestones.json` | 10 (6 in `Glint`: M0-M5; 4 in `glint-library`: L0-L3) |
| Project fields and views | `.github/project-management/project.json` | 20 fields, 18 views |

The label registry avoids green hues (brand rule); the colors are recorded in the file itself.

Two repositories share one organization project:

- `Astraive/Glint` - the main planning and (later) implementation repository.
- `Astraive/glint-library` - the official public theme library. It is an optional source; independent Git repositories and `.glint` files remain first-class.

## The project does not exist yet

The GitHub Projects v2 board `Glint — Master Development Roadmap` has not been created, so `project.json` records `project.id` and `project.number` as `null`. The configuration tool handles this explicitly:

- Offline plan: lists a planned `project:create` operation and plans fields, views, and links against a "project not yet created" placeholder.
- `-Apply`: looks the project up by registry id (or by exact title if the id is `null`) under organization `Astraive`. If it is absent it creates it (`createProjectV2`), then links both repositories, creates the fields, and creates the views. If a registry id is set but not found, it refuses to create a replacement.
- `-VerifyOnly`: never creates anything. A missing project is reported as a pending operation and a failure.

After the project is created, record the observed `id` and `number` in `project.json` so later runs find it directly. New projects are private; do not change visibility through this tool.

## Safe configuration model

`scripts/configure-github.ps1` is a PowerShell 7 tool limited to repository settings, topics, security feature settings, labels, milestones, branch protection, and the Project V2 creation, title, repository links, fields, and views. It never calls the issue publisher. Its GitHub requests pass an allowlist: it does not change repository visibility, names, commit history, issue metadata, issue labels, project items or drafts, or native issue relationships, and it preserves unrelated label and topic objects.

The repositories must be public and MIT-licensed; the script refuses to proceed otherwise. Security feature settings are requested as enabled; GitHub may reject features unavailable for the repository, plan, or authorization, and such rejections are reported as failures rather than success. Each mutation is followed by a refetch where the API supports readback, and any unsuccessful readback is recorded as a failure. Mutations are paced, reads are bounded by the command timeout, and mutations are not retried blindly. A failed or interrupted apply may leave some infrastructure changes applied; inspect the JSON audit report and run verification before deciding what to do next.

## Offline plan and audit report

From the repository root, an ordinary invocation is offline: it reads only the checked-in static registries and proposes operations without contacting GitHub or writing a report inside the repository.

```powershell
pwsh -NoProfile -File scripts/configure-github.ps1
```

The default audit report is `../.issues/reports/github-config-audit.json`, outside the repository. Use `-ReportPath` to choose another location; the tool refuses paths inside either Git repository or behind symlinks. The report records operations, intended values, pending manual configuration, and failures; it does not contain issue content or ticket membership.

## Authorized apply

Apply is an explicit, separate action and requires an authenticated GitHub CLI (`gh auth status`) with organization project permissions:

```powershell
pwsh -NoProfile -File scripts/configure-github.ps1 -Apply
```

Apply only from an authorized maintainer session after reviewing the offline plan. It requests the repository description and topics, merge defaults (squash merge, delete merged branches, no required sign-off), enables discussions and available security protections, restricts Actions workflow tokens to read-only, upserts the 74 labels in both repositories and the milestones in their designated repositories, creates or locates and links the shared project, and upserts project fields and views. Once issues and customized milestone descriptions or manual UI view layouts exist, avoid re-running a full `-Apply` unless an infrastructure reset is intended, because it overwrites native milestone descriptions with the static summaries; use read-only `-VerifyOnly` for ongoing audits.

Branch protection is excluded unless the maintainer supplies the exact names of checks observed green on the actual CI workflows:

```powershell
& ./scripts/configure-github.ps1 -Apply -ConfigureBranchProtection `
  -RequiredCheck @('Repository quality', 'Python security analysis')
```

The `Rust workspace` matrix job is skipped until a root `Cargo.toml` exists, so it must not be listed as a required check until it has run. Protection applies to both `main` branches and preserves additional existing required checks and push restrictions. It requires strict up-to-date checks, administrator enforcement, zero approvals for single-maintainer bootstrap, dismissal of stale approvals, no force pushes or deletions, linear history, and resolved conversations. Signing and code-owner approval remain unenforced until their prerequisites exist.

## Verification and limitations

`-VerifyOnly` performs read-only API calls, writes an external audit report, and exits nonzero if any requested state differs, any required native field is absent, or a manual-only setting remains pending. Do not combine it with `-Apply`.

```powershell
pwsh -NoProfile -File scripts/configure-github.ps1 -VerifyOnly
```

The Project V2 GraphQL API supports `name`, `layout`, `filter`, and `configuration: { visibleFieldIds }` on view create and update, but exposes no public setters for grouping, sorting, or `Slice by`. For readback, `ProjectV2View` exposes `groupByFields`, `verticalGroupByFields`, and ordered `sortByFields`, but not `Slice by`:

- **Board (`BOARD_LAYOUT`, `Master Roadmap`):** columns come from `verticalGroupByFields` (`group_by`: `Phase`), swimlanes from `groupByFields` (`secondary_group_by`: `Status`), `slice_by` is `null`, and `sort_by` is `["Priority", "Milestone"]`.
- **Tables (`TABLE_LAYOUT`, the other 17 views):** one grouping field (`groupByFields` mapped to `group_by`), `secondary_group_by` is `null`, the secondary dimension lives in the UI **Slice by** sidebar (`slice_by`), and two ordered `sort_by` fields apply.

In `-VerifyOnly` mode the tool normalizes those layout-specific vectors separately. `view-api:<view>` compares grouping and ordered sort fields and is `verified` only on exact match. `view-slice:<view>` is always `manual-only-unverifiable-by-api`, because the public API cannot read `Slice by`; `-VerifyOnly` therefore exits nonzero until a maintainer has checked those settings in the GitHub UI. Grouping, sorting, and Slice by for the 18 views have not been configured or verified: the project does not exist yet.

Existing select-field options are merged with the desired options while preserving their IDs, colors, and descriptions. If the project has zero items, `Todo` is renamed to `Backlog` keeping its option ID; when items exist, legacy `Todo` is retained and the migration is recorded. `Platform` is a real multi-select field with `all`, `windows`, `macos`, `linux`, `wayland`, `x11`, `hyprland`, `kde`, `gnome`, `sway`, `cli`. GitHub reserves the field name `Type`, so detailed classification uses `Work type` with the 14 `type:*` values.

Native Repository, Milestone, Assignees, Title, Labels, Parent issue, Sub-issues progress, and Updated remain native; no custom shadows are created. Publishing issues and their native relationships is the job of the [issue publisher](backlog-publication.md).

## Status of this document

The offline plan has been run and writes its report outside the repository. The online paths (`-Apply`, `-VerifyOnly`), including project creation, have not been executed against GitHub. Treat them as unverified until a maintainer has run them and reviewed the audit report.
