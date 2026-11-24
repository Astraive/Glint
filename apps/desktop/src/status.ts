// Turns a DaemonStatus into what the Home page shows. Pure, so the honest
// wording for every failure category is unit tested.

import type { DaemonStatus, ProblemCategory } from "./ipc.ts";

export type Tone = "ok" | "attention" | "down";

export interface StatusRow {
  label: string;
  value: string;
}

export interface StatusView {
  tone: Tone;
  headline: string;
  detail: string;
  rows: StatusRow[];
  warnings: { code: string; message: string }[];
}

const UNREACHABLE: Record<ProblemCategory, { headline: string; detail: string }> = {
  unavailable: {
    headline: "Daemon unavailable",
    detail:
      "No Glint core is answering for this user. Start glint-daemon; the manager does not start it and shows no state until it answers.",
  },
  "permission-denied": {
    headline: "Access to the daemon was refused",
    detail: "The core's endpoint belongs to another user or session. Nothing is shown in its place.",
  },
  timeout: {
    headline: "Daemon not responding",
    detail: "The core did not answer in time. It may be busy or hung; the manager will ask again.",
  },
  unsupported: {
    headline: "Daemon version not supported",
    detail: "The running core and this manager share no IPC API version. Update one of them.",
  },
  failure: {
    headline: "Could not read the daemon",
    detail: "Reading the core's state failed. The error code below names the cause.",
  },
};

function yesNo(value: boolean): string {
  return value ? "yes" : "no";
}

export function describeStatus(status: DaemonStatus): StatusView {
  if (status.state === "unreachable") {
    const { headline, detail } = UNREACHABLE[status.problem.category] ?? UNREACHABLE.failure;
    return {
      tone: "down",
      headline,
      detail,
      rows: [
        { label: "Error code", value: status.problem.code },
        { label: "Category", value: status.problem.category },
        { label: "Detail", value: status.problem.message },
      ],
      warnings: [],
    };
  }

  const { snapshot } = status;
  const ready = snapshot.lifecycle === "ready";
  const rows: StatusRow[] = [
    { label: "Lifecycle", value: snapshot.lifecycle },
    { label: "Daemon service", value: snapshot.daemon },
    { label: "Process ID", value: String(snapshot.pid) },
    { label: "Server", value: `${status.server} (IPC API ${status.api_version})` },
    { label: "Sequence", value: String(status.sequence) },
  ];
  if (snapshot.theme) {
    const theme = snapshot.theme;
    rows.push({ label: "Active theme", value: `${theme.theme_id} ${theme.version} (${theme.origin})` });
    if (theme.fallback_reason) rows.push({ label: "Fallback reason", value: theme.fallback_reason });
    rows.push({ label: "Surfaces", value: theme.surfaces.length > 0 ? theme.surfaces.join(", ") : "none" });
  } else {
    rows.push({ label: "Active theme", value: "not selected yet" });
  }
  if (snapshot.state) {
    rows.push({ label: "Installed packages", value: String(snapshot.state.installed_packages) });
    rows.push({ label: "Database recovered", value: yesNo(snapshot.state.database_recovered) });
  }
  if (snapshot.capabilities) {
    const caps = snapshot.capabilities;
    rows.push({
      label: "Capabilities",
      value: `${caps.usable} usable of ${caps.advertised} advertised (${caps.adapter})`,
    });
    if (caps.discovery_error) rows.push({ label: "Discovery error", value: caps.discovery_error });
  }
  if (snapshot.config) rows.push({ label: "User config", value: snapshot.config.user_config });
  if (snapshot.shutdown_reason) rows.push({ label: "Shutting down", value: snapshot.shutdown_reason });

  const degraded = snapshot.warnings.length > 0 || snapshot.shutdown_reason !== null;
  return {
    tone: ready && !degraded ? "ok" : "attention",
    headline: ready ? (degraded ? "Core running with warnings" : "Core running") : `Core ${snapshot.lifecycle}`,
    detail: ready
      ? "Connected to the Glint core over local IPC. Values below are read from it, not cached."
      : "Connected, but the core is not ready. Values below are what it has published so far.",
    rows,
    warnings: snapshot.warnings.map(({ code, message }) => ({ code, message })),
  };
}
