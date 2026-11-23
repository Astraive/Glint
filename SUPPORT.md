# Support

## Current Status

Glint is currently a **planning and repository-infrastructure baseline**. **No executable desktop app, CLI binary, installer, SDK package, or published `.glint` theme is available yet.**

If you are looking for binaries today, none exist in this repository yet. See [README.md](README.md) and [CHANGELOG.md](CHANGELOG.md) for current contents and [docs/plan/06-engineering/roadmap-milestones-and-success.md](docs/plan/06-engineering/roadmap-milestones-and-success.md) for the planned phases.

## Where to Get Help

- **Orientation and specifications:** start with [docs/plan/README.md](docs/plan/README.md) and its decision register [docs/plan/00-history/decisions-and-open-questions.md](docs/plan/00-history/decisions-and-open-questions.md).
- **Questions and design feedback:** use [Astraive/Glint Discussions](https://github.com/Astraive/Glint/discussions) (when enabled) rather than opening bug reports for general questions.
- **Repository scripts or documentation issues:** file a report through the issue forms in `.github/ISSUE_TEMPLATE/`.
- **Security vulnerabilities:** **do not** ask for help publicly; follow [SECURITY.md](SECURITY.md) and use [private vulnerability reporting](https://github.com/Astraive/Glint/security/advisories/new).
- **Official theme library:** visit the companion repository [`Astraive/glint-library`](https://github.com/Astraive/glint-library).

## Platform Support Boundaries (Planned Policy)

Once Glint implementations ship, platform support is expected to follow these rules:

- **Support is a tested combination, not an OS logo.** A platform claim will name the OS build, session type, desktop environment or compositor, and verified capabilities.
- **No blanket cross-platform guarantee.** Windows 11 is the first native target and the only current test host; Linux and macOS adapters are compile-checked in CI only until real hardware verification exists, and each desktop environment advances independently.
- **Glint-managed surfaces only.** Glint does not promise to re-skin OS-owned interfaces such as the Windows Settings app, UAC or lock screens, GNOME Shell, or macOS Notification Center.
- **Capability limits:** missing compositor protocols or denied OS permissions disable optional features with an explicit explanation instead of reporting fake success.

## Preparing Bug and Diagnostic Reports (Once an App Exists)

When filing bugs against future executable builds:

1. Include expected vs. actual behavior, reproduction steps, and whether the issue occurs in preview or on the live desktop.
2. Provide your exact environment (OS build, session/compositor, GPU/driver, Glint commit or version).
3. Attach diagnostics only after reviewing and redacting private host paths or display details.
4. Never upload personal media, unlicensed assets, proprietary fonts, or memory dumps.
