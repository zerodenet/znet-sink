import { removalConfirmed } from '$lib/features/profile-removal/types';
import type { RemovalGateway, RemovalInput, RemovalPreview, RemovalResult } from '$lib/features/profile-removal/types';
type Invoke = <T>(command: string, args: Record<string, unknown>) => Promise<T>;
// Publication notifications follow verified receipts, never a successful IPC
// transport response, a local selection, or an unconfirmed runtime transaction.
export function createRemovalGateway(invoke: Invoke, changed: () => void): RemovalGateway {
  return {
    preview: id => invoke<RemovalPreview>('profile_removal_preview', { id }),
    async commit(input: RemovalInput) {
      const result = await invoke<RemovalResult>('profile_removal_commit', { input });
      if (removalConfirmed(input, result)) changed();
      return result;
    },
    async reconcile(id) {
      const result = await invoke<RemovalResult>('profile_removal_reconcile', { id });
      if (result.status === 'confirmed' && result.removedId === id) changed();
      return result;
    },
  };
}
