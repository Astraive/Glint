import { describe, expect, it } from "vitest";
import { BRIDGE_UNAVAILABLE, commandBridge, type DaemonStatus, type ProblemCategory } from "../src/ipc.ts";
import { describeStatus } from "../src/status.ts";

const connected: DaemonStatus = {
  state: "connected",
  api_version: 1,
  server: "glint-daemon/0.0.0",
  status: { lifecycle: "ready", daemon: "serving", pid: 4242, sequence: 9 },
  sequence: 9,
  snapshot: {
    lifecycle: "ready",
    daemon: "serving",
    pid: 4242,
    state: { schema_version: 4, installed_packages: 1, database_recovered: false },
    config: { user_config: "absent", diagnostics: 0, use_default_fallback: true },
    capabilities: { adapter: "headless", generation: 1, advertised: 0, usable: 0, discovery_error: null },
    theme: {
      theme_id: "official.default",
      version: "0.1.0",
      origin: "bundled-default",
      profile_id: null,
      revision: null,
      digest: null,
      fallback_reason: "no-active-theme",
      surfaces: [],
      skipped: [],
    },
    warnings: [],
    shutdown_reason: null,
  },
};

function unreachable(category: ProblemCategory, code = "GLINT_IPC_UNAVAILABLE"): DaemonStatus {
  return { state: "unreachable", problem: { code, category, message: "detail" } };
}

describe("describeStatus", () => {
  it("shows a ready core with the values it reported", () => {
    const view = describeStatus(connected);
    expect(view.tone).toBe("ok");
    expect(view.headline).toBe("Core running");
    const rows = Object.fromEntries(view.rows.map((row) => [row.label, row.value]));
    expect(rows["Process ID"]).toBe("4242");
    expect(rows["Active theme"]).toBe("official.default 0.1.0 (bundled-default)");
    expect(rows["Fallback reason"]).toBe("no-active-theme");
    expect(rows.Capabilities).toBe("0 usable of 0 advertised (headless)");
  });

  it("never calls a degraded or booting core healthy", () => {
    const warned = structuredClone(connected);
    if (warned.state !== "connected") throw new Error("fixture");
    warned.snapshot.warnings.push({ stage: "resolve-config", code: "GLINT_CORE_WARN_CONFIG_REJECTED", message: "x" });
    expect(describeStatus(warned).tone).toBe("attention");
    expect(describeStatus(warned).warnings).toHaveLength(1);
    expect(describeStatus(warned).headline).toBe("Core running with warnings");

    const booting = structuredClone(connected);
    if (booting.state !== "connected") throw new Error("fixture");
    booting.snapshot.lifecycle = "starting:open-state";
    booting.snapshot.theme = null;
    const view = describeStatus(booting);
    expect(view.tone).toBe("attention");
    expect(view.headline).toBe("Core starting:open-state");
    expect(view.rows).toContainEqual({ label: "Active theme", value: "not selected yet" });
  });

  it("keeps every failure category distinct and shows no state for it", () => {
    const headlines = new Set<string>();
    for (const category of ["unavailable", "permission-denied", "timeout", "unsupported", "failure"] as const) {
      const view = describeStatus(unreachable(category));
      expect(view.tone).toBe("down");
      expect(view.rows.map((row) => row.label)).toEqual(["Error code", "Category", "Detail"]);
      headlines.add(view.headline);
    }
    expect(headlines.size).toBe(5);
    expect(describeStatus(unreachable("unavailable")).headline).toBe("Daemon unavailable");
  });
});

describe("commandBridge", () => {
  it("calls only the two allow-listed commands", async () => {
    const calls: string[] = [];
    const bridge = commandBridge(async (command) => {
      calls.push(command);
      return command === "daemon_status" ? connected : { state: "connected", round_trip_ms: 3 };
    });
    expect(await bridge.status()).toEqual(connected);
    expect(await bridge.ping()).toEqual({ state: "connected", round_trip_ms: 3 });
    expect(calls).toEqual(["daemon_status", "daemon_ping"]);
  });

  it("turns a rejected command into an honest unreachable answer", async () => {
    const bridge = commandBridge(async () => {
      throw new Error("not inside Tauri");
    });
    const status = await bridge.status();
    expect(status.state).toBe("unreachable");
    if (status.state === "unreachable") {
      expect(status.problem.code).toBe(BRIDGE_UNAVAILABLE);
      expect(status.problem.category).toBe("unavailable");
    }
    expect((await bridge.ping()).state).toBe("unreachable");
  });
});
