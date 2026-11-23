# OS test matrix and environment pinning

This page records which operating systems and checks are real today, which are not, and which tool versions are pinned. It follows [ADR 0002](../adr/0002-windows-11-only-hands-on-test-host.md) and the plan's [OS matrix](../plan/06-engineering/testing-and-quality-gates.md#os-matrix) and [test environment policy](../plan/03-platforms/compatibility-matrix.md#test-environment-policy).

**Rule:** a compatibility claim attaches to a tested matrix row, never to an OS family. Only claim an environment after a real test passes there. A row that has not been exercised is `not-run`, and `not-run` is never reported as a pass.

Status values: `real` (executed and recorded), `ci` (runs on a hosted runner only), `not-run` (no hardware or never executed), `n/a` (no workspace to run yet).

## Hosts that exist today

| Host | Kind | Pinned detail | Used for |
|---|---|---|---|
| Windows 11 Home Single Language | Physical, hands-on | OS version 10.0.26300; 1 monitor, 1920x1080, 125% scale (see [spike environment](../research/spikes/windows-appbar.md#environment)) | Hands-on and visual checks, local fmt/clippy/test |
| `windows-latest` | GitHub-hosted runner | Image floats with GitHub | Compile and test in CI |
| `ubuntu-latest` | GitHub-hosted runner | Image floats with GitHub | Compile and test in CI; repository quality job |
| `macos-latest` | GitHub-hosted runner | Image floats with GitHub | Compile and test in CI |

Runner images are not pinned to a fixed version; CI records `rustc --version` and `cargo --version` in each Rust job so the actual compiler is visible in the log. Pin a runner image only when a failure is traced to an image change.

CI is not native-platform verification. Linux and macOS Rust jobs are named "compile and test only", every Rust job writes that caveat to its step summary, and cargo runs with `--locked` so `Cargo.lock` is enforced. The Rust jobs run only when a `Cargo.toml` exists; otherwise they report "not applicable", which is not a pass (see [repository-checks.yml](../../.github/workflows/repository-checks.yml)).

## Matrix: OS and environment by check

Columns: **fmt / clippy / test** is `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`. **Hands-on** is a person exercising real desktop integration. **Visual** is a person (or recorded capture) confirming what is on screen.

| Platform | Environment | fmt / clippy / test | Hands-on | Visual | Pinned details still to record |
|---|---|---|---|---|---|
| Windows | Windows 11 10.0.26300, one display at 125% | real (local) and ci (`windows-latest`) | real for the SPK-001 and SPK-008 code spikes; product hosts not-run | not-run (pending SPK-002) | Second display/DPI combination not-run |
| Linux | Arch, Hyprland (Wayland) | ci (`ubuntu-latest`, compile only) | not-run (no hardware) | not-run | Compositor build, GPU, scale |
| Linux | KDE Plasma (Wayland) | ci (`ubuntu-latest`) | not-run (no hardware) | not-run | Plasma version, session type |
| Linux | GNOME (Wayland) | ci (`ubuntu-latest`) | not-run (no hardware) | not-run | GNOME Shell version, extension state |
| Linux | Sway / Niri | ci (`ubuntu-latest`) | not-run (no hardware) | not-run | Compositor version, protocols advertised |
| Linux | One X11 session | ci (`ubuntu-latest`) | not-run (no hardware) | not-run | Window manager, desktop environment |
| macOS | Apple Silicon | ci (`macos-latest`, compile only) | not-run (no hardware) | not-run | macOS version |
| macOS | Intel | not-run (no runner or hardware assigned) | not-run (no hardware) | not-run | macOS version |

The `ci` entries describe the workflow as configured. They only execute once a Cargo workspace is on the branch being checked; before that they are `n/a`. Linux and macOS rows therefore support no claim beyond "it compiled and its unit tests passed on a hosted runner", and no desktop-integration claim at all.

Every row without hardware is **pending** and stays `not-run` until someone runs it on real hardware and attaches evidence.

## Pinned toolchains

| Tool | Pin | Where it is pinned |
|---|---|---|
| Rust | 1.95.0, components `rustfmt` and `clippy`, minimal profile | [rust-toolchain.toml](../../rust-toolchain.toml); applied on CI by the Rust setup action |
| Node.js | `>=24.0.0` | `engines` in [package.json](../../package.json) |
| pnpm | 10.18.0 | `packageManager` in [package.json](../../package.json) |
| Biome (TypeScript lint) | 2.6.8, exact | `devDependencies` in [package.json](../../package.json) and [biome.json](../../biome.json); run with `pnpm lint` |
| Python | 3.12 on CI (repository checks); 3.12+ locally | `python-version` in [repository-checks.yml](../../.github/workflows/repository-checks.yml); [development guide](../development.md) |
| PowerShell | 7+ (`pwsh`) | [development guide](../development.md) |

`Node >=24` is a floor, not an exact version: record the exact `node --version` with any evidence. Manager UI dependencies are pinned per [ADR 0003](../adr/0003-ui-sdk-for-glint-manager-ui.md).

## Recording environment evidence for spikes

Every spike report under [`docs/research/spikes/`](../research/spikes/README.md) starts with an **Environment** table, as the plan's [decision outputs](../plan/08-research/feasibility-spikes.md#decision-outputs) require. Copy the one in the [Windows appbar spike](../research/spikes/windows-appbar.md#environment) and fill in:

| Item | How to capture it |
|---|---|
| OS and build | `winver`, or `[System.Environment]::OSVersion.Version` (Windows); `uname -a` and `/etc/os-release` (Linux); `sw_vers` (macOS) |
| Desktop / compositor | Name and exact version, session type (Wayland or X11) on Linux; Explorer version on Windows |
| Hardware | CPU architecture, GPU, displays (count, resolution, scale factor), power state if relevant |
| Toolchain | `rustc --version`, `cargo --version`, `node --version`, `pnpm --version`, `python --version` |
| Library versions | Exact versions from `Cargo.lock` or the lockfile, not ranges |
| Code revision | `git rev-parse --short HEAD`, and say whether the tree was clean |
| Date | ISO date of the run |

Then state, per matrix row touched, one of `pass`, `fail`, `blocked` or `not-run`, with the evidence (command output, log excerpt, or a screenshot/recording reference). Rules:

- Evidence must come from a real execution on the stated environment. Never copy results from a different host.
- Record absent environments as `not-run` with the reason (for example "no hardware"), never omit the row and never leave it blank.
- Distinguish what a process can observe about itself from what a person saw on screen; mark the latter `visual` only if someone actually looked.
- Keep no personal data or unredacted host paths in evidence.
- When a result changes a matrix row, update this page in the same pull request, and write an [ADR](../adr/README.md) if it changes support policy.

Milestone gate evidence is planned for `tests/evidence/gates/M0/`; no evidence there exists yet.
