// requestAnimationFrame timing over N frames per phase. The phase class on
// <body> switches which CSS animations run (see baseline.css):
//   idle       nothing animates
//   transform  24 tiles animate transform + opacity (compositor-friendly)
//   effects    a backdrop-filter card slides and cards animate box-shadow
//              + filter (paint-heavy)
import { round } from './fonts';

export interface FrameStats {
  phase: string;
  frames: number;
  meanMs: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  maxMs: number;
  /** Intervals longer than 1.5x the median (a missed vsync). */
  dropped: number;
  estimatedHz: number;
  longTasks: number;
  visibility: DocumentVisibilityState;
}

const nextFrame = () => new Promise<number>((resolve) => requestAnimationFrame(resolve));

export async function measurePhase(phase: string, frames: number): Promise<FrameStats> {
  document.body.classList.remove('phase-idle', 'phase-transform', 'phase-effects');
  document.body.classList.add(`phase-${phase}`);
  let longTasks = 0;
  let observer: PerformanceObserver | null = null;
  try {
    observer = new PerformanceObserver((list) => {
      longTasks += list.getEntries().length;
    });
    observer.observe({ type: 'longtask', buffered: false });
  } catch {
    observer = null;
  }
  // Let the animation start and settle for a few frames before sampling.
  for (let i = 0; i < 10; i++) await nextFrame();
  const intervals: number[] = [];
  let last = await nextFrame();
  for (let i = 0; i < frames; i++) {
    const now = await nextFrame();
    intervals.push(now - last);
    last = now;
  }
  observer?.disconnect();
  document.body.classList.remove(`phase-${phase}`);
  const sorted = [...intervals].sort((a, b) => a - b);
  const pct = (p: number) => sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))];
  const median = pct(50);
  const mean = intervals.reduce((a, b) => a + b, 0) / intervals.length;
  return {
    phase,
    frames: intervals.length,
    meanMs: round(mean),
    p50Ms: round(median),
    p95Ms: round(pct(95)),
    p99Ms: round(pct(99)),
    maxMs: round(sorted[sorted.length - 1]),
    dropped: intervals.filter((v) => v > median * 1.5).length,
    estimatedHz: round(1000 / median, 1),
    longTasks,
    visibility: document.visibilityState,
  };
}
