import { SettingsMigration, type MigrationState } from '$lib/features/mobile/settings-migration';
import { settingsTransferGateway } from '$lib/services/settings-transfer';
import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte';

// Presentation state survives navigation; the native host owns plans and saves.
// There is no second job queue, UI timer or persistent configuration copy here.
const listeners = new Set<(state: MigrationState) => void>();
export const settingsMigrationSession = new SettingsMigration(settingsTransferGateway, state => {
  for (const listener of listeners) listener(state);
});
proxyConfigSignal.onActiveChanged(() => settingsMigrationSession.invalidate());
export function subscribeSettingsMigration(listener: (state: MigrationState) => void): () => void {
  listeners.add(listener);
  listener(settingsMigrationSession.state);
  return () => { listeners.delete(listener); };
}
