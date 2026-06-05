import { getCorePolicies } from '$lib/services/core';
import type { ProxyNode } from '$lib/types/protocol';

const MAX_HISTORY = 300; // 5 minutes at 1-second sampling
const MIN_RATE_INTERVAL_MS = 500; // minimum interval for stable speed calculation

// ── Runtime / policy node extraction (varied data shapes from core IPC) ──

function extractNodes(data: unknown): ProxyNode[] {
  if (!data || typeof data !== 'object') return [];

  const obj = data as Record<string, unknown>;
  const candidates: unknown[] = [];

  for (const key of ['proxies', 'outbounds', 'nodes', 'proxyNodes', 'servers']) {
    const arr = obj[key];
    if (Array.isArray(arr)) candidates.push(...arr);
  }

  for (const key of ['proxy', 'inbounds', 'config']) {
    const sub = obj[key];
    if (sub && typeof sub === 'object' && !Array.isArray(sub)) {
      for (const subKey of ['proxies', 'outbounds', 'nodes']) {
        const arr = (sub as Record<string, unknown>)[subKey];
        if (Array.isArray(arr)) candidates.push(...arr);
      }
    }
  }

  return candidates.map((item, idx) => parseNode(item, idx)).filter(Boolean) as ProxyNode[];
}

function parseNode(item: unknown, index: number): ProxyNode | null {
  if (!item || typeof item !== 'object') return null;
  const obj = item as Record<string, unknown>;

  const name = obj['name'] || obj['tag'] || obj['id'];
  if (!name || typeof name !== 'string') return null;

  const protocol = (typeof obj['protocol'] === 'string' ? obj['protocol'] : '')
    || (typeof obj['type'] === 'string' ? obj['type'] : '')
    || 'ZNet';

  const host = obj['server'] || obj['address'] || obj['host'] || obj['domain'] || '';
  const port = obj['port'];
  const domain = typeof host === 'string'
    ? host + (typeof port === 'number' ? `:${port}` : '')
    : '';

  const delay = typeof obj['delay'] === 'number'
    ? obj['delay']
    : typeof obj['latency'] === 'number'
      ? obj['latency']
      : 0;

  return {
    id: typeof obj['id'] === 'string' ? obj['id'] : `${name}-${index}`,
    name: name,
    protocol: protocol,
    delay: delay,
    domain: domain || 'zerodenet.org',
  };
}

function extractPolicyNodes(data: unknown): ProxyNode[] {
  const groups = valuesFromContainer(data, ['policies', 'outbound_groups', 'outboundGroups', 'policy_groups', 'policyGroups', 'groups', 'items']);
  const nodes = groups.flatMap((group) => {
    if (!group || typeof group !== 'object') return [];
    const obj = group as Record<string, unknown>;
    const selected = stringFrom(obj, ['selected', 'current', 'now', 'target']);
    return valuesFromContainer(obj, ['members', 'targets', 'children', 'outbounds', 'proxies', 'items'])
      .map((member, idx) => parsePolicyMember(member, selected, idx))
      .filter(Boolean) as ProxyNode[];
  });

  return dedupeNodes(nodes);
}

function parsePolicyMember(item: unknown, selected: string | null, index: number): ProxyNode | null {
  if (typeof item === 'string') {
    return { id: item, name: item, protocol: 'Zero', delay: 0, domain: selected === item ? 'selected' : 'policy' };
  }
  if (!item || typeof item !== 'object') return null;

  const obj = item as Record<string, unknown>;
  const name = stringFrom(obj, ['tag', 'targetTag', 'target_tag', 'name', 'id', 'target']);
  if (!name) return null;

  const protocol = stringFrom(obj, ['kind', 'type', 'protocol']) ?? 'Zero';
  const delay = numberFrom(obj, ['delayMs', 'delay_ms', 'latencyMs', 'latency_ms', 'latency']) ?? 0;
  const alive = boolFrom(obj, ['alive', 'healthy', 'available']);
  const domain = stringFrom(obj, ['server', 'address', 'host', 'domain'])
    ?? (selected === name ? 'selected' : alive === false ? 'unavailable' : 'policy');

  return {
    id: stringFrom(obj, ['id']) ?? `${name}-${index}`,
    name,
    protocol,
    delay,
    domain,
  };
}

function valuesFromContainer(data: unknown, keys: string[]): unknown[] {
  if (Array.isArray(data)) return data;
  if (!data || typeof data !== 'object') return [];

  const obj = data as Record<string, unknown>;
  for (const key of keys) {
    const value = obj[key];
    if (Array.isArray(value)) return value;
    if (value && typeof value === 'object') return Object.values(value);
  }

  return [];
}

function stringFrom(obj: Record<string, unknown>, keys: string[]): string | null {
  for (const key of keys) {
    const value = obj[key];
    if (typeof value === 'string' && value.trim()) return value.trim();
    if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  }
  return null;
}

function numberFrom(obj: Record<string, unknown>, keys: string[]): number | null {
  for (const key of keys) {
    const value = obj[key];
    if (typeof value === 'number' && isFinite(value)) return value;
    if (typeof value === 'string') {
      const parsed = Number(value);
      if (isFinite(parsed)) return parsed;
    }
  }
  return null;
}

