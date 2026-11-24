// The manager's areas (brand and design language, "Glint app areas").
// Only Home is implemented by MGR-001; every other area names the ticket
// that builds it instead of showing invented content.

export interface Area {
  id: string;
  title: string;
  summary: string;
  /** The ticket(s) that build the page, or null when it exists. */
  owner: string | null;
}

export const AREAS: readonly Area[] = [
  { id: "home", title: "Home", summary: "Core status read over local IPC.", owner: null },
  { id: "discover", title: "Discover", summary: "Official and custom registries, filters and previews.", owner: "MGR-005" },
  { id: "installed", title: "Installed", summary: "Installed themes, origins, versions, pin, remove and export.", owner: "MGR-002" },
  { id: "customize", title: "Customize", summary: "Token editor, surface assignments and monitor layout.", owner: "MGR-007, MGR-008" },
  { id: "preview", title: "Preview", summary: "Simulated safe desktop with capability emulation.", owner: "MGR-003" },
  { id: "integrations", title: "Integrations", summary: "OS-specific features and diagnostics.", owner: "MGR-009" },
  { id: "developer", title: "Developer", summary: "Active local theme, reload and manifest diagnostics.", owner: "CRT-005" },
  { id: "settings", title: "Settings", summary: "Startup, permissions, cache, profiles and backups.", owner: "MGR-009" },
];
