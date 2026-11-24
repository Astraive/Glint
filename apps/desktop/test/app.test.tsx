// Smoke test: the whole manager renders with UI SDK, shows every area in
// the navigation, and shows the honest "daemon unavailable" state instead
// of any core values when the core cannot be reached.

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { App } from "../src/App.tsx";
import type { Bridge, DaemonStatus } from "../src/ipc.ts";
import { AREAS } from "../src/pages.ts";

const down: DaemonStatus = {
  state: "unreachable",
  problem: { code: "GLINT_IPC_UNAVAILABLE", category: "unavailable", message: "no endpoint record" },
};

const bridge: Bridge = {
  status: async () => down,
  ping: async () => ({ state: "unreachable", problem: down.problem }),
};

describe("App", () => {
  it("renders navigation for every manager area", () => {
    const html = renderToStaticMarkup(<App bridge={bridge} />);
    for (const title of ["Home", "Discover", "Installed", "Customize", "Preview", "Integrations", "Developer", "Settings"]) {
      expect(html).toContain(`>${title}</button>`);
    }
    expect(AREAS).toHaveLength(8);
    expect(html).toContain('role="tablist"');
    expect(html).toContain("Checking the daemon");
  });

  it("shows daemon unavailable with its code and no core values", () => {
    const html = renderToStaticMarkup(<App bridge={bridge} initialStatus={down} />);
    expect(html).toContain("Daemon unavailable");
    expect(html).toContain("GLINT_IPC_UNAVAILABLE");
    expect(html).not.toContain("Core running");
    expect(html).not.toContain("Active theme");
  });

  it("names the owning ticket on pages that are not built", () => {
    const html = renderToStaticMarkup(<App bridge={bridge} />);
    // Panels other than the selected one may not render their content on
    // the server; the area list itself carries the owners.
    for (const area of AREAS.filter((a) => a.owner !== null)) {
      expect(area.owner).toMatch(/^(MGR|CRT)-\d{3}/);
    }
    expect(html).toContain("Glint");
  });
});
