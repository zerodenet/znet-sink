// Per-node latency history for hover sparklines.
//
// Records the last N probe results per outbound tag, persisted to
// localStorage so history survives tab switches and app restarts.
// Histories are namespaced by active proxy configuration so profiles that
// reuse node or policy tags cannot merge into one another.

import { browser } from '$app/environment';
import { guiState } from '$lib/services/gui-state.svelte';
import {
  buildDelayHistoryScope,
  planDelayHistoryScopeTransition,
  splitDelayHistoryScope,
} from '$lib/services/delay-history-scope';

const STORAGE_KEY = 'znet-delay-history-v2';
const LEGACY_STORAGE_KEY = 'znet-delay-history';
const MAX_ENTRIES = 20; // per node
const MAX_NODES = 500; // across all configuration scopes
const PRUNE_AFTER_MS = 1000 * 60 * 60 * 24 * 7; // 7 days
const EMPTY_FINGERPRINT = splitDelayHistoryScope(
  buildDelayHistoryScope(undefined, [], []),
).fingerprint;

export interface DelayEntry {
  /** Latency in ms. `-1` = timeout/unreachable, `0` = idle/zero, `>0` = latency. */
  delay: number;
  /** Unix-ms timestamp of the probe. */
  at: number;
  /** Policy-group history only: selected member that produced this result. */
  selectedTag?: string;
}

type HistoryMap = Record<string, DelayEntry[]>;
type ScopedHistoryMap = Record<string, HistoryMap>;

