# Contributing to Glint

Thank you for contributing to Glint (`Astraive/Glint`). Before writing code or documentation, read [README.md](README.md), the planning entrypoint [docs/plan/README.md](docs/plan/README.md), [docs/development.md](docs/development.md), and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Repository Stage and Scope

Glint is currently in its **planning and repository-infrastructure baseline** (pre-implementation).

- Do **not** create empty Cargo workspaces, placeholder crates, empty TypeScript packages, or no-op workflow stubs solely to mirror the proposed layout in [docs/plan/06-engineering/monorepo-and-module-boundaries.md](docs/plan/06-engineering/monorepo-and-module-boundaries.md).
- Every change must truthfully state what was verified versus what remains unimplemented.
- Respect the accuracy labels in the plan (`CONFIRMED`, `RECOMMENDED`, `RESEARCH REQUIRED`, `NOT GUARANTEED`). Do not silently turn a recommended interface, version or schedule into a confirmed requirement.
- Official theme catalog, registry content and community theme submissions belong in the companion repository [`Astraive/glint-library`](https://github.com/Astraive/glint-library), not inside `Astraive/Glint`.

## Prerequisites and Local Verification

Current repository checks use Python 3.12+ (standard library only), PowerShell 7 (`pwsh`), and Git:

```sh
python scripts/check-repository.py
python scripts/check-conventions.py --title "docs(repo): clarify policy links [DOC-001]"
pwsh -NoProfile -File scripts/check-powershell.ps1
```

Wait for real Rust/Node tooling to exist in later milestones before running `cargo` or `npm` workflows; do not invent passing build, test, or platform claims before those prerequisites exist.

## Opt-In Local Git Hooks

To catch broken links, PowerShell syntax issues, or non-compliant commit messages locally before pushing, install the opt-in repository hooks ([.githooks/README.md](.githooks/README.md)):

```powershell
pwsh -NoProfile -File .githooks/install.ps1
```

This configures **only** repository-local `core.hooksPath = .githooks`. It does **not** alter your global Git configuration, `user.name`, `user.email`, or commit history, and never rewrites files silently.

## Commit Messages, PR Titles, and Identity Rules

Pull requests are squash-merged into `main`, so your PR title becomes the commit subject on `main`. Both local commit subjects (`commit-msg`) and PR titles (`scripts/check-conventions.py`) enforce:

```text
type(scope): subject [TICKET-ID]
```

- **Allowed `type` values:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`
- **Required `scope`:** lowercase kebab-case matching `^[a-z0-9][a-z0-9-]*$` (for example `docs`, `repo`, `ci`, `core`, `windows-host`)
- **Required `[TICKET-ID]`:** `[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+` (for example `[PRJ-001]`, `[REL-009]`, `[SPK-001]`, `[DOC-001]`)
- **`Co-authored-by:` trailers are allowed.** Credit pairing and shared authorship with standard trailers; each contributor still commits under their own identity.
- **No Dependabot bypass:** Dependabot cannot append trailing ticket IDs, and CI intentionally enforces strict PR titles for all PRs without bot exemptions. A maintainer triages automatic dependency PRs and renames the PR title to append the dependency-freshness owner (`[REL-009]`, or a dedicated upgrade/security ticket) before merge, re-triggering the `pull_request` `edited` check.
- **Contributor identity:** commit using your own Git `user.name` and `user.email`. Do not enable automatic DCO/sign-off injection or rewrite other contributors' history.

See [docs/issue-conventions.md](docs/issue-conventions.md) for ticket taxonomy, labels, and milestones.

## External Backlog Boundary

Detailed local backlog records and derived reports live **outside** Git in `../.issues` ([docs/backlog-validation.md](docs/backlog-validation.md), [docs/backlog-publication.md](docs/backlog-publication.md)):

- **Never** copy `.issues/`, `issue.json` records, `publication-ledger.json`, `publication-ledger.lock`, or `reports/` files into either Git repository.
- **Never** commit links pointing to `../.issues/...` or invented GitHub issue numbers prior to authorized publication.

## Engineering and Review Standards

Once application development starts:

1. **Test-first loop:** write a focused failing test, confirm the intended failure, implement the minimal complete behavior, and run targeted plus affected regression checks.
2. **Architectural boundaries:** keep the shared core free of platform-specific code; platform behavior belongs in host/adapter crates behind capability reporting; `.glint` packages are never trusted to run arbitrary native code or shell commands; renderers get no broad native APIs.
3. **Platform evidence:** native desktop behavior requires manual reproduction and an exact environment (OS build, session/compositor, GPU/driver, capabilities), never mock-only claims. Windows 11 is the only current verification host; Linux and macOS are compile-only in CI until real hardware exists.
4. **Privacy and rights:** never commit personal media, proprietary fonts or icons without a verified license, tokens or secrets, or unredacted host paths. Do not use proprietary brand assets in official themes without rights.
5. **Brand:** the Glint brand palette is neutral graphite, silver and white; it avoids green.
6. **Security reports:** never open a public PR or issue for an undisclosed vulnerability; follow [SECURITY.md](SECURITY.md).

## Related Policies

- [GOVERNANCE.md](GOVERNANCE.md) — maintainer ownership, review policy, and bootstrap branch protection
- [SECURITY.md](SECURITY.md) — private vulnerability disclosure and security invariants
- [SUPPORT.md](SUPPORT.md) — support channels and platform boundaries
- [VERSIONING.md](VERSIONING.md) — independent SemVer and draft format versioning
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — community participation standards
