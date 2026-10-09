/** Decimal byte units shared by both overview presentations. */
export function formatOverviewBytes(bytes: number | null): string {
  if (bytes === null || !Number.isFinite(bytes) || bytes < 0) return '—';
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(0)} KB`;
  return `${Math.round(bytes)} B`;
}

/** Rates in the shared overview history are decimal MB/s. */
export function formatOverviewSpeed(speed: number): string {
  if (!Number.isFinite(speed) || speed < 0) return '—';
  if (speed >= 1) return `${speed.toFixed(2)} MB/s`;
  const kb = speed * 1000;
  if (kb >= 1) return `${kb.toFixed(kb >= 100 ? 0 : 1)} KB/s`;
  return speed > 0 ? '<1 KB/s' : '0 KB/s';
}

/** The upstream counters must belong to the same kernel statistics scope. */
export function overviewTrafficTotals(up: number | null, down: number | null, unavailable: boolean) {
  const valid = (value: number | null) => !unavailable && value !== null && Number.isFinite(value) && value >= 0 ? value : null;
  const upload = valid(up);
  const download = valid(down);
  return { up: upload, down: download, total: upload !== null && download !== null ? upload + download : null };
}
