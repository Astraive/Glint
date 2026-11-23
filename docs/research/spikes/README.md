# Feasibility spike reports

Each spike under `spikes/` writes its findings here, as the
[decision outputs](../../plan/08-research/feasibility-spikes.md#decision-outputs)
section asks: environment, exact library versions, code revision, evidence,
pass/fail, blockers and a recommendation. Spike code is throwaway and is never
imported by product code.

| Report | Ticket | Status |
|---|---|---|
| [Windows appbar + WebView2 host](windows-appbar.md) | SPK-001 | Code spike passes its self-checks; visual verification pending in SPK-002 |
| [Package security](package-security.md) | SPK-007 | 91 hostile fixtures rejected with no writes; limits and `zip` crate gaps recorded for PKG-001 |
| [Windows audio, network and battery providers](windows-system-providers.md) | SPK-010 | Pass on one Windows 11 laptop: every provider and sub-capability mapped; restart, stale-subscription and COM apartment results recorded; no-battery, device removal and real network changes not verifiable here |
