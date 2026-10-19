import { ProfileActivation } from '$lib/features/configuration/profile-activation';
import { invoke } from '@tauri-apps/api/core';
import { coreEvents } from './core-events.svelte';
import { proxyConfigSignal } from './proxy-config-signal.svelte';
import { ruleSetSignal } from './rule-set-signal.svelte';
import {
  reconcileGuiTunRuntime,
} from './tun';
import type {
  ProxyConfigProfile,
  ProxyConfigUpsert,
  ProxyConfigImport,
  SubscriptionProfile,
  SubscriptionUpsert,
  SubscriptionSyncAllOutcome,
  SubscriptionRemovalPreview,
  SubscriptionRemovalOutcome,
  RuleSetProfile,
  RuleSetSummary,
  RuleSetUpsert,
  RuleSetKernelPayload,
  RuleSetSyncAllOutcome,
  CommonRuleBindingInput,
  CommonRuleInjectionStatus,
  EffectiveRuleSetOption,
} from '$lib/types/domain';

export type {
  ProxyConfigProfile,
  SubscriptionProfile,
  SubscriptionSyncAllOutcome,
  RuleSetProfile,
  RuleSetSummary,
};

function reconcileTunAfterConfigMutation(context: string): Promise<void> {
  return reconcileGuiTunRuntime()
    .then(() => undefined)
    .catch((error) => {
      // A profile mutation has already committed by the time this runs. Keep
      // the configuration change authoritative and surface runtime divergence
      // through the normal TUN observed/desired status instead of pretending
      // the profile switch itself failed.
      console.warn(`[config] TUN reconciliation after ${context} failed`, error);
    });
}

// ── Proxy configs ──

export async function listProxyConfigs(): Promise<ProxyConfigProfile[]> {
  // Read the shared revision before the async boundary. When this function is
  // called from a Svelte effect (ProfilesTab, config editor mount, etc.), the
  // effect becomes dependent on successful proxy-config mutations no matter
  // which interaction mode initiated them.
  void proxyConfigSignal.revision;
  return invoke('proxy_config_list');
}

export async function getProxyConfig(id: string): Promise<ProxyConfigProfile> {
  return invoke('proxy_config_get', { id });
}

export async function upsertProxyConfig(input: ProxyConfigUpsert): Promise<ProxyConfigProfile> {
  const profile = await invoke<ProxyConfigProfile>('proxy_config_upsert', { input });
  proxyConfigSignal.markChanged(profile.active);
  ruleSetSignal.markChanged();
  if (profile.active) await reconcileTunAfterConfigMutation('profile upsert');
  return profile;
}

export async function importProxyConfig(input: ProxyConfigImport): Promise<ProxyConfigProfile> {
  const profile = await invoke<ProxyConfigProfile>('proxy_config_import', { input });
  proxyConfigSignal.markChanged(profile.active);
  ruleSetSignal.markChanged();
  if (profile.active) await reconcileTunAfterConfigMutation('profile import');
  return profile;
}

const profileActivation = new ProfileActivation({
  load: getProxyConfig,
  commit: (id: string) => invoke<ProxyConfigProfile>('proxy_config_set_active', { id }),
  restartObservation: async () => { await coreEvents.stop(); await coreEvents.start(); },
  changed: () => { proxyConfigSignal.markChanged(true); ruleSetSignal.markChanged(); },
  reconcile: () => reconcileTunAfterConfigMutation('profile activation'),
  warn: (context: string, error: unknown) => console.warn(`[config] ${context} failed`, error),
});

export function setActiveProxyConfig(id: string): Promise<ProxyConfigProfile> {
  return profileActivation.activate(id);
}

/** Latest composition metadata, not proof of a successful kernel apply. */
export function getConfigCompositionReport(): Promise<{sourceProfileId: string | null; layers: {source: string; changedPaths: string[]}[]} | null> {
  return invoke('proxy_config_composition_report');
}

export async function removeProxyConfig(id: string): Promise<void> {
  const previous = await getProxyConfig(id);
  await invoke('proxy_config_remove', { id });
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  if (previous.active) await reconcileTunAfterConfigMutation('profile removal');
}

