# Backlog publication

The publication source is the **external workspace** `../.issues`, relative to this repository (`E:\astraive\glint\.issues` on the maintainer's machine). It holds both repositories' ticket folders (`glint/<ID>/` and `glint-library/<ID>/`, each with `issue.json` and `CONTENT.md`), `labels.json`, `milestones.json`, `project.json`, and coverage registers. It is not application code. Do not move it into either repository, commit it, or push it. Source `issue.json` files stay local authoring records: the publisher never writes GitHub numbers, URLs, project IDs, or publication status into them.

`scripts/publish-issues.ps1` is a PowerShell 7 publisher for `Astraive/Glint`, `Astraive/glint-library`, and the organization Project **Glint — Master Development Roadmap**. It is ported from the AsciiWall publisher and keeps every safety property of that tool. Publication creates planning records. It does **not** create implementations or passing gates. Bodies and the project Status keep the source Backlog/unimplemented state. Publication does not advance checkbox completion, acceptance criteria, test claims, assignees, or issue open/closed state.

No live publication has happened yet. Only an explicitly authorized `-Apply` for a named scope may write to GitHub. The external publication ledger (`../.issues/publication-ledger.json`), the read-only `-VerifyOnly` audit, and external audit snapshots record the real remote results. This guide does not.

## Modes and prerequisites

You need **PowerShell 7.5 or later** (`pwsh`), Python 3.12+ for `scripts/validate-backlog.py`, and GitHub CLI `gh`. Version 7.5 matters because `ConvertFrom-Json -DateKind String` keeps ISO-8601 timestamps such as `verification.checked_at` exactly as written when the ledger is read and saved. The separate `scripts/configure-github.ps1` tool needs only PowerShell 7.0+.

Before any GitHub access, lock creation, or ledger write, the publisher runs the backlog validator with exactly this command:

```text
python scripts/validate-backlog.py --backlog-root <external-backlog> --source-root <this-repository>
```

If the validator is missing or exits nonzero, the run stops. No report-writing flag is passed. The publisher then runs its own checks:

- the registries and every ticket's labels, milestone, and project values
- select-option limits and ownership of native fields
- `project.derived_labels`
- local Markdown link destinations
- the combined parent/dependency graph, which must have no cycles

| Mode | GitHub access | Local publication writes | Remote writes |
| --- | --- | --- | --- |
| No switch / `-DryRun` | None. Fully offline; `gh` is never invoked. | None. No ledger and no lock. | None |
| `-VerifyOnly` | Paginated REST/GraphQL reads and schema introspection | None. An existing ledger is only read. | None. Differences are reported, not repaired. |
| `-Apply` | Reads, plus explicit journaled writes | The external lock and the atomic external ledger only | Resumes managed publication. Refuses to overwrite manual body edits or roll back progress. |

The mode switches are mutually exclusive. Defaults:

- owner: `Astraive`
- Project: resolved at runtime (see the next section)
- documentation revision for a fresh ledger: `main`
- backlog: the sibling `../.issues`

`-BacklogPath` can select another external directory, but never one inside this repository or a sibling `../glint-library` checkout. The publisher rejects a backlog path that is, contains, or sits under a symlink or reparse point.

`-SourceRef` selects the documentation branch, tag, or commit in `Astraive/Glint`. Remote modes resolve it before any write, and every published `docs/...` link uses the resolved commit SHA. Apply records both the requested ref and the resolved SHA in the ledger. Later runs without `-SourceRef` use the ref pinned in the ledger, and they stop if that ref now resolves to a different SHA. An explicit `-SourceRef` that conflicts with the pinned one is rejected. DryRun stays offline and only reports the ref you passed, or `main`. The local validator checks this checkout's documents, so make sure the checkout matches the revision you publish.

```powershell
pwsh -NoProfile -File ./scripts/publish-issues.ps1 -DryRun
pwsh -NoProfile -File ./scripts/publish-issues.ps1 -BacklogPath ../.issues -DryRun -SourceRef main
```

Standard output is a JSON plan. It lists the repositories (backlog key and GitHub slug), the project title and how it will be resolved, the creation order, source and initial-body digests, declared `derived_labels`, and the labels, published title, milestone key, parent, hard blockers, and project field values for each ticket. The plan contains no invented issue numbers or URLs; real identities can only be discovered remotely. The initial-body digest describes the creation body, not the final body with resolved links. It also depends on the Project number, which DryRun does not know until that number is recorded somewhere it can read offline.

### Project resolution (the publisher never creates the Project)

`project.json` keeps `number` and `id` as `null` until `scripts/configure-github.ps1` creates the Project. Identity may sit at the top level or under a nested `project` object, as in `.github/project-management/project.json`. Remote modes resolve the existing organization Project read-only, before taking the lock or loading the ledger:

1. With `-ProjectNumber N`, or a non-null `number` in `project.json`, the publisher reads organization Project `N`.
2. Otherwise it pages through every organization Project and requires exactly one whose title matches `desired_title` exactly (case-sensitive, including the em dash).

The run fails closed with a message to run `scripts/configure-github.ps1` if no Project matches. It also fails if more than one Project matches, if the Project is closed, if its title differs from `desired_title`, or if its node ID differs from a non-null `project.json` `id`. The publisher never creates, renames, or deletes a Project. The resolved number and node ID are recorded in the ledger, and later remote runs must resolve to the same Project.

Offline DryRun reports `project_number: null` and `project_resolution: exact title at publication` unless a number comes from `-ProjectNumber`, `project.json`, or (under `-CreateMissing`) the existing ledger.

### Ticket format and tolerance

Tickets follow the AsciiWall `issue.json` schema. The publisher reads only what publication needs:

- **Required** (a missing or invalid value fails before any remote access): `id` (`[A-Z][A-Z0-9]*(-[A-Z0-9]+)+`), `title`, `repository`, `kind` (`roadmap|epic|task|gate`), `milestone` (`M<n>` or `L<n>`), `priority` (`P0`–`P3`), `size` (`S|M|L|XL`), `type`, `area`, a non-empty `platforms` array, and a positive `effort_days`.
- **Optional** (in-memory defaults only; the source file is never rewritten):
  - `parent` and `epic` default to `null`.
  - `depends_on` and `related` default to `[]`.
  - `status` defaults to `Backlog`.
  - `phase` defaults to the milestone title without its `<ID> — ` prefix, for example `Foundation & Feasibility`.
- **Ignored**: `settings_keys`, `scenario_ids`, `work_units`, `sources`, `requirements`, `test_plan`, `proposed_paths`, `github`, and any other authoring fields. They may be dropped or repurposed.
- **`repository`** may be the backlog key (`glint`, `glint-library`), the GitHub name (`Glint`), or `Astraive/Glint`. It is normalized to the backlog key, which also names the ticket's folder and prefixes ledger and milestone keys such as `glint/M0`. References in `parent` and `depends_on` accept a bare globally unique ID, `glint/ID`, `Glint/ID`, or `Astraive/glint-library/ID`.
- **Published title** is always `[ID] title`. A source title that already starts with `[ID] ` is used unchanged. A title prefixed with a *different* ID is rejected.
- **Labels** come from the ticket: `kind:`, `priority:`, `size:`, `type:`, `area:`, one `platform:` label per platform, plus any declared derived label. Every label must exist in `labels.json`.
- **Project fields**: `Status`, `Priority`, `Size`, `Work type`, `Kind`, `Area`, `Platform` (multi-select), and `Phase` come from the ticket. `Effort` comes from `effort_days`. `Target release` is the milestone's `target_release`, which is required. `Blocked` is `Yes` while any hard blocker is still open. `Repo role` comes from the field's `role_map` (or `roles`) map; otherwise it is `Main App` for `glint` and `Theme Library` for `glint-library`. Every select value must be one of the field's declared options.
- **Milestones** without a `description` get a deterministic one built from `goal`, `exit_conditions`, `gate`, and `target_release`, marked as planning only.

### Publish only an authorized milestone

`-Milestone` takes qualified registry keys such as `glint/M0` or `glint-library/L0`. The whole source model is still validated, but only matching tickets and the roadmap/epic ancestors they need are reconciled. An ancestor keeps its own milestone: it is a tracking container, not permission to publish its later tasks. A missing hard dependency fails before any remote access. Add its milestone explicitly instead; later work is never published silently. Pass several keys as a PowerShell array, for example `-Milestone glint/M0,glint-library/L0`.

```powershell
pwsh -NoProfile -File scripts/publish-issues.ps1 -DryRun -Milestone glint/M0
pwsh -NoProfile -File scripts/publish-issues.ps1 -VerifyOnly -Milestone glint/M0 -ExpectedLogin NeerajCodz
pwsh -NoProfile -File scripts/publish-issues.ps1 -Apply -Milestone glint/M0 -ExpectedLogin NeerajCodz
```

**`-Apply` requires `-ExpectedLogin` and an explicit scope** (`-Milestone ...` or `-CreateMissing`). This is stricter than AsciiWall, which allowed an unscoped Apply without a login check. To publish the full backlog, list every milestone key. `-ExpectedLogin` checks the authenticated account before the lock, the ledger, Project resolution, or any write. It also checks the author of each selected issue during verification. It does not assign issues to that account. A scoped verification records exactly its ticket keys and leaves other ticket and operation records untouched. Only the requested milestones' descriptions are managed. An ancestor's other milestone must already exist; it is read but never changed.

DryRun stays fully offline even when `-ExpectedLogin` is supplied.

### Append-only creation of remaining tickets (`-CreateMissing`)

Once an initial batch exists remotely, `-CreateMissing` creates only the tickets that are still unpublished. Issues that already exist, and their verified ledger records, stay read-only.

```powershell
# Offline preview from the source backlog and the local external ledger only (no gh calls, no lock, no ledger writes)
pwsh -NoProfile -File ./scripts/publish-issues.ps1 -BacklogPath ../.issues -DryRun -CreateMissing

# Read-only live audit of the pinned remaining batch
pwsh -NoProfile -File ./scripts/publish-issues.ps1 -BacklogPath ../.issues -VerifyOnly -CreateMissing -ExpectedLogin NeerajCodz

# Explicitly authorized append-only apply
pwsh -NoProfile -File ./scripts/publish-issues.ps1 -BacklogPath ../.issues -Apply -CreateMissing -ExpectedLogin NeerajCodz
```

- **Offline preview is not proof of live state.** `-DryRun -CreateMissing` emits `create_missing`, `preview_basis`, `creation_batch_pinned`, `protected_tickets`, `protected_keys`, `mutable_tickets`, `mutable_keys`, `github_accessed: false`, and `ledger_written: false`. `preview_basis` says whether it used the pinned `ledger.creation_batch` or only ledger identity records. Remote issues missing from the local ledger cannot be seen offline. Permissions and schema can only be checked by `-VerifyOnly` or `-Apply`.
- **Durable pin and resume (`ledger.creation_batch`).** The first `-Apply -CreateMissing` validates the full source and discovers remote issues read-only. Before creating any issue, it durably writes `creation_batch`, which records `schema_version`, `state`, `pinned_at`, `source_ref`, `source_sha`, `source_order`, `mutable_keys`, `protected_keys`, `protected_identities`, and `source_hashes`. Later Apply and VerifyOnly runs with `-CreateMissing` require that pin and reuse it. `-VerifyOnly -CreateMissing` fails closed if nothing is pinned yet. A ticket that was created but not finished stays mutable on resume and is never reclassified as protected. Before any write, the run stops on changed source order, source-hash drift, or a missing identity for a required parent or dependency.
- **Protected issues are never updated.** This covers closed issues, edited bodies, custom fields, labels, assignees, milestones, and progressed project `Status`. `Find-Identity -ReadOnly` resolves them in memory without touching the ledger. Their cached identities let new issue bodies link to real URLs and let new issues attach native parent and blocker edges to them. Attaching incoming edges changes GitHub's automatic `updatedAt` and sub-issue progress on a protected target; external before/after snapshots should ignore those fields.
- **Milestone ownership.** Only milestones that own at least one mutable ticket are managed. A milestone whose tickets are all protected is checked read-only.
- `-CreateMissing` cannot be combined with `-Milestone`.

`-GhCommand` and `-PythonCommand` take **executable paths**, not shell expressions, functions, argument strings, or `.ps1` scripts. Processes run through `ProcessStartInfo.ArgumentList` with no shell, UTF-8 streams, and JSON sent on stdin (`gh api --input -`). A fake `gh` must emit the same `--include` HTTP-header-plus-JSON output as the real one. A fake Python receives the validator path and the exact root arguments. These seams do not change any mode's safety rules, and a fake validator never counts as evidence that the real backlog is valid.

## Authorization and native API requirements

Before an Apply, authenticate `gh` with access to **both** repositories and the organization Project. The account needs these permissions:

- create and update issues, labels, and milestones
- set native issue types (Task, Bug, Feature)
- add native cross-repository sub-issues and hard blockers
- read Project repository links, fields, options, items, and item values; write fields, options, items, and item values

Fine-grained tokens need the matching repository Issues and organization Projects permissions. Keep credentials in the `gh` credential store or the environment, never in these files or the ledger.

Remote modes introspect the live schema. Platform needs native multi-select support: `ProjectV2MultiSelectField`, `multiSelectOptions`, and `ProjectV2FieldValue.multiSelectOptionIds`. Updating select options must preserve existing option IDs. A missing or incompatible native API fails the run. The publisher never falls back to text fields, draft items, label-only representations, or skipped phases. Conflicting native fields, parents, blockers, duplicate matches, and option-limit conflicts also stop the run for an operator to resolve. The publisher never destroys unrelated data to make verification pass.

Do not run two Apply processes at once, and do not edit the source backlog while publishing. `-VerifyOnly` does not take the Apply lock, so an audit that overlaps an Apply is not a stable snapshot.

## Apply phases

1. **Validate and prepare.**
   - Validate the external source and registries.
   - Reject any initial or final rendered body over GitHub's 65,536-character issue-body limit. The final check uses a conservative link-expansion estimate with synthetic issue and Project numbers that are never emitted.
   - Check the expected login and resolve the Project.
   - Take the exclusive external lock and load the ledger.
   - Inspect the live GraphQL schema and resolve the documentation revision in `Astraive/Glint`.
2. **Upsert registries.** Upsert every source label in both repositories, leaving other labels alone. Match milestones by exact repository and title across open and closed milestones, and reject ambiguous matches. Only the title and description of managed milestones are updated. Due dates and open/closed state are never replaced, and no dates are invented.
3. **Reconcile the existing Project.**
   - Link both repositories.
   - Require the native Repository, Milestone, and Assignees fields and never shadow them.
   - Create any missing custom field with the registry's data type.
   - Add missing select options without deleting options or changing existing IDs, colors, or descriptions. If keeping the existing options would exceed 50, stop for a manual, non-destructive migration.
   - The Project title must already match. The publisher does not rename the Project.
4. **Discover and create issues.**
   - Page through every open and closed issue in both repositories, excluding pull requests.
   - A match is the stable marker `<!-- glint-backlog:v1 repository=<key> ticket=<ID> -->`, or the exact `[ID] title` plus explicit ticket and repository metadata, or an identity already in the ledger. Several candidates or a conflicting marker is an error.
   - Create only selected tickets, in dependency order. References without a published identity stay plain text and never become invented URLs.
5. **Render final bodies and metadata.**
   - Once every selected identity is known, ticket links in scope become real issue URLs. Out-of-scope references are marked as local backlog.
   - `docs/...` links point to `Astraive/Glint/blob/<resolved-SHA>/docs/...`.
   - Requirements, unchecked criteria, and the unimplemented status are preserved.
   - The publisher refuses to overwrite a closed issue's planning metadata, or a remote body that matches no known managed hash (a prior desired or verified body, the journaled create body, the current rendered body, or the raw template).
   - Unrelated labels and assignees are kept. The native type is Task, Bug, or Feature; the exact classification stays in `type:*` and **Work type**.
6. **Reconcile native relationships.** Add a missing parent with `POST /repos/{owner}/{parentRepo}/issues/{n}/sub_issues`, where `sub_issue_id` is the child's numeric REST ID. Never set `replace_parent`, and never remove another parent. Add hard blockers on the dependent issue's `dependencies/blocked_by` endpoint. Soft `related` links stay as body links. Unexpected existing blockers are kept and reported as conflicts.
7. **Reconcile Project items.**
   - Add open issues by their real node IDs, never as drafts. Never add a closed issue or reset its Status.
   - Check `Status` before writing any other field. If the remote Status differs from both the last verified Status and the desired Status, stop instead of rolling back manual progress. An unset Status, or the default `Todo`, can still become `Backlog` on first publication.
   - Write Status under `project:value:<repo>/<id>/Status`. Write all changed non-Status fields in one journaled GraphQL alias mutation per item, under `project:values:<repo>/<id>`.
8. **Refetch everything independently.**
   - Verify titles, exact bodies and their SHA-256, managed and submitted labels, native type, milestone, parent and blocker IDs, item content, native fields, and custom values.
   - A mutation response never counts as proof.
   - The run exits zero with a `verified` JSON result only if everything matches.

Only supported local Markdown links are rewritten: repository documentation, external coverage-register references, and ticket links. Links inside fenced code blocks are left alone. Any other local link fails preflight instead of being published as a broken link.

The Project views, groupings, `Slice by` settings, and sorting are owned by `scripts/configure-github.ps1`. The publisher never deletes or rewrites views.

## Derived labels

`project.json` may declare `derived_labels` as `{ name, ticket_ids, source, meaning }` objects, for example `path:critical`. An entry without `ticket_ids` is treated as an empty list. A ticket gets a derived label only when its ID appears in that entry's `ticket_ids`. Each derived label name must be registered in `labels.json` and must not use a built-in family prefix. Reconciliation and verification manage only the declared derived names plus the six taxonomy families, so user-created `path:*` labels are left alone.

## External ledger, locking, and recovery

The ledger is `../.issues/publication-ledger.json` and its lock is `../.issues/publication-ledger.lock`. Both stay outside version control. Back the ledger up as operational state. Never copy its IDs into source `issue.json` files, and never delete it to force issues to be recreated.

Apply opens the lock with exclusive file sharing. The lock file may remain after the process exits; ownership comes from **the held file handle**, not from the file existing. Each ledger update writes a unique temporary file in the same directory, flushes it to disk, and renames it over the ledger. A failure therefore leaves either the complete old ledger or the complete new one. A hard crash can leave an unreferenced `.tmp` file, which is not a committed ledger.

The ledger stores exact numeric issue IDs, node IDs, numbers, and URLs. It also stores markers, source hashes, desired metadata and body hashes, label and milestone identities, parent and blocker identities, the Project number and node ID, field configurations, item IDs and desired values, independent verification results, and each operation's intent, outcome, or failure. Intent is persisted **before** the remote write. If persisting the result fails after the request, the operation stays uncertain; it is never assumed absent.

Writes are **never retried automatically**, whether they create, update, add relationships, or run GraphQL mutations. Only safe reads get bounded retries (`-ReadAttempts`, default 4). Read retries honor `Retry-After` and rate-limit reset headers for up to 60 seconds; a longer wait stops the run cleanly for a later resume. Every child process has a time limit (`-CommandTimeoutSeconds`, default 180), and a write that timed out may still have reached GitHub. Writes are spaced at least 1.05 seconds apart.

After an interruption, run the same Apply command again. Each phase refetches remote state first:

- A created issue that matches by marker or metadata is adopted.
- A journaled initial body lets an interrupted finalization resume.
- An operation whose desired result is already visible remotely is marked reconciled.

Apply stops cleanly without overwriting anything if it finds a manual body edit, a closed issue that would need Backlog metadata changes, or a manual Status change. There is no force flag. If an uncertain operation still cannot be proven applied, Apply stops and prints its exact operation key, HTTP method and endpoint, and JSON payload.

Only after you independently confirm that the exact pending request **did not apply** may you authorize one attempt for that key:

```powershell
# Run only after confirming that the pending operation did not apply remotely. Use the exact key the publisher printed.
pwsh -NoProfile -File ./scripts/publish-issues.ps1 -BacklogPath ../.issues -Apply -Milestone glint/M0 -ExpectedLogin NeerajCodz -RetryOperation 'create:glint/PRJ-001'
```

A grouped Project field batch (`project:values:<repo>/<id>`) is reconciled on resume only if a readback shows every desired value already matches. Otherwise, stop and confirm which fields did not apply before you authorize that key once. `-RetryOperation` works only with Apply. It never bypasses identity ambiguity, native conflicts, the body, closed-issue, or Status preservation guards, registry validation, or final verification. Never authorize a creation retry just because an eventually consistent listing did not show the issue yet.

Rerun `-VerifyOnly` after any remote change made outside this workflow. It reports every difference read-only, repairs nothing, and writes nothing to the ledger. Failures exit nonzero and print the exact unverified journaled requests to standard error.

## Regression tests

These run in CI (`.github/workflows/repository-checks.yml`), need no GitHub access, and work only with isolated temporary files:

- `pwsh -NoProfile -File scripts/test-publisher-ledger.ps1` creates and repeatedly replaces a real ledger file. It checks that issue identities, exact Unicode payloads, timestamp strings, and verification and receipt transitions all survive.
- `pwsh -NoProfile -File scripts/test-publisher-scope.ps1` loads the publisher's scope and identity functions from its syntax tree without running the script. It checks these behaviors:
  - Closed or in-progress issues become protected read-only keys, and their identities stay available for incoming references.
  - A pinned `creation_batch` resume keeps created-but-unfinished issues writable.
  - Read-only discovery leaves the ledger file unchanged.
  - Unresolved required identities fail closed.
  - The Glint-specific rules hold: repository aliases, `[ID] title` titles, optional-field defaults, rejection of missing required fields, and alias references.
- `pwsh -NoProfile -File scripts/test-project-view-audit.ps1` loads the view-audit functions from `scripts/configure-github.ps1` and checks board and table grouping orientation, ordered sort vectors, and that `Slice by` is reported as a manual-only check.

## Differences from the AsciiWall publisher

- Repositories `Astraive/Glint` and `Astraive/glint-library`. Backlog keys are `glint` and `glint-library`, and a `Repo-Name` mapping handles the capital-G GitHub slug. Issue markers are `glint-backlog:v1`.
- The Project is resolved at runtime by number or exact title and is never created or renamed. AsciiWall had Project 4 hard-coded and renamed it to the desired title.
- `-Apply` requires `-ExpectedLogin` plus `-Milestone` or `-CreateMissing`.
- Published titles always carry the `[ID] ` prefix. Required ticket fields are validated with clear messages. Optional fields get in-memory defaults, and dropped AsciiWall-only fields are ignored.
- Registries may use a nested `project` object or object-wrapped arrays. `derived_labels[].ticket_ids` may be omitted. Missing milestone descriptions are composed from structured fields.
- There is no co-author handling. The AsciiWall publisher had none either, and Glint allows `Co-authored-by:` trailers.
