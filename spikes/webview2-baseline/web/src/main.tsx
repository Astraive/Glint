// SPK-008 throwaway: one React panel that looks like a small theme (status
// bar + control center) and measures the engine it runs in. When hosted by
// spike-webview2-baseline it posts `report:<json>` to the host through
// window.chrome.webview.postMessage; in any other browser it only renders.
import { StrictMode, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { runA11yProbes } from './probes/a11y';
import { runCssProbes } from './probes/css';
import { runEnvProbes } from './probes/env';
import { round, runFontProbes } from './probes/fonts';
import { type FrameStats, measurePhase } from './probes/frames';
import { runImageProbes } from './probes/images';

const SCRIPT_START = performance.now();
const FRAMES = { idle: 120, transform: 240, effects: 240 };

declare global {
  interface Window {
    chrome?: { webview?: { postMessage(message: string): void } };
  }
}

function rect(el: Element | null) {
  if (!el) return null;
  const r = el.getBoundingClientRect();
  return { x: round(r.x), y: round(r.y), w: round(r.width), h: round(r.height) };
}

function measureLayout() {
  const cards = document.querySelector('.cards');
  const panel = document.querySelector('.panel');
  const card = document.querySelector('.card');
  return {
    remPx: parseFloat(getComputedStyle(document.documentElement).fontSize),
    panel: rect(panel),
    brand: rect(document.querySelector('.brand')),
    clock: rect(document.querySelector('.clock')),
    controlCenterButton: rect(document.querySelector('[aria-label="Open control center"]')),
    center: rect(document.querySelector('.center')),
    cardColumns: cards ? getComputedStyle(cards).gridTemplateColumns : null,
    firstCard: rect(card),
    tile: rect(document.querySelector('.tile')),
    translucency: {
      htmlBackground: getComputedStyle(document.documentElement).backgroundColor,
      panelBackground: panel ? getComputedStyle(panel).backgroundColor : null,
      panelBackdropFilter: panel ? getComputedStyle(panel).backdropFilter : null,
      cardBackdropFilter: card ? getComputedStyle(card).backdropFilter : null,
    },
  };
}

function ListBench({ tick }: { tick: number }) {
  return (
    <ul hidden>
      {Array.from({ length: 200 }, (_, i) => (
        <li key={i}>{(i * 31 + tick) % 97}</li>
      ))}
    </ul>
  );
}

function App() {
  const [clock, setClock] = useState(() => new Date());
  const [wifi, setWifi] = useState(true);
  const [workspace, setWorkspace] = useState(1);
  const [tick, setTick] = useState(0);
  const [status, setStatus] = useState('probing...');
  const [summary, setSummary] = useState('');
  const probes = useRef<HTMLDivElement>(null);
  const picture = useRef<HTMLImageElement>(null);
  const started = useRef(false);

  useEffect(() => {
    const id = setInterval(() => setClock(new Date()), 1000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    // StrictMode runs effects twice in development; this is a production
    // bundle, but guard anyway so the probes run exactly once.
    if (started.current) return;
    started.current = true;
    const firstCommitMs = round(performance.now() - SCRIPT_START);
    void (async () => {
      const report: Record<string, unknown> = { schema: 'glint-spk008-report/1' };
      const errors: string[] = [];
      const step = async (name: string, fn: () => unknown) => {
        setStatus(`probing ${name}...`);
        try {
          report[name] = await fn();
        } catch (err) {
          errors.push(`${name}: ${err}`);
          report[name] = null;
        }
      };
      await step('env', runEnvProbes);
      await step('fonts', runFontProbes);
      await step('layout', measureLayout);
      await step('css', () => runCssProbes(probes.current as HTMLDivElement));
      await step('images', () => runImageProbes(picture.current));
      await step('a11y', runA11yProbes);
      await step('frames', async () => {
        const out: FrameStats[] = [];
        for (const [phase, n] of Object.entries(FRAMES)) out.push(await measurePhase(phase, n));
        return out;
      });
      await step('react', () => {
        const start = performance.now();
        for (let i = 1; i <= 50; i++) flushSync(() => setTick(i));
        return { version: '19.3.1', firstCommitMs, rerender200ItemsX50Ms: round(performance.now() - start) };
      });
      // Paint entries are read last: they do not exist yet when env runs.
      report.paint = performance.getEntriesByType('paint').map((e) => ({ name: e.name, startMs: round(e.startTime) }));
      report.errors = errors;
      report.totalMs = round(performance.now() - SCRIPT_START);
      const json = JSON.stringify(report);
      setSummary(JSON.stringify(report, null, 1).slice(0, 4000));
      setStatus(errors.length ? `done with ${errors.length} error(s)` : 'done');
      window.chrome?.webview?.postMessage(`report:${json}`);
    })();
  }, []);

  const time = clock.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  return (
    <>
      <div className="wallpaper with-hole" aria-hidden="true" />
      <div className="panel" role="toolbar" aria-label="Glint probe panel">
        <button type="button" aria-label="Start">
          <span className="brand">Glint</span>
        </button>
        {[1, 2, 3, 4].map((n) => (
          <button key={n} type="button" aria-label={`Workspace ${n}`} aria-pressed={workspace === n} onClick={() => setWorkspace(n)}>
            {n}
          </button>
        ))}
        <span className="spacer" />
        <button type="button" role="switch" className="switch" aria-label="Wi-Fi" aria-checked={wifi} onClick={() => setWifi(!wifi)}>
          Wi-Fi
        </button>
        <time className="clock" dateTime={clock.toISOString()}>
          {time}
        </time>
        <button type="button" aria-label="Open control center" aria-expanded="true">
          ☰
        </button>
      </div>
      <section className="center" role="region" aria-label="Quick settings">
        <div className="cards">
          <div className="card">
            <h2>Sound</h2>
            <input className="meter" type="range" min={0} max={100} defaultValue={60} aria-label="Volume" role="slider" />
            <div className="masked" aria-hidden="true" />
          </div>
          <div className="card">
            <h2>Power</h2>
            <div role="progressbar" aria-label="Battery" aria-valuemin={0} aria-valuemax={100} aria-valuenow={72} className="masked" />
          </div>
          <div className="card">
            <h2>Tiles</h2>
            <div className="tiles" aria-hidden="true">
              {Array.from({ length: 24 }, (_, i) => (
                <div className="tile" key={i} />
              ))}
            </div>
          </div>
          <div className="card">
            <h2>Images</h2>
            <div className="images">
              <img src="img/probe.png" alt="PNG probe" />
              <img src="img/probe.svg" alt="SVG probe" />
              <img src="img/probe.webp" alt="WebP probe" />
              <img src="img/probe.avif" alt="AVIF probe" />
              <picture>
                <source srcSet="img/probe.avif" type="image/avif" />
                <source srcSet="img/probe.webp" type="image/webp" />
                <img ref={picture} src="img/probe.png" alt="Picture element probe" />
              </picture>
            </div>
          </div>
        </div>
        <div className="glass-mover" aria-hidden="true" />
        <div className="card">
          <h2>Status: {status}</h2>
          <div className="results">{summary}</div>
        </div>
      </section>
      <ListBench tick={tick} />
      <div id="probes" ref={probes} aria-hidden="true" />
    </>
  );
}

const root = document.getElementById('root');
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
