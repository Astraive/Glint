# Backlog validation and external layout

`scripts/validate-backlog.py` is a Python 3.12+ standard-library-only CLI. It validates Glint's local planning backlog against the frozen planning documents in `docs/plan/` and can write deterministic derived reports. It makes no network calls, spawns no subprocesses, never edits tickets or source documents, and creates no GitHub issues, labels, milestones or project items.

## External directory layout

The backlog and its reports live outside both Git repositories (`Glint` and `glint-library`) so unpublished planning data is never committed:

```text
<workspace>/                      # e.g. E:\astraive\glint
├── Glint/                        # this repository (--source-root, default: script's repo)
│   ├── docs/plan/                # frozen planning documents (45 Markdown files)
│   ├── .github/project-management/{labels,milestones,project}.json
│   └── scripts/validate-backlog.py
└── .issues/                      # external backlog (--backlog-root, default: ../.issues)
    ├── schema.json
    ├── glint/<ID>/{issue.json,CONTENT.md}
    ├── glint-library/<ID>/{issue.json,CONTENT.md}
    ├── coverage/*.json
    ├── labels.json, milestones.json, project.json   # byte-identical copies of the canonical registries
    └── reports/                  # written only with --write-reports
```

Isolation rules:

- A `--backlog-root` inside the source root, inside any sibling `glint`/`glint-library` checkout, or inside any directory containing `.git` is rejected, as is a backlog root that contains the source repository.
- Symlinked backlog roots, ticket directories, JSON files, report directories and link targets are refused. Empty JSON, duplicate JSON keys and nonfinite numbers fail.
- Error locations use POSIX `/` separators on every platform.

## Source inventory

The validator embeds the 45 original planning documents under `docs/plan/` (README, INDEX, `00-history` … `08-research`, and `docs/superpowers/{specs,plans}`). Two files are deliberately excluded: `docs/plan/ALL-IN-ONE-GLINT-DUMP.md` (a derived concatenation) and `docs/plan/MANIFEST.json` (not Markdown). Adding a new Markdown file under `docs/plan/` fails validation until the inventory is updated on purpose.

## What is checked

- **Schema:** every `issue.json` against `schema.json` (repositories `glint`/`glint-library`, milestones `M0`–`M5`/`L0`–`L3`, platforms `all, windows, macos, linux, wayland, x11, hyprland, kde, gnome, sway, cli`). Atomic tasks and gates are at most size `L` and five person-days; S/M/L/XL effort limits are 1/3/5/8.
- **Body:** mandatory `CONTENT.md` headings in order-independent form (Metadata through Tracking), H1/title agreement, metadata agreement, numbered requirements, one checkbox per acceptance criterion, test-plan agreement, local-only publication text, the planned project title `Glint — Master Development Roadmap` (no numbered project URL, because the project does not exist yet), an evidence location, the master roadmap ID, and every proposed path.
- **Links and sources:** each `sources[]` entry names an exact heading in an original document, and References carries a working relative link (`../../../Glint/docs/plan/<path>.md#<anchor>`) whose anchor follows GitHub slug rules. Remote GitHub issue/PR links are forbidden.
- **Graph:** unique IDs and titles, same-repository parent trees, epics parented to their roadmap, gates without children, non-empty containers, exactly one gate per milestone (`QA-Mn`, `LQA-Ln`), no aggregate hard blockers, no cycles, no transitively redundant blockers, and Depends On / Blocks / Related sections that match the JSON graph. Cross-repository blockers are allowed.
- **Coverage:** every original document is attested as fully read and has at least one requirement mapping; each mapping's issues cite the mapping's exact section; every task and gate is mapped; every plan work unit (`FND-T1..5`, `RT-T1..5`, `PA-T1..4`, `LD-T1..5`, `WB-A1..F4`, `SP-1..7`) has an actionable owner.
- **Registries:** every ticket's priority/size/type/kind/area/platform label exists in `labels.json`; no label color is a green hue (brand rule); every milestone has a repository and `target_release`; ticket `phase` equals the milestone's phase and is a `Phase` option in `project.json`; `project.json` declares 18 views (17 tables, 1 board) whose grouping/sort fields exist.

A structural pass cannot prove that a person read a document or that any proposed test was run.

## Usage

```console
python scripts/validate-backlog.py
python scripts/validate-backlog.py --backlog-root ../.issues --source-root . --json
python scripts/validate-backlog.py --write-reports
```

Exit `0`: all checks passed (and reports were written safely when requested). Exit `1`: validation errors, or a missing, symlinked or in-repository backlog root. `scripts/publish-issues.ps1` runs the read-only check and requires exit `0` before any dry run.

## Derived reports

`--write-reports` writes only under `<backlog-root>/reports/`, through exclusive temporary files and atomic replacement:

- `backlog-audit.json` — counts and sorted validation errors.
- `backlog-creation-ledger.json` — local objects with `null` GitHub identifiers.
- `backlog-requirement-traceability.md` — source sections, owning tickets and evidence to obtain.
- `backlog-dependency-graph.md` — hard edges, cross-repository edges, the effort-weighted critical path (the derived `path:critical` set), the longest chain into each gate, and tasks that do not block their own milestone gate (for example hardware-pending Linux/macOS verification).
- `backlog-inventory.md` — ticket inventory, per-milestone effort and priority counts, plan work-unit ownership, and document read/mapping counts.

Invalid backlogs still produce reports, marked invalid. Reports carry no timestamps or workstation paths.

## CI behavior

A fresh clone has no sibling `.issues` directory, so repository CI does not run backlog validation and never commits reports. Run the validator locally against the external workspace.
