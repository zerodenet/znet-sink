import { listen } from '@tauri-apps/api/event';
import { proxyConfigSignal } from './proxy-config-signal.svelte';

/** One app-level bridge keeps both layouts current after scheduler/plugin publication. */
export function listenConfigurationSources(): Promise<() => void> {
  return listen('subscriptions:updated', () => proxyConfigSignal.markChanged(true));
}
