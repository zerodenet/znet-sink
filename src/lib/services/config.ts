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
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  await reconcileTunAfterConfigMutation('profile upsert');
  return profile;
}

export async function importProxyConfig(input: ProxyConfigImport): Promise<ProxyConfigProfile> {
  const profile = await invoke<ProxyConfigProfile>('proxy_config_import', { input });
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  await reconcileTunAfterConfigMutation('profile import');
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
  await invoke('proxy_config_remove', { id });
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  await reconcileTunAfterConfigMutation('profile removal');
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
  // A sync can create/update the generated proxy profile and may hot-refresh
  // the active target, so every config-backed surface must reconcile.
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  await reconcileTunAfterConfigMutation('subscription sync');
  return subscription;
}

export async function resetSubscriptionConditionalCache(id:string,expectedUpdatedAtUnixMs:number):Promise<{cleared:boolean}>{
  if(!id || !Number.isSafeInteger(expectedUpdatedAtUnixMs) || expectedUpdatedAtUnixMs<0)throw new Error('来源版本无法确认，请刷新后重新预览。');
  const result=await invoke<{cleared:boolean}>('subscription_reset_conditional_cache',{id,expectedUpdatedAtUnixMs:String(expectedUpdatedAtUnixMs)});
  if(typeof result?.cleared!=='boolean')throw new Error('条件记录清理回执未确认，请先核对。');
  return result;
}

export interface SubscriptionReconciliation {
  status: 'result-unknown' | 'local-publication-pending';
  source: { id: string; updatedAtUnixMs: string; lastSyncAtUnixMs: string | null };
  material: {
    status: 'saved-valid' | 'saved-invalid' | 'missing' | 'not-linked' | 'ownership-mismatch';
    profileId: string | null; updatedAtUnixMs: string | null; active: boolean | null;
    validation: 'native-schema-only' | 'not-available' | 'not-inspected';
  };
  evidence: { candidate: 'not-recorded' | 'recorded'; receipt: 'not-recorded' | 'confirmed' };
  runtime: {
    status: 'readable' | 'unavailable' | 'changed' | 'unsupported';
    coreInstanceId: string | null; configRevision: string | null;
    reasonCode: string; reason: string;
  };
  canResolve: boolean;
  recoveryId: string | null;
  reason: string;
}

function isU64Decimal(value: unknown): value is string {
  return typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value)
    && value.length <= 20 && BigInt(value) <= 18446744073709551615n;
}

