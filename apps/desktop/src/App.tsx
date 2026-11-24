// The manager shell: navigation for every app area, the Home status page fed
// by the core over IPC, and the colour scheme switch. UI SDK supplies the
// behaviour (per-component imports only); brand.css supplies the look.

import type { ColorSchemeValue } from "@ui-sdk/core";
import { Button } from "@ui-sdk/react/button";
import { Preferences, usePreference } from "@ui-sdk/react/preferences";
import {
  SegmentedControl,
  SegmentedControlIndicator,
  SegmentedControlItem,
} from "@ui-sdk/react/segmented-control";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@ui-sdk/react/tabs";
import { useCallback, useEffect, useState } from "react";
import type { Bridge, DaemonStatus } from "./ipc.ts";
import { AREAS, type Area } from "./pages.ts";
import { colorScheme } from "./preferences.ts";
import { describeStatus } from "./status.ts";

/** How often Home asks the core again while it is shown. */
export const REFRESH_MS = 5000;

function StatusPanel({ status }: { status: DaemonStatus | null }) {
  if (status === null) {
    return (
      <section className="status" data-tone="pending" aria-live="polite">
        <h2>Checking the daemon...</h2>
        <p>Asking the Glint core for its state over local IPC.</p>
      </section>
    );
  }
  const view = describeStatus(status);
  return (
    <section className="status" data-tone={view.tone} aria-live="polite">
      <h2>{view.headline}</h2>
      <p>{view.detail}</p>
      <dl className="status-rows">
        {view.rows.map((row) => (
          <div key={row.label}>
            <dt>{row.label}</dt>
            <dd>{row.value}</dd>
          </div>
        ))}
      </dl>
      {view.warnings.length > 0 && (
        <>
          <h3>Warnings</h3>
          <ul className="warnings">
            {view.warnings.map((warning) => (
              <li key={`${warning.code}:${warning.message}`}>
                <code>{warning.code}</code> {warning.message}
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

function Home({ bridge, initialStatus }: { bridge: Bridge; initialStatus: DaemonStatus | null }) {
  const [status, setStatus] = useState<DaemonStatus | null>(initialStatus);

  const refresh = useCallback(async () => {
    setStatus(await bridge.status());
  }, [bridge]);

  useEffect(() => {
    let live = true;
    const tick = async () => {
      const next = await bridge.status();
      if (live) setStatus(next);
    };
    void tick();
    const timer = setInterval(() => void tick(), REFRESH_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [bridge]);

  return (
    <>
      <header className="page-header">
        <h1>Home</h1>
        <Button className="button" onClick={refresh} pendingAnnouncement="Checking the daemon">
          Check again
        </Button>
      </header>
      <StatusPanel status={status} />
    </>
  );
}

function Planned({ area }: { area: Area }) {
  return (
    <>
      <header className="page-header">
        <h1>{area.title}</h1>
      </header>
      <section className="status" data-tone="pending">
        <h2>Not built yet</h2>
        <p>{area.summary}</p>
        <p>
          This page arrives with <strong>{area.owner}</strong>. The manager shows nothing here until then.
        </p>
      </section>
    </>
  );
}

function SchemeSwitch() {
  const { value } = usePreference(colorScheme);
  return (
    <SegmentedControl<ColorSchemeValue>
      className="scheme"
      aria-label="Colour scheme"
      value={value}
      onValueChange={(next) => {
        if (next) colorScheme.set(next);
      }}
    >
      <SegmentedControlIndicator className="scheme-indicator" />
      <SegmentedControlItem value="system">System</SegmentedControlItem>
      <SegmentedControlItem value="light">Light</SegmentedControlItem>
      <SegmentedControlItem value="dark">Dark</SegmentedControlItem>
    </SegmentedControl>
  );
}

export interface AppProps {
  bridge: Bridge;
  /** Status to show before the first answer (tests); null shows "checking". */
  initialStatus?: DaemonStatus | null;
}

export function App({ bridge, initialStatus = null }: AppProps) {
  return (
    <Preferences preferences={[colorScheme]}>
      <Tabs className="shell" defaultValue="home" orientation="vertical">
        <nav className="sidebar" aria-label="Glint manager">
          <div className="wordmark">Glint</div>
          <TabsList className="nav" aria-label="Sections">
            {AREAS.map((area) => (
              <TabsTab className="nav-item" key={area.id} value={area.id}>
                {area.title}
              </TabsTab>
            ))}
          </TabsList>
          <SchemeSwitch />
        </nav>
        {AREAS.map((area) => (
          <TabsPanel className="page" key={area.id} value={area.id}>
            {area.owner === null ? <Home bridge={bridge} initialStatus={initialStatus} /> : <Planned area={area} />}
          </TabsPanel>
        ))}
      </Tabs>
    </Preferences>
  );
}
