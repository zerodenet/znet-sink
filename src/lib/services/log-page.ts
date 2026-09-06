import type { LogEntry, LogPage } from '$lib/types/logs';

const fieldKeys = new WeakMap<LogEntry, string | undefined>();
function fieldsKey(entry: LogEntry) {
  if (!fieldKeys.has(entry)) fieldKeys.set(entry, JSON.stringify(entry.fields));
  return fieldKeys.get(entry);
}
function sameEntry(left: LogEntry, right: LogEntry) {
  return left.id === right.id && left.source === right.source && left.level === right.level
    && left.message === right.message && left.occurredAtUnixMs === right.occurredAtUnixMs
    && fieldsKey(left) === fieldsKey(right);
}

/**
 * Merge log pages into a unique, ascending ID sequence.
 *
 * Persisted logs can contain duplicate IDs after overlapping app instances or
 * an interrupted historical write. The latest occurrence wins so keyed Svelte
 * lists never receive duplicate identities.
 */
export function mergeLogPage(current: LogEntry[], page: LogPage): LogEntry[] {
  const merged = new Map<number, LogEntry>();
  const oldestAvailableId = page.oldestAvailableId;

  for (const entry of current) {
    if (oldestAvailableId == null || entry.id >= oldestAvailableId) {
      merged.set(entry.id, entry);
    }
  }
  for (const entry of page.items) {
    const existing = merged.get(entry.id);
    merged.set(entry.id, existing && sameEntry(existing, entry) ? existing : entry);
  }

  const next = Array.from(merged.values()).sort((a, b) => a.id - b.id);
  return next.length === current.length && next.every((entry, index) => entry === current[index]) ? current : next;
}
