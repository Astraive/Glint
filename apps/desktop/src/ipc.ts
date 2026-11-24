// The manager's view of the core, as the Tauri commands in src-tauri return
// it (src-tauri/src/bridge.rs). Field names are the IPC wire names
// (crates/glint-ipc), forwarded unchanged.

/** Why the core could not be read. Categories are never merged. */
export type ProblemCategory = "unsupported" | "permission-denied" | "unavailable" | "timeout" | "failure";

export interface Problem {
  /** Stable `GLINT_IPC_*`, `GLINT_STORAGE_*` or `GLINT_DESKTOP_*` code. */
  code: string;
  category: ProblemCategory;
  /** Human-readable detail; not stable. */
  message: string;
}

export interface StatusReply {
  lifecycle: string;
  daemon: string;
  pid: number;
  sequence: number;
}

export interface StateSummary {
  schema_version: number;
  installed_packages: number;
  database_recovered: boolean;
}

export interface ConfigSummary {
  user_config: string;
  diagnostics: number;
  use_default_fallback: boolean;
}

export interface CapabilitySummary {
  adapter: string;
  generation: number;
  advertised: number;
  usable: number;
  discovery_error: string | null;
}

export interface ThemeSummary {
  theme_id: string;
  version: string;
  origin: string;
  profile_id: string | null;
  revision: number | null;
  digest: string | null;
  fallback_reason: string | null;
  surfaces: string[];
  skipped: string[];
}

export interface WarningInfo {
  stage: string;
  code: string;
  message: string;
}

export interface CoreSnapshot {
  lifecycle: string;
  daemon: string;
  pid: number;
  state: StateSummary | null;
  config: ConfigSummary | null;
  capabilities: CapabilitySummary | null;
  theme: ThemeSummary | null;
  warnings: WarningInfo[];
  shutdown_reason: string | null;
}

export type DaemonStatus =
  | {
      state: "connected";
      api_version: number;
      server: string;
      status: StatusReply;
      sequence: number;
      snapshot: CoreSnapshot;
    }
  | { state: "unreachable"; problem: Problem };

export type PingResult = { state: "connected"; round_trip_ms: number } | { state: "unreachable"; problem: Problem };

/** What the UI needs from the native side; tests pass a fake. */
export interface Bridge {
  status(): Promise<DaemonStatus>;
  ping(): Promise<PingResult>;
}

/** Code used when the page is not running inside the manager window. */
export const BRIDGE_UNAVAILABLE = "GLINT_DESKTOP_BRIDGE_UNAVAILABLE";

export type Invoke = (command: string) => Promise<unknown>;

function bridgeProblem(error: unknown): Problem {
  return {
    code: BRIDGE_UNAVAILABLE,
    category: "unavailable",
    message: `the manager's native bridge did not answer: ${String(error)}`,
  };
}

/**
 * The bridge over the two allow-listed Tauri commands. A rejected call (the
 * page opened outside the manager window, or a command refused by the
 * capability) becomes an `unreachable` answer, never a fake success.
 */
export function commandBridge(invoke: Invoke): Bridge {
  return {
    async status() {
      try {
        return (await invoke("daemon_status")) as DaemonStatus;
      } catch (error) {
        return { state: "unreachable", problem: bridgeProblem(error) };
      }
    },
    async ping() {
      try {
        return (await invoke("daemon_ping")) as PingResult;
      } catch (error) {
        return { state: "unreachable", problem: bridgeProblem(error) };
      }
    },
  };
}
