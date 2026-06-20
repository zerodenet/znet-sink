/**
 * @typedef {{ delay: number, at: number }} DelayEntry
 * @typedef {{ x: number, y: number, alive: boolean }} SparkPoint
 * @typedef {{ maxDelay: number, points: SparkPoint[] }} Sparkline
 */

/**
 * Build the normalized point set for a mini delay sparkline (120x32 viewBox).
 * @param {DelayEntry[]} history
 * @returns {Sparkline}
 */
export function buildSparkline(history) {
  const maxDelay = Math.max(1, ...history.map((entry) => entry.delay));
  const points = history.map((entry, index) => {
    const x = (index / Math.max(1, history.length - 1)) * 120;
    const y = 30 - (entry.delay > 0 ? Math.min(28, (entry.delay / maxDelay) * 28) : 0);
    return { x, y, alive: entry.delay > 0 };
  });
  return { maxDelay, points };
}

/**
 * Convert sparkline points to an SVG polyline string.
 * @param {Sparkline} sparkline
 */
export function sparklinePath(sparkline) {
  return sparkline.points.map((point) => `${point.x.toFixed(1)},${point.y.toFixed(1)}`).join(' ');
}

/**
 * Mean latency across successful probes.
 * @param {DelayEntry[]} history
 */
export function meanDelay(history) {
  const alive = history.filter((entry) => entry.delay > 0);
  if (alive.length === 0) return '-';
  return String(Math.round(alive.reduce((sum, entry) => sum + entry.delay, 0) / alive.length));
}
