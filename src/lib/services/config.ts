import { invoke } from '@tauri-apps/api/core';
import type {
  ProxyConfigProfile,
  ProxyConfigUpsert,
  ProxyConfigImport,
  SubscriptionProfile,
  SubscriptionUpsert,
  SubscriptionSyncAllOutcome,
  RuleSetProfile,
  RuleSetUpsert,
  RuleSetKernelPayload,
  RuleSetSyncAllOutcome,
  CommonRuleBindingInput,
  CommonRuleInjectionStatus,
} from '$lib/types/domain';

export type {
  ProxyConfigProfile,
  SubscriptionProfile,
  SubscriptionSyncAllOutcome,
  RuleSetProfile,
};

// ── Proxy configs ──

export async function listProxyConfigs(): Promise<ProxyConfigProfile[]> {
  return invoke('proxy_config_list');
}

export async function getProxyConfig(id: string): Promise<ProxyConfigProfile> {
  return invoke('proxy_config_get', { id });
}

export async function upsertProxyConfig(input: ProxyConfigUpsert): Promise<ProxyConfigProfile> {
  return invoke('proxy_config_upsert', { input });
}

export async function importProxyConfig(input: ProxyConfigImport): Promise<ProxyConfigProfile> {
  return invoke('proxy_config_import', { input });
}

export async function setActiveProxyConfig(id: string): Promise<ProxyConfigProfile> {
  return invoke('proxy_config_set_active', { id });
}

export async function removeProxyConfig(id: string): Promise<void> {
  return invoke('proxy_config_remove', { id });
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
  return invoke('subscription_sync', { id });
}

export async function syncAllSubscriptions(): Promise<SubscriptionSyncAllOutcome> {
  return invoke('subscription_sync_all');
}

export async function removeSubscription(id: string): Promise<void> {
  return invoke('subscription_remove', { id });
}

// ── Rule sets ──

export async function listRuleSets(): Promise<RuleSetProfile[]> {
  return invoke('rule_set_list');
}

export async function getRuleSet(id: string): Promise<RuleSetProfile> {
  return invoke('rule_set_get', { id });
}

export async function upsertRuleSet(input: RuleSetUpsert): Promise<RuleSetProfile> {
  return invoke('rule_set_upsert', { input });
}

export async function removeRuleSet(id: string): Promise<void> {
  return invoke('rule_set_remove', { id });
}

export async function updateRuleSet(id: string): Promise<RuleSetProfile> {
  return invoke('rule_set_update', { id });
}

export async function updateAllRuleSets(): Promise<RuleSetSyncAllOutcome> {
  return invoke('rule_set_update_all');
}

export async function getRuleSetKernelPayloads(): Promise<RuleSetKernelPayload[]> {
  return invoke('rule_set_kernel_payloads');
}

export async function getCommonRuleInjectionStatus(): Promise<CommonRuleInjectionStatus> {
  return invoke('rule_set_common_status');
}

export async function setCommonRuleInjectionEnabled(enabled: boolean): Promise<CommonRuleInjectionStatus> {
  return invoke('rule_set_set_common_enabled', { enabled });
}

export async function setCommonRuleBinding(input: CommonRuleBindingInput): Promise<RuleSetProfile> {
  return invoke('rule_set_set_common_binding', { input });
}
