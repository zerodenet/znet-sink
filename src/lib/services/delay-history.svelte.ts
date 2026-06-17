// Per-node latency history for hover sparklines.
//
// Records the last N probe results per outbound tag, persisted to
// localStorage so history survives tab switches and app restarts.
// Used by the NodesTab delay popover to show a mini trend chart.

import { browser } from '$app/environment';

const STORAGE_KEY = 'znet-delay-history';
const MAX_ENTRIES = 20; // per node
const MAX_NODES = 500; // bound total memory footprint
const PRUNE_AFTER_MS = 1000 * 60 * 60 * 24 * 7; // 7 days

export interface DelayEntry {
  /** Latency in ms. `-1` = timeout/unreachable, `0` = idle/zero, `>0` = latency. */
  delay: number;
  /** Unix-ms timestamp of the probe. */
  at: number;
}

type HistoryMap = Record<string, DelayEntry[]>;

function load(): HistoryMap {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as HistoryMap;
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

class DelayHistoryStore {
  history = $state<HistoryMap>({});

  constructor() {
    this.history = load();
  }

  /** Record a probe result for a node. */
  record(tag: string, delayMs: number | undefined, reachable: boolean): void {
    if (!tag) return;
    // `-1` marks a timeout / unreachable probe (e.g. kernel not running) so
    // the UI can show "timeout" instead of mistaking it for "never probed".
    const value = reachable ? Math.max(0, delayMs ?? 0) : -1;
    const entry: DelayEntry = { delay: value, at: Date.now() };

    const existing = this.history[tag] ?? [];
    const next = [...existing, entry];
    if (next.length > MAX_ENTRIES) {
      next.splice(0, next.length - MAX_ENTRIES);
    }

    this.history = { ...this.history, [tag]: next };
    this.persist();
  }

  /** Get the ordered history entries for a node (oldest → newest). */
  getHistory(tag: string): DelayEntry[] {
    return this.history[tag] ?? [];
  }

  /** Latest known latency for a node, or undefined if never probed. */
  latest(tag: string): number | undefined {
    const entries = this.history[tag];
    if (!entries || entries.length === 0) return undefined;
    return entries[entries.length - 1].delay;
  }

  /** Prune stale entries (older than PRUNE_AFTER_MS) and over-sized maps. */
  prune(): void {
    const cutoff = Date.now() - PRUNE_AFTER_MS;
    const tags = Object.keys(this.history);
    let changed = false;
    const next: HistoryMap = {};

    for (const tag of tags) {
      const fresh = (this.history[tag] ?? []).filter((e) => e.at >= cutoff);
      if (fresh.length > 0) next[tag] = fresh;
      if (fresh.length !== (this.history[tag]?.length ?? 0)) changed = true;
    }

    // Hard cap on total node count — drop oldest-tagged entries first.
    const keys = Object.keys(next);
    if (keys.length > MAX_NODES) {
      const latestAt = (entries: DelayEntry[] | undefined): number =>
        entries && entries.length > 0 ? entries[entries.length - 1].at : 0;
      const sorted = keys.sort((a, b) => latestAt(next[b]) - latestAt(next[a]));
      for (const k of sorted.slice(MAX_NODES)) delete next[k];
      changed = true;
    }

    if (changed) {
      this.history = next;
      this.persist();
    }
  }

  /** Remove all history for a single node. */
  clear(tag: string): void {
    if (!this.history[tag]) return;
    const next = { ...this.history };
    delete next[tag];
    this.history = next;
    this.persist();
  }

  /** Remove all history. */
  clearAll(): void {
    this.history = {};
    this.persist();
  }

  private persist(): void {
    if (!browser) return;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.history));
    } catch {
      // Storage may be full / unavailable — history is best-effort only.
    }
  }
}

export const delayHistory = new DelayHistoryStore();

// Prune once on module load so stale data doesn't linger.
if (browser) {
  delayHistory.prune();
}