/** Read-only result inspection. Never reloads profiles or reconciles TUN. */
export async function reconcileUnknownSubscription(
  id: string, expectedUpdatedAtUnixMs: number, expectedTargetProfileId: string | null,
): Promise<SubscriptionReconciliation> {
  const fail = () => new Error('订阅结果核对回执未确认，请刷新后重试。');
  if (!id || id.trim() !== id || !Number.isSafeInteger(expectedUpdatedAtUnixMs) || expectedUpdatedAtUnixMs < 0
    || !(expectedTargetProfileId === null || (typeof expectedTargetProfileId === 'string' && expectedTargetProfileId.length > 0))) {
    throw new Error('来源版本或关联目标无法确认，请刷新后重新核对。');
  }
  const result = await invoke<SubscriptionReconciliation>('subscription_reconcile_unknown', {
    id, expectedUpdatedAtUnixMs: String(expectedUpdatedAtUnixMs),
  });
  if (!result || typeof result.canResolve !== 'boolean'
    || result.status !== (result.canResolve ? 'local-publication-pending' : 'result-unknown')
    || !(result.recoveryId === null || isRecoveryId(result.recoveryId))
    || result.source?.id !== id || result.source.updatedAtUnixMs !== String(expectedUpdatedAtUnixMs)
    || !isU64Decimal(result.source.updatedAtUnixMs)
    || !(result.source.lastSyncAtUnixMs === null || isU64Decimal(result.source.lastSyncAtUnixMs))
    || !['not-recorded', 'recorded'].includes(result.evidence?.candidate)
    || !['not-recorded', 'confirmed'].includes(result.evidence?.receipt)
    || (result.evidence.receipt === 'confirmed' && result.evidence.candidate !== 'recorded')
    || (result.evidence.candidate === 'recorded') !== (result.recoveryId !== null)
    || typeof result.reason !== 'string' || !result.reason
    || !result.material || result.material.profileId !== expectedTargetProfileId
    || !['saved-valid', 'saved-invalid', 'missing', 'not-linked', 'ownership-mismatch'].includes(result.material.status)
    || !result.runtime || !['readable', 'unavailable', 'changed', 'unsupported'].includes(result.runtime.status)
    || typeof result.runtime.reason !== 'string' || !result.runtime.reason) {
    throw fail();
  }
  const material = result.material;
  if (material.status === 'not-linked' || material.status === 'missing') {
    if (material.updatedAtUnixMs !== null || material.active !== null || material.validation !== 'not-available'
      || (material.status === 'not-linked') !== (material.profileId === null)) throw fail();
  } else if (!material.profileId || !isU64Decimal(material.updatedAtUnixMs) || typeof material.active !== 'boolean'
    || material.validation !== (material.status === 'ownership-mismatch' ? 'not-inspected' : 'native-schema-only')) {
    throw fail();
  }
  const runtime = result.runtime;
  const validReasonCodes = {
    readable: ['runtime_identity_only', 'confirmed_receipt_matches'], unavailable: ['runtime_identity_unreadable'],
    changed: ['runtime_identity_changed'],
    unsupported: ['mobile_runtime_bridge_not_adapted', 'kernel_identity_not_supported'],
  };
  if (!validReasonCodes[runtime.status].includes(runtime.reasonCode)) throw fail();
  if (runtime.status === 'readable') {
    if (typeof runtime.coreInstanceId !== 'string' || !runtime.coreInstanceId.trim() || runtime.coreInstanceId.length > 512
      || /[\u0000-\u001f\u007f]/.test(runtime.coreInstanceId) || !isU64Decimal(runtime.configRevision)) throw fail();
  } else if (runtime.coreInstanceId !== null || runtime.configRevision !== null) throw fail();
  if (result.canResolve && (result.evidence.candidate !== 'recorded' || result.evidence.receipt !== 'confirmed'
    || !isRecoveryId(result.recoveryId) || runtime.status !== 'readable'
    || runtime.reasonCode !== 'confirmed_receipt_matches' || material.status === 'ownership-mismatch')) throw fail();
  return result;
}

function isRecoveryId(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 256 && /^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(value);
}

/** Publishes only a previously confirmed, host-persisted candidate. No sync/apply/TUN action. */
export async function recoverConfirmedSubscriptionLocal(
  id: string, expectedUpdatedAtUnixMs: string, recoveryId: string,
): Promise<SubscriptionProfile> {
  if (!id || id.trim() !== id || !isU64Decimal(expectedUpdatedAtUnixMs) || !isRecoveryId(recoveryId)) {
    throw new Error('来源版本或恢复记录无法确认，请重新核对。');
  }
  let result: SubscriptionProfile;
  try {
    result = await invoke<SubscriptionProfile>('subscription_recover_confirmed_local', { id, expectedUpdatedAtUnixMs, recoveryId });
  } catch {
    throw new Error('本地保存未确认，原核对记录保留；请检查后再决定是否重新核对。');
  }
  if (!result || result.id !== id || result.managedSource != null || !Number.isSafeInteger(result.updatedAtUnixMs)
    || result.updatedAtUnixMs < 0 || BigInt(result.updatedAtUnixMs) < BigInt(expectedUpdatedAtUnixMs)) {
    throw new Error('本地保存回执未确认，请刷新订阅状态核对；不要自动重复提交。');
  }
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  return result;
}

export async function syncAllSubscriptions(): Promise<SubscriptionSyncAllOutcome> {
  const outcome = await invoke<SubscriptionSyncAllOutcome>('subscription_sync_all');
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  await reconcileTunAfterConfigMutation('subscription sync all');
  return outcome;
}

export async function getSubscriptionRemovalPreview(id: string): Promise<SubscriptionRemovalPreview> {
  return invoke('subscription_remove_preview', { id });
}

export async function removeSubscription(
  id: string,
  removeAssociatedConfig = false,
): Promise<SubscriptionRemovalOutcome> {
  const outcome = await invoke<SubscriptionRemovalOutcome>('subscription_remove', {
    id,
    removeAssociatedConfig,
  });
  if (outcome.removedProxyConfig) proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  await reconcileTunAfterConfigMutation('subscription removal');
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