function loadScoped(): ScopedHistoryMap {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as ScopedHistoryMap;
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function loadLegacy(): HistoryMap {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as HistoryMap;
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function mergeEntries(left: DelayEntry[], right: DelayEntry[]): DelayEntry[] {
  const merged = [...left, ...right]
    .sort((a, b) => a.at - b.at)
    .filter((entry, index, entries) => index === 0 || !(
      entries[index - 1].at === entry.at
      && entries[index - 1].delay === entry.delay
      && entries[index - 1].selectedTag === entry.selectedTag
    ));
  return merged.length > MAX_ENTRIES
    ? merged.slice(merged.length - MAX_ENTRIES)
    : merged;
}

function mergeHistory(left: HistoryMap, right: HistoryMap): HistoryMap {
  const merged: HistoryMap = { ...left };
  for (const [tag, entries] of Object.entries(right)) {
    merged[tag] = mergeEntries(merged[tag] ?? [], entries);
  }
  return merged;
}

class DelayHistoryStore {
  private scopedHistory = $state<ScopedHistoryMap>({});
  private legacyHistory: HistoryMap = {};
  private lastScope: string | null = null;
  private provisionalScope: string | null = null;

  constructor() {
    this.scopedHistory = loadScoped();
    this.legacyHistory = loadLegacy();
  }

  private currentScope(): string {
    const candidate = buildDelayHistoryScope(
      guiState.selfTest?.activeProxyConfigId,
      guiState.configNodes,
      guiState.configPolicyGroups,
    );
    const transition = planDelayHistoryScopeTransition(
      this.lastScope,
      candidate,
      this.provisionalScope,
      EMPTY_FINGERPRINT,
    );

    if (transition.migrateFrom && transition.migrateFrom !== candidate) {
      this.mergeScope(transition.migrateFrom, candidate);
    }
    this.provisionalScope = transition.provisionalScope;
    this.lastScope = candidate;
    this.importLegacy(candidate);
    return candidate;
  }

  private mergeScope(source: string, target: string): void {
    const sourceHistory = this.scopedHistory[source];
    if (!sourceHistory) return;
    const next = { ...this.scopedHistory };
    next[target] = mergeHistory(next[target] ?? {}, sourceHistory);
    delete next[source];
    this.scopedHistory = next;
    this.persist();
  }

  /** Import tag-only history into the first resolved active scope once.
   * Perfect profile separation is impossible for already-global legacy data,
   * so it is assigned only to the currently active configuration and then the
   * legacy key is removed. It will not reappear in every later profile. */
  private importLegacy(target: string): void {
    if (Object.keys(this.legacyHistory).length === 0) return;
    this.scopedHistory = {
      ...this.scopedHistory,
      [target]: mergeHistory(this.scopedHistory[target] ?? {}, this.legacyHistory),
    };
    this.legacyHistory = {};
    if (browser) {
      try {
        localStorage.removeItem(LEGACY_STORAGE_KEY);
      } catch {
        // Storage cleanup is best effort.
      }
    }
    this.persist();
  }

  /** Current configuration's history map. Reading this also tracks the active
   * profile and config structure as Svelte dependencies. */
  get history(): HistoryMap {
    return this.scopedHistory[this.currentScope()] ?? {};
  }

  /** Record a probe result for a node or policy group in the active config. */
  record(
    tag: string,
    delayMs: number | undefined,
    reachable: boolean,
    at = Date.now(),
    selectedTag?: string,
  ): void {
    if (!tag) return;
    const scope = this.currentScope();
    const scoped = this.scopedHistory[scope] ?? {};
    // `-1` marks a timeout / unreachable probe (e.g. kernel not running) so
    // the UI can show "timeout" instead of mistaking it for "never probed".
    const value = reachable ? Math.max(0, delayMs ?? 0) : -1;
    const entry: DelayEntry = {
      delay: value,
      at,
      ...(selectedTag ? { selectedTag } : {}),
    };

    const existing = scoped[tag] ?? [];
    if (existing.some((item) =>
      item.at === entry.at
      && item.delay === entry.delay
      && item.selectedTag === entry.selectedTag
    )) return;
    const next = [...existing, entry].sort((left, right) => left.at - right.at);
    if (next.length > MAX_ENTRIES) {
      next.splice(0, next.length - MAX_ENTRIES);
    }

    this.scopedHistory = {
      ...this.scopedHistory,
      [scope]: { ...scoped, [tag]: next },
    };
    this.persist();
  }

  /** Get the active configuration's ordered entries (oldest → newest). */
  getHistory(tag: string): DelayEntry[] {
    return this.history[tag] ?? [];
  }

  /** Latest known latency in the active configuration. */
  latest(tag: string): number | undefined {
    const entries = this.history[tag];
    if (!entries || entries.length === 0) return undefined;
    return entries[entries.length - 1].delay;
  }

  /** Timestamp of the latest probe in the active configuration. */
  latestTime(tag: string): number | undefined {
    const entries = this.history[tag];
    if (!entries || entries.length === 0) return undefined;
    return entries[entries.length - 1].at;
  }

  /** Prune stale entries and enforce one total node bound across profiles. */
  prune(): void {
    const cutoff = Date.now() - PRUNE_AFTER_MS;
    let changed = false;
    const next: ScopedHistoryMap = {};

    for (const [scope, history] of Object.entries(this.scopedHistory)) {
      const scopedNext: HistoryMap = {};
      for (const [tag, entries] of Object.entries(history)) {
        const fresh = entries.filter((entry) => entry.at >= cutoff);
        if (fresh.length > 0) scopedNext[tag] = fresh;
        if (fresh.length !== entries.length) changed = true;
      }
      if (Object.keys(scopedNext).length > 0) next[scope] = scopedNext;
      else if (Object.keys(history).length > 0) changed = true;
    }

    const records = Object.entries(next).flatMap(([scope, history]) =>
      Object.entries(history).map(([tag, entries]) => ({ scope, tag, entries })),
    );
    if (records.length > MAX_NODES) {
      records.sort((left, right) => {
        const leftAt = left.entries[left.entries.length - 1]?.at ?? 0;
        const rightAt = right.entries[right.entries.length - 1]?.at ?? 0;
        return rightAt - leftAt;
      });
      for (const record of records.slice(MAX_NODES)) {
        delete next[record.scope]?.[record.tag];
        if (next[record.scope] && Object.keys(next[record.scope]).length === 0) {
          delete next[record.scope];
        }
      }
      changed = true;
    }

    if (changed) {
      this.scopedHistory = next;
      this.persist();
    }
  }

  /** Remove history for one tag in the active configuration. */
  clear(tag: string): void {
    const scope = this.currentScope();
    const scoped = this.scopedHistory[scope];
    if (!scoped?.[tag]) return;
    const nextScope = { ...scoped };
    delete nextScope[tag];
    const next = { ...this.scopedHistory };
    if (Object.keys(nextScope).length > 0) next[scope] = nextScope;
    else delete next[scope];
    this.scopedHistory = next;
    this.persist();
  }

  /** Remove all history for the active configuration only. */
  clearAll(): void {
    const scope = this.currentScope();
    if (!this.scopedHistory[scope]) return;
    const next = { ...this.scopedHistory };
    delete next[scope];
    this.scopedHistory = next;
    this.persist();
  }

  private persist(): void {
    if (!browser) return;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.scopedHistory));
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
