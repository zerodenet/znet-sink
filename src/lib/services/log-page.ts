import type { LogEntry, LogPage } from '$lib/types/logs';

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
    merged.set(entry.id, entry);
  }

  return Array.from(merged.values()).sort((a, b) => a.id - b.id);
}