// ── Subscriptions ──

export async function listSubscriptions(): Promise<SubscriptionProfile[]> {
  return invoke('subscription_list');
}

export async function getSubscription(id: string): Promise<SubscriptionProfile> {
  return invoke('subscription_get', { id });
}

export async function upsertSubscription(input: SubscriptionUpsert): Promise<SubscriptionProfile> {
  return invoke('subscription_upsert', { input });
}

export async function syncSubscription(id: string): Promise<SubscriptionProfile> {
  const subscription = await invoke<SubscriptionProfile>('subscription_sync', { id });
  // The backend updates the linked profile and applies it only when active.
  // It owns capture preservation in that transaction; syncing an unrelated
  // subscription must never re-enable a saved but currently stopped TUN.
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  return subscription;
}

export async function syncAllSubscriptions(): Promise<SubscriptionSyncAllOutcome> {
  const outcome = await invoke<SubscriptionSyncAllOutcome>('subscription_sync_all');
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  return outcome;
}

export async function getSubscriptionRemovalPreview(id: string): Promise<SubscriptionRemovalPreview> {
  return invoke('subscription_remove_preview', { id });
}

export async function removeSubscription(
  id: string,
  removeAssociatedConfig = false,
): Promise<SubscriptionRemovalOutcome> {
  const preview = removeAssociatedConfig ? await getSubscriptionRemovalPreview(id) : null;
  const outcome = await invoke<SubscriptionRemovalOutcome>('subscription_remove', {
    id,
    removeAssociatedConfig,
  });
  if (outcome.removedProxyConfig) proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  if (outcome.removedProxyConfig && preview?.targetProxyConfig?.active) {
    await reconcileTunAfterConfigMutation('subscription removal');
  }
  return outcome;
}

// ── Rule sets ──

export async function listRuleSets(): Promise<RuleSetSummary[]> {
  return invoke('rule_set_list');
}

export async function getRuleSet(id: string): Promise<RuleSetProfile> {
  return invoke('rule_set_get', { id });
}

export async function upsertRuleSet(input: RuleSetUpsert): Promise<RuleSetProfile> {
  const result = await invoke<RuleSetProfile>('rule_set_upsert', { input });
  ruleSetSignal.markChanged();
  return result;
}

export async function removeRuleSet(id: string): Promise<void> {
  await invoke('rule_set_remove', { id });
  ruleSetSignal.markChanged();
}

export async function updateRuleSet(id: string): Promise<RuleSetProfile> {
  const result = await invoke<RuleSetProfile>('rule_set_update', { id });
  ruleSetSignal.markChanged();
  return result;
}

export async function updateAllRuleSets(): Promise<RuleSetSyncAllOutcome> {
  const result = await invoke<RuleSetSyncAllOutcome>('rule_set_update_all');
  ruleSetSignal.markChanged();
  return result;
}

export async function updateBuiltinRuleSets(): Promise<RuleSetSyncAllOutcome> {
  const result = await invoke<RuleSetSyncAllOutcome>('rule_set_update_builtins');
  ruleSetSignal.markChanged();
  return result;
}

export async function getRuleSetKernelPayloads(): Promise<RuleSetKernelPayload[]> {
  return invoke('rule_set_kernel_payloads');
}

export async function getEffectiveRuleSetOptions(): Promise<EffectiveRuleSetOption[]> {
  void ruleSetSignal.revision;
  return invoke('rule_set_effective_options');
}

export async function getCommonRuleInjectionStatus(): Promise<CommonRuleInjectionStatus> {
  return invoke('rule_set_common_status');
}

export async function setCommonRuleInjectionEnabled(enabled: boolean): Promise<CommonRuleInjectionStatus> {
  const result = await invoke<CommonRuleInjectionStatus>('rule_set_set_common_enabled', { enabled });
  ruleSetSignal.markChanged();
  return result;
}

export async function setCommonRuleBinding(input: CommonRuleBindingInput): Promise<RuleSetProfile> {
  const result = await invoke<RuleSetProfile>('rule_set_set_common_binding', { input });
  ruleSetSignal.markChanged();
  return result;
}
