import { connectionLifecycleKey } from '$lib/services/connection-view';
import type { GuiConnectionItem } from '$lib/types/gui-api';

/** A refreshed first page must overlap the loaded window before merging it.
 * Otherwise the old cursor would skip records between the new and old pages. */
export function historyHeadReplacesWindow(
  current: GuiConnectionItem[],
  head: GuiConnectionItem[],
  hasMore: boolean,
): boolean {
  if (!hasMore) return true; // A filter/retention change may have removed old rows.
  if (current.length === 0) return true;
  const identities = new Set(current.map(connectionLifecycleKey));
  return !head.some(item => identities.has(connectionLifecycleKey(item)));
}
