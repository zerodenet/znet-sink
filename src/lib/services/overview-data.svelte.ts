import { getCorePolicies, getCoreStats, getCoreRuntime } from '$lib/services/core';
import type { ProxyNode } from '$lib/types/protocol';

const MAX_HISTORY = 300; // 5 minutes at 1-second sampling
const STATS_INTERVAL_MS = 1000;
const RUNTIME_INTERVAL_MS = 30_000;

function extractSpeed(data: unknown, direction: 'up' | 'down'): number {
  if (!data || typeof data !== 'object') return 0;

  const upKeys = [
    'uploadSpeed', 'upload_speed', 'uploadSpeedBps', 'upstreamBps',
    'outboundSpeed', 'outbound_speed', 'tx', 'txSpeed',
    'up', 'upload', 'outbound',
  ];
  const downKeys = [
    'downloadSpeed', 'download_speed', 'downloadSpeedBps', 'downstreamBps',
    'inboundSpeed', 'inbound_speed', 'rx', 'rxSpeed',
    'down', 'download', 'inbound',
  ];
  const keys = direction === 'up' ? upKeys : downKeys;

  for (const key of keys) {
    const val = (data as Record<string, unknown>)[key];
    if (typeof val === 'number' && isFinite(val)) {
      return key.endsWith('Bps') || key === direction || key === 'up' || key === 'down'
        ? val / 1_000_000 // bytes/sec → MB/s
        : val;
    }
  }

  // Try nested stats object
  const stats = (data as Record<string, unknown>)['stats'] || (data as Record<string, unknown>)['data'];
  if (stats && typeof stats === 'object') {
    return extractSpeed(stats, direction);
  }

  return 0;
}

function extractTotalBytes(data: unknown, direction: 'up' | 'down'): number {
  if (!data || typeof data !== 'object') return 0;

  const upKeys = [
    'totalUploadBytes', 'uploadTotal', 'totalUpload', 'totalUp',
    'uploadBytes', 'txBytes', 'totalTx', 'outboundTotal',
    'totalOutbound', 'txTotal',
  ];
  const downKeys = [
    'totalDownloadBytes', 'downloadTotal', 'totalDownload', 'totalDown',
    'downloadBytes', 'rxBytes', 'totalRx', 'inboundTotal',
    'totalInbound', 'rxTotal',
  ];
  const keys = direction === 'up' ? upKeys : downKeys;

  for (const key of keys) {
    const val = (data as Record<string, unknown>)[key];
    if (typeof val === 'number' && isFinite(val) && val >= 0) return val;
  }

  // Try nested stats/traffic object
  const nested = (data as Record<string, unknown>)['stats']
    || (data as Record<string, unknown>)['data']
    || (data as Record<string, unknown>)['traffic'];
  if (nested && typeof nested === 'object') {
    return extractTotalBytes(nested, direction);
  }

  return 0;
}

function extractConnections(data: unknown): number {
  if (!data || typeof data !== 'object') return 0;
  const keys = ['connections', 'connectionCount', 'activeConnections', 'sessions', 'activeFlows'];
  for (const key of keys) {
    const val = (data as Record<string, unknown>)[key];
    if (typeof val === 'number' && isFinite(val)) return val;
  }
  return 0;
}

function extractNodes(data: unknown): ProxyNode[] {
  if (!data || typeof data !== 'object') return [];

  const obj = data as Record<string, unknown>;
  const candidates: unknown[] = [];

  for (const key of ['proxies', 'outbounds', 'nodes', 'proxyNodes', 'servers']) {
    const arr = obj[key];
    if (Array.isArray(arr)) candidates.push(...arr);
  }

  // Also check nested objects
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
  const groups = valuesFromContainer(data, ['policies', 'policy_groups', 'policyGroups', 'groups', 'items']);
  const nodes = groups.flatMap((group) => {
    if (!group || typeof group !== 'object') return [];
    const obj = group as Record<string, unknown>;
    const selected = stringFrom(obj, ['selected', 'current', 'now', 'target']);
    return valuesFromContainer(obj, ['members', 'targets', 'children', 'proxies', 'items'])
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

class OverviewDataStore {
  speedHistory = $state<{ up: number; down: number }[]>(Array(MAX_HISTORY).fill(null).map(() => ({ up: 0, down: 0 })));
  proxyNodes = $state<ProxyNode[]>([]);
  activeConnections = $state(0);
  isLive = $state(false);
  totalUpBytes = $state(0);
  totalDownBytes = $state(0);

  // Derived: human-readable cumulative totals in MB
  get totalUpMB() { return this.totalUpBytes / 1_000_000; }
  get totalDownMB() { return this.totalDownBytes / 1_000_000; }

  private _statsTimer: ReturnType<typeof setInterval> | null = null;
  private _runtimeTimer: ReturnType<typeof setInterval> | null = null;
  private _runtimeTicks = 0;

  start() {
    if (this._statsTimer) return;
    this._pollStats();
    this._pollRuntime();
    this._statsTimer = setInterval(() => this._pollStats(), STATS_INTERVAL_MS);
    this._runtimeTimer = setInterval(() => this._pollRuntime(), RUNTIME_INTERVAL_MS);
  }

  stop() {
    if (this._statsTimer) { clearInterval(this._statsTimer); this._statsTimer = null; }
    if (this._runtimeTimer) { clearInterval(this._runtimeTimer); this._runtimeTimer = null; }
  }

  /** Called by core-events service when a stats event arrives via gui:event stream. */
  applyStatsEvent(data: Record<string, unknown>) {
    this.isLive = true;
    const up = extractSpeed(data, 'up');
    const down = extractSpeed(data, 'down');

    this.speedHistory.push({ up, down });
    if (this.speedHistory.length > MAX_HISTORY) {
      this.speedHistory.shift();
    }

    this.activeConnections = extractConnections(data);
    this.totalUpBytes = extractTotalBytes(data, 'up');
    this.totalDownBytes = extractTotalBytes(data, 'down');
  }

  /** Called by core-events service when a runtime/config event arrives. */
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

  private async _pollStats() {
    try {
      const result = await getCoreStats();
      if (!result.available || !result.response) {
        this.isLive = false;
        return;
      }
      this.isLive = true;

      const up = extractSpeed(result.response, 'up');
      const down = extractSpeed(result.response, 'down');

      this.speedHistory.push({ up, down });
      if (this.speedHistory.length > MAX_HISTORY) {
        this.speedHistory.shift();
      }

      this.activeConnections = extractConnections(result.response);
      this.totalUpBytes = extractTotalBytes(result.response, 'up');
      this.totalDownBytes = extractTotalBytes(result.response, 'down');
    } catch {
      this.isLive = false;
    }
  }

  private async _pollRuntime() {
    this._runtimeTicks++;
    // Poll runtime less frequently or on first tick
    if (this._runtimeTicks > 1 && this._runtimeTicks % (RUNTIME_INTERVAL_MS / STATS_INTERVAL_MS) !== 0) return;

    try {
      const result = await getCoreRuntime();
      if (!result.available || !result.response) return;

      const nodes = extractNodes(result.response);
      if (nodes.length > 0) {
        this.proxyNodes = nodes;
      } else {
        await this.refreshPolicyNodes();
      }
    } catch {
      // Runtime might not be available yet
    }
  }
}

export const overviewData = new OverviewDataStore();
