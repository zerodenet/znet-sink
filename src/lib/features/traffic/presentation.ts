import type { Observation, Point } from './history';

// Presentation only: authoritative cumulative counters and raw rates stay intact.
export const RATE_WINDOW_MS = 3000;

function windowRate(points: Point[], end: number, key: string): number | null {
  const latest = points[end]?.values[key];
  if (latest == null || !Number.isFinite(latest) || latest < 0) return null;
  let remaining = RATE_WINDOW_MS;
  let bytes = 0;
  let elapsed = 0;
  for (let index = end; index >= 0 && remaining > 0; index--) {
    const point = points[index];
    const rate = point.values[key];
    // Gaps/availability changes must not carry an older rate into recovery.
    if (rate == null || !Number.isFinite(rate) || rate < 0) break;
    const interval = point.intervalMs;
    // Older in-memory histories without timing retain their measured rate.
    if (interval == null || !Number.isFinite(interval) || interval <= 0) break;
    const used = Math.min(interval, remaining);
    bytes += rate * used;
    elapsed += used;
    remaining -= used;
  }
  return elapsed ? bytes / elapsed : latest;
}

/** Weight each sample by its actual monotonic interval, including short queries. */
export function displayRate(observation: Observation | undefined, key: string): number | null {
  const raw = observation?.rates[key];
  if (raw == null || !Number.isFinite(raw) || raw < 0) return null;
  return observation!.points.length
    ? windowRate(observation!.points, observation!.points.length - 1, key)
    : raw;
}

export function displaySeries(points: Point[], keys: string[]): Point[] {
  return points.map((point, index) => ({
    ...point,
    values: Object.fromEntries(keys.map(key => [key, windowRate(points, index, key)])),
  }));
}

/** Fixed scale bands + headroom keep small peak changes from resizing the chart. */
export function chartCeiling(previous: number, peak: number): number {
  const required = Math.max(1024, peak * 1.25);
  const band = 1024 * 2 ** Math.ceil(Math.log2(required / 1024));
  if (!Number.isFinite(previous) || previous < 1024) return band;
  if (required > previous) return band;
  // Reduce by one band only after the whole two-minute history is much quieter.
  if (band < previous && required <= previous / 4) return previous / 2;
  return previous;
}