function boolFrom(obj: Record<string, unknown>, keys: string[]): boolean | null {
  for (const key of keys) {
    const value = obj[key];
    if (typeof value === 'boolean') return value;
    if (typeof value === 'string') {
      const normalized = value.toLowerCase();
      if (['true', 'yes', '1'].includes(normalized)) return true;
      if (['false', 'no', '0'].includes(normalized)) return false;
    }
  }
  return null;
}

function dedupeNodes(nodes: ProxyNode[]): ProxyNode[] {
  const seen = new Set<string>();
  return nodes.filter((node) => {
    const key = node.name.toLowerCase();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

// ── Compact fallback: extract a number from raw data using known key variations ──

function pickNumber(data: Record<string, unknown>, keys: string[]): number {
  for (const key of keys) {
    const val = data[key];
    if (typeof val === 'number' && isFinite(val)) return val;
  }
  return 0;
}

function pickNumberRecursive(data: Record<string, unknown>, keys: string[]): number {
  for (const key of keys) {
    const val = data[key];
    if (typeof val === 'number' && isFinite(val)) return val;
  }

  // Try one level of nesting (raw IPC often wraps in `{ stats: {...} }` or `{ data: {...} }`)
  for (const wrapper of ['stats', 'data', 'traffic', 'result']) {
    const nested = data[wrapper];
    if (nested && typeof nested === 'object' && !Array.isArray(nested)) {
      for (const key of keys) {
        const val = (nested as Record<string, unknown>)[key];
        if (typeof val === 'number' && isFinite(val)) return val;
      }
    }
  }

  return 0;
}

// ── Store ──

class OverviewDataStore {
  speedHistory = $state<{ up: number; down: number }[]>(
    Array.from({ length: MAX_HISTORY }, () => ({ up: 0, down: 0 })),
  );
  proxyNodes = $state<ProxyNode[]>([]);
  activeConnections = $state(0);
  isLive = $state(false);
  totalUpBytes = $state(0);
  totalDownBytes = $state(0);

  // Speed calculation baseline (delta-based, matching Rust build_traffic_snapshot)
  private _lastBytesUp = 0;
  private _lastBytesDown = 0;
  private _lastSampleAt = 0;

  get totalUpMB() { return this.totalUpBytes / 1_000_000; }
  get totalDownMB() { return this.totalDownBytes / 1_000_000; }

  /**
   * Apply a stats event.
   *
   * Two data paths:
   * 1. Typed (traffic.sampled event → GuiTrafficStats): { bytesUp, bytesDown, activeSessions, ... }
   * 2. Raw (getCoreStats IPC or legacy events): unknown shape, fallback key matching
   */
  applyStatsEvent(data: Record<string, unknown>) {
    this.isLive = true;

    // Primary keys (GuiTrafficStats from event stream)
    const bytesUp = pickNumber(data, ['bytesUp', 'bytes_up', 'upload', 'tx']);
    const bytesDown = pickNumber(data, ['bytesDown', 'bytes_down', 'download', 'rx']);
    const sessions = pickNumber(data, ['activeSessions', 'active_sessions', 'activeConnections', 'connectionCount']);

    // Fallback keys (raw IPC often nests inside { stats: {...} })
    const totalUp = bytesUp || pickNumberRecursive(data, [
      'totalUploadBytes', 'uploadTotal', 'totalUp', 'totalUpload',
      'uploadBytes', 'txBytes', 'totalTx', 'bytes_up', 'bytesUp',
    ]);
    const totalDown = bytesDown || pickNumberRecursive(data, [
      'totalDownloadBytes', 'downloadTotal', 'totalDown', 'totalDownload',
      'downloadBytes', 'rxBytes', 'totalRx', 'bytes_down', 'bytesDown',
    ]);
    const connections = sessions || pickNumberRecursive(data, [
      'activeSessions', 'active_sessions', 'activeConnections', 'connections',
      'sessionCount', 'connectionCount', 'sessions', 'activeFlows',
    ]);

    // Delta-based speed (MB/s)
    const now = Date.now();
    let upRate = 0;
    let downRate = 0;

    if (this._lastSampleAt > 0) {
      const intervalMs = now - this._lastSampleAt;
      if (intervalMs >= MIN_RATE_INTERVAL_MS) {
        upRate = ((bytesUp - this._lastBytesUp) / intervalMs * 1000) / 1_000_000;
        downRate = ((bytesDown - this._lastBytesDown) / intervalMs * 1000) / 1_000_000;
      }
    }

    this._lastBytesUp = bytesUp;
    this._lastBytesDown = bytesDown;
    this._lastSampleAt = now;

    this.speedHistory.push({ up: Math.max(0, upRate), down: Math.max(0, downRate) });
    if (this.speedHistory.length > MAX_HISTORY) {
      this.speedHistory.shift();
    }

    this.activeConnections = connections;
    this.totalUpBytes = totalUp;
    this.totalDownBytes = totalDown;
  }

  applyRuntimeEvent(data: Record<string, unknown>) {
    const nodes = extractNodes(data);
    if (nodes.length > 0) {
      this.proxyNodes = nodes;
    }
  }

  applyPolicyEvent(data: unknown) {
    const nodes = extractPolicyNodes(data);
    if (nodes.length > 0) {
      this.proxyNodes = nodes;
    }
  }

  async refreshPolicyNodes() {
    const result = await getCorePolicies();
    if (!result.available || !result.response) return;
    this.applyPolicyEvent(result.response);
  }
}

export const overviewData = new OverviewDataStore();
