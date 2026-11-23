# Security Policy

## Current Status and Supported Versions

Glint is currently in a **planning and repository-infrastructure baseline** (`main`). **No executable application, daemon, shell host, renderer, package importer, installer, or release artifact has been shipped yet.**

| Target / Release | Supported | Notes |
| --- | --- | --- |
| Released binaries (`0.x` / `1.x`) | None yet | No compiled release exists; security controls are design proposals in [docs/plan/](docs/plan/README.md) and tracked as backlog prerequisites. |
| `main` (plan, repo scripts, workflows) | Active baseline | Planning documents and Python/PowerShell repository tooling and CI configuration only. |

Accordingly, renderer confinement, `.glint` archive extraction limits, permission-bridge enforcement, fuzzing, and binary signing are **planned release gates**, not yet tested production defenses.

## Reporting a Vulnerability (Private Route Only)

**Do not open a public GitHub issue, pull request, or discussion** for an undisclosed vulnerability, a hostile `.glint` archive or path-traversal proof of concept, a sandbox or permission-bridge escape, a privilege-escalation flaw in CI/workflows, or a supply-chain compromise.

Report vulnerabilities privately through:

1. **GitHub Private Vulnerability Reporting (preferred):** submit a draft security advisory at [Astraive/Glint Security Advisories](https://github.com/Astraive/Glint/security/advisories/new).
2. **Maintainer private fallback:** if GitHub Private Vulnerability Reporting is unavailable or disabled, contact repository maintainer `@NeerajCodz` (`https://github.com/NeerajCodz`) privately before posting any details publicly.

### What to include

- Affected repository (`Astraive/Glint` or `Astraive/glint-library`), commit SHA (or future release version), and component
- Exact reproduction steps using a **minimized, rights-cleared, synthetic fixture**
- Expected vs. observed security boundary behavior and potential impact
- Environment (OS build, session/compositor, or Python/PowerShell tool versions) when applicable

**Privacy guardrail:** never attach personal files, proprietary fonts or icons, tokens, or unredacted local filesystem paths to a report.

## Response, Triage, and Remediation Policy

- Repository maintainer `@NeerajCodz` reviews incoming reports, determines the affected scope, and coordinates remediation.
- Destructive-data, archive-escape, permission-bypass, desktop-lockout, and supply-chain failures are treated as **non-waivable** release blockers and cannot be shipped under a "beta" label.
- Security updates and advisories publish affected versions and artifact hashes without collecting user data or silent telemetry.

## Planned Security Boundaries

Future Glint releases are designed around these invariants (see [docs/plan/README.md](docs/plan/README.md) and its security-related architecture documents). The [threat model](docs/security/threat-model.md) maps each hazard to its owner ticket and records which controls are implemented, prototyped or still planned:

- **Data-only packages by default:** `.glint` packages never execute arbitrary shell scripts, native binaries, or privileged system hooks.
- **Restricted renderers:** programmable theme UI runs in isolated, precompiled bundles with no broad native APIs; every bridge call is authorized against package grants and OS capabilities.
- **Strict extraction:** import enforces archive size, entry-count and path limits before anything touches live state.
- **Safe mode:** a built-in default theme and recovery command remain available if a theme misbehaves.
- **CI privilege separation:** untrusted pull requests never receive release signing secrets and never run privileged `pull_request_target` checkouts.
- **Explicit non-goals:** Glint is not a sandbox against an already compromised operating system, malicious driver, local administrator account, or modified application binaries, and it does not secure OS-owned surfaces such as login, UAC or lock screens.
