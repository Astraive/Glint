// Engine and environment facts the page can observe about itself.
import { round } from './fonts';

interface UaData {
  brands: { brand: string; version: string }[];
  platform: string;
  getHighEntropyValues(hints: string[]): Promise<Record<string, unknown>>;
}

export interface EnvReport {
  userAgent: string;
  brands: string[];
  fullVersionList: string[];
  platform: string;
  platformVersion: string;
  devicePixelRatio: number;
  viewport: string;
  screen: string;
  hardwareConcurrency: number;
  webglRenderer: string;
  webgpu: boolean;
  jsHeap: { usedMiB: number; totalMiB: number; limitMiB: number } | null;
  measureUserAgentSpecificMemory: boolean;
  timing: { domContentLoadedMs: number };
}

export async function runEnvProbes(): Promise<EnvReport> {
  const ua = (navigator as Navigator & { userAgentData?: UaData }).userAgentData;
  let fullVersionList: string[] = [];
  let platformVersion = '';
  if (ua) {
    try {
      const high = await ua.getHighEntropyValues(['fullVersionList', 'platformVersion']);
      fullVersionList = ((high.fullVersionList as { brand: string; version: string }[]) ?? []).map((b) => `${b.brand} ${b.version}`);
      platformVersion = String(high.platformVersion ?? '');
    } catch {
      /* hints denied: leave empty */
    }
  }
  let webglRenderer = 'unavailable';
  try {
    const gl = document.createElement('canvas').getContext('webgl');
    const ext = gl?.getExtension('WEBGL_debug_renderer_info');
    if (gl) webglRenderer = String(ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER));
  } catch (err) {
    webglRenderer = `error: ${err}`;
  }
  const mem = (performance as Performance & { memory?: { usedJSHeapSize: number; totalJSHeapSize: number; jsHeapSizeLimit: number } }).memory;
  const mib = (n: number) => round(n / 1048576, 1);
  const nav = performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming | undefined;
  return {
    userAgent: navigator.userAgent,
    brands: ua ? ua.brands.map((b) => `${b.brand} ${b.version}`) : [],
    fullVersionList,
    platform: ua?.platform ?? navigator.platform,
    platformVersion,
    devicePixelRatio: window.devicePixelRatio,
    viewport: `${innerWidth}x${innerHeight}`,
    screen: `${screen.width}x${screen.height}`,
    hardwareConcurrency: navigator.hardwareConcurrency,
    webglRenderer,
    webgpu: 'gpu' in navigator,
    jsHeap: mem ? { usedMiB: mib(mem.usedJSHeapSize), totalMiB: mib(mem.totalJSHeapSize), limitMiB: mib(mem.jsHeapSizeLimit) } : null,
    measureUserAgentSpecificMemory: 'measureUserAgentSpecificMemory' in performance,
    timing: {
      domContentLoadedMs: nav ? round(nav.domContentLoadedEventEnd) : -1,
    },
  };
}
