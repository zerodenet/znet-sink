import { invoke } from '@tauri-apps/api/core';
import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte';
import { ruleSetSignal } from '$lib/services/rule-set-signal.svelte';
import { createRemovalGateway } from './gateway';
export const profileRemovalGateway = createRemovalGateway(invoke, () => {
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
});
