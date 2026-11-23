<!--
Disposable research/risk spike PR template.
PR title format: type(scope): subject [TICKET-ID], for example "chore(windows-host): record appbar WebView2 spike [SPK-001]".
Co-authored-by trailers are allowed; contributors still commit under their own identity.
-->

**Ticket:** <!-- Stable ticket ID (e.g. SPK-001) and `Refs #N` / `Closes #N` once published. -->

## Hypothesis and question

<!-- The single bounded question from docs/plan this spike evaluates (for example Windows appbar + WebView2, Hyprland layer-shell + WebKitGTK, or package-security fixtures). -->

## Environment tuple

- OS build / kernel:
- Session type / desktop / compositor:
- WebView runtime (WebView2 / WebKitGTK / WKWebView) and version:
- GPU / driver / graphics backend:
- Toolchain / dependency versions:

## Observed outcome and fallback decision

- **Result:** <!-- Pass / Fail / Inconclusive -->
- **Observed behavior:**
- **Fallback or architectural consequence:** <!-- Link decision record or doc update if a contract changes. -->
- **Platforms not verified:** <!-- Name every platform this spike did not run on; leave them pending rather than assumed. -->

## Test evidence

<!-- Exact commands, fixtures, and observed measurements (or why manual native steps were needed). -->

## Checklist

- [ ] Spike remains explicitly scoped as an experiment; no untested platform support tier is promoted.
- [ ] Spike code does not weaken Glint invariants (offline core, sandboxed renderer, permission-scoped bridge, no executable package scripts).
- [ ] No proprietary fonts or icons, trademarked assets, secrets, or unredacted private paths, SSIDs, or window titles committed.
- [ ] Relevant risk register or decision docs updated if findings change contracts.
