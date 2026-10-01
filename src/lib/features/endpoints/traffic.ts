import type { NetworkEndpoint } from './types';

export interface TrafficPoint { at: number; rx: number | null; tx: number | null }
export interface EndpointTraffic {
  scope: string;
  source: 'inner' | 'outer' | null;
  baseline: { at: number; rx: number | null; tx: number | null } | null;
  points: TrafficPoint[];
}
export const TRAFFIC_WINDOW_MS = 120_000;
const MAX_SAMPLE_GAP_MS = 15_000;

function counter(value: number | null | undefined): number | null {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : null;
}
export function sampleEndpointTraffic(previous: EndpointTraffic | undefined, endpoint: NetworkEndpoint): EndpointTraffic {
  const innerRx = counter(endpoint.counters.inner_rx_bytes);
  const innerTx = counter(endpoint.counters.inner_tx_bytes);
  const outerRx = counter(endpoint.counters.outer_rx_bytes);
  const outerTx = counter(endpoint.counters.outer_tx_bytes);
  const source = innerRx !== null || innerTx !== null ? 'inner' : outerRx !== null || outerTx !== null ? 'outer' : null;
  const scope = JSON.stringify([endpoint.core_instance_id, endpoint.endpoint_id, endpoint.config_revision, endpoint.generation, source]);
  const history: EndpointTraffic = previous?.scope === scope ? previous : { scope, source, baseline: null, points: [] };
  const at = endpoint.observed_at_unix_ms;
  if (!Number.isSafeInteger(at) || at <= 0 || (history.baseline && at <= history.baseline.at)) return history;
  const rx = source === 'inner' ? innerRx : outerRx;
  const tx = source === 'inner' ? innerTx : outerTx;
  const elapsed = history.baseline ? at - history.baseline.at : 0;
  const rate = (current: number | null, before: number | null | undefined) =>
    elapsed > 0 && elapsed <= MAX_SAMPLE_GAP_MS && current !== null && before != null && current >= before
      ? (current - before) * 1000 / elapsed : null;
  const point = { at, rx: rate(rx, history.baseline?.rx), tx: rate(tx, history.baseline?.tx) };
  return { scope, source, baseline: { at, rx, tx }, points: [...history.points, point].filter(point => at - point.at <= TRAFFIC_WINDOW_MS).slice(-121) };
}

export function trafficPaths(points: TrafficPoint[], key: 'rx' | 'tx', ceiling: number): string {
  const end = points.at(-1)?.at ?? 0;
  let open = false;
  return points.map(point => {
    const rate = point[key];
    if (rate === null) { open = false; return ''; }
    const x = Math.max(0, 300 - (end - point.at) / TRAFFIC_WINDOW_MS * 300);
    const y = 57 - Math.min(1, rate / ceiling) * 48;
    const command = `${open ? 'L' : 'M'}${x.toFixed(2)},${y.toFixed(2)}`;
    open = true;
    return command;
  }).join(' ');
}
export function formatEndpointRate(value: number | null | undefined): string {
  if (value == null) return '—';
  if (value < 1024) return `${Math.round(value)} B/s`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB/s`;
  return `${(value / 1024 / 1024).toFixed(1)} MB/s`;
}
