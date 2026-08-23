import { invoke } from '@tauri-apps/api/core';
import {
  getAppConfig,
  getGuiConnectionStatus,
  getGuiCoreHealth,
  startCoreProcess,
  updateAppConfig,
} from './core';
import type { AppConfig } from '$lib/types/app-config';
import type { ProxyConfigProfile } from '$lib/types/domain';
import type { GuiTunStatus } from '$lib/types/gui-api';
import type { GuiManagedTunStatus, TunConfigSource } from '$lib/types/tun';

const CORE_READY_TIMEOUT_MS = 8_000;
const CORE_READY_INTERVAL_MS = 100;

interface TunPolicy {
  appConfig: AppConfig;
  profile?: ProxyConfigProfile;
  profileManaged: boolean;
  profileDesiredEnabled: boolean;
}

export interface TunProfileTransition {
  stoppedAppRuntime: boolean;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

function profileTunPolicy(content: unknown): Pick<TunPolicy, 'profileManaged' | 'profileDesiredEnabled'> {
  if (!isObject(content) || !isObject(content.runtime)) {
    return { profileManaged: false, profileDesiredEnabled: false };
  }
  const runtime = content.runtime;
  if (!Object.prototype.hasOwnProperty.call(runtime, 'tun')) {
    return { profileManaged: false, profileDesiredEnabled: false };
  }
  // `runtime.tun: null` is an explicit profile-owned disabled state. Any
  // non-null value remains profile-owned and is left for Zero to validate;
  // ZNet-Sink must not silently replace source intent with local runtime state.
  return {
    profileManaged: true,
    profileDesiredEnabled: runtime.tun !== null,
  };
}

async function listProfiles(): Promise<ProxyConfigProfile[]> {
  // Keep TUN lifecycle independent from config.ts so config mutations can call
  // back into runtime reconciliation without creating a module cycle.
  return invoke('proxy_config_list');
}

async function resolveTunPolicy(): Promise<TunPolicy> {
  const [appConfig, profiles] = await Promise.all([getAppConfig(), listProfiles()]);
  const profile = profiles.find((item) => item.active);
  return {
    appConfig,
    profile,
    ...profileTunPolicy(profile?.content),
  };
}

function enrichTunStatus(status: GuiTunStatus, policy: TunPolicy): GuiManagedTunStatus {
  let configSource: TunConfigSource | undefined;
  let configSourceName: string | undefined;

  if (policy.profileManaged) {
    configSource = 'profile';
    configSourceName = policy.profile?.name;
  } else if (policy.appConfig.tun.enabled === true) {
    configSource = 'app';
    configSourceName = 'ZNet-Sink';
  } else if (status.enabled) {
    // Compatibility/direct-control state that is not represented by either
    // current persistent source. Keep it visible without claiming ownership.
    configSource = 'runtime';
  }

  const desiredEnabled = policy.profileManaged
    ? policy.profileDesiredEnabled
    : policy.appConfig.tun.enabled === true
      || (policy.appConfig.tun.enabled === undefined && status.enabled);

  return {
    ...status,
    desiredEnabled,
    configSource,
    configSourceName,
  };
}

async function rawTunStatus(): Promise<GuiTunStatus> {
  return invoke('gui_tun_status');
}

async function ensureCoreReady(): Promise<void> {
  const current = await getGuiConnectionStatus().catch(() => null);
  if (!current?.coreAvailable) {
    await startCoreProcess();
  }

  const deadline = Date.now() + CORE_READY_TIMEOUT_MS;
  let lastError: unknown = null;
  while (Date.now() < deadline) {
    try {
      const health = await getGuiCoreHealth();
      if (health.healthy) return;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, CORE_READY_INTERVAL_MS));
  }

  const message = (lastError as { message?: string } | null)?.message
    ?? 'Zero readiness check timed out';
  throw new Error(message);
}

function profileManagedError(policy: TunPolicy, action: 'enable' | 'disable'): { code: string; message: string } {
  const name = policy.profile?.name ? `“${policy.profile.name}”` : '当前配置';
  const state = policy.profileDesiredEnabled ? '启用' : '关闭';
  return {
    code: 'tun_managed_by_profile',
    message: `${name} 已显式定义 runtime.tun（期望${state}），该配置优先于 ZNet-Sink 的本地 TUN 设置。请编辑或切换该配置后再${action === 'enable' ? '开启' : '关闭'} TUN。`,
  };
}

function runtimeOwnershipError(): { code: string; message: string } {
  return {
    code: 'tun_runtime_ownership_mismatch',
    message: 'Zero 当前仍报告配置托管的 TUN，但活动配置未声明 runtime.tun。请重新应用当前配置或重启内核后再操作。',
  };
}

function validateAppDnsHijackPrecondition(policy: TunPolicy): void {
  if (!policy.appConfig.tun.dnsHijack) return;
  if (!policy.appConfig.dns.enabled || !policy.appConfig.dns.config) {
    throw {
      code: 'tun_dns_hijack_requires_dns',
      message: '开启 TUN DNS 劫持前，请先在 DNS 设置中启用并保存 Real DNS 或 Fake-IP。',
    };
  }
}

export async function getGuiTunStatus(): Promise<GuiManagedTunStatus> {
  const status = await rawTunStatus();
  try {
    return enrichTunStatus(status, await resolveTunPolicy());
  } catch {
    return {
      ...status,
      desiredEnabled: status.enabled,
      configSource: status.enabled ? 'runtime' : undefined,
    };
  }
}

/**
 * Stop an app-owned command TUN before activating a profile that explicitly
 * owns runtime.tun. Core intentionally rejects configured TUN activation while
 * a command-managed TUN is still running, so the client owns this handoff.
 *
 * The persisted AppConfig desired state is not changed. `stoppedAppRuntime`
 * records the exact pre-switch runtime so a failed profile transition can put
 * it back even for legacy configs that do not yet have an explicit desired bit.
 */
export async function prepareGuiTunForProfileSwitch(content: unknown): Promise<TunProfileTransition> {
  const target = profileTunPolicy(content);
  if (!target.profileManaged) return { stoppedAppRuntime: false };

  const current = await rawTunStatus().catch(() => null);
  if (!current?.enabled || current.managedByConfig) {
    return { stoppedAppRuntime: false };
  }

  await invoke('gui_tun_disable');
  return { stoppedAppRuntime: true };
}

/** Restore the exact app-owned runtime stopped only for a failed profile handoff. */
export async function restoreGuiTunAfterFailedProfileSwitch(
  transition: TunProfileTransition,
): Promise<void> {
  if (!transition.stoppedAppRuntime) return;

  const policy = await resolveTunPolicy();
  if (policy.profileManaged) return;

  const current = await rawTunStatus();
  if (current.enabled) return;
  validateAppDnsHijackPrecondition(policy);
  await ensureCoreReady();
  await invoke('gui_tun_enable');
}

/**
 * Reconcile the current Core instance with the persisted local desired state.
 * Profile/static runtime.tun always wins; otherwise the local preference is
 * replayed through tun.start/tun.stop and never through config.apply.
 *
 * `enabled: undefined` is a migration state, not an implicit OFF. Until the
 * user explicitly toggles TUN, preserve whatever command-managed state is
 * already observed instead of mutating it during an unrelated config refresh.
 */
export async function reconcileGuiTunRuntime(): Promise<GuiManagedTunStatus> {
  const policy = await resolveTunPolicy();
  let current = await rawTunStatus();

  if (policy.profileManaged) {
    return enrichTunStatus(current, policy);
  }

  if (current.enabled && current.managedByConfig) {
    throw runtimeOwnershipError();
  }

  const desired = policy.appConfig.tun.enabled;
  if (desired === undefined) {
    return enrichTunStatus(current, policy);
  }

  if (desired && !current.enabled) {
    validateAppDnsHijackPrecondition(policy);
    await ensureCoreReady();
    current = await invoke('gui_tun_enable');
  } else if (!desired && current.enabled) {
    current = await invoke('gui_tun_disable');
  }

  return enrichTunStatus(current, policy);
}

export async function enableGuiTun(): Promise<GuiManagedTunStatus> {
  await ensureCoreReady();
  const policy = await resolveTunPolicy();
  const current = await rawTunStatus();

  if (policy.profileManaged) {
    if (!policy.profileDesiredEnabled) {
      throw profileManagedError(policy, 'enable');
    }
    const enriched = enrichTunStatus(current, policy);
    if (!current.enabled) {
      const name = policy.profile?.name ? `“${policy.profile.name}”` : '当前配置';
      throw {
        code: 'tun_profile_runtime_inactive',
        message: `${name} 已要求启用 TUN，但 Zero 当前未运行该 TUN。请检查内核运行状态或配置错误。`,
      };
    }
    return enriched;
  }

  validateAppDnsHijackPrecondition(policy);
  if (current.enabled && current.managedByConfig) {
    throw runtimeOwnershipError();
  }

  const previousDesired = policy.appConfig.tun.enabled === true;
  await updateAppConfig({ tun: { enabled: true } });

  try {
    if (!current.enabled) {
      await invoke('gui_tun_enable');
    }
    return getGuiTunStatus();
  } catch (error) {
    // A failed explicit enable should not leave a new persisted ON intent that
    // will be replayed on every subsequent Core generation. AppConfig's patch
    // model cannot restore legacy undefined, so failure falls back to explicit
    // OFF, which is safer than an unintended future auto-start.
    await updateAppConfig({ tun: { enabled: previousDesired } }).catch(() => undefined);
    throw error;
  }
}

export async function disableGuiTun(): Promise<GuiManagedTunStatus> {
  const policy = await resolveTunPolicy();
  const current = await rawTunStatus();

  if (policy.profileManaged) {
    throw profileManagedError(policy, 'disable');
  }
  if (current.enabled && current.managedByConfig) {
    throw runtimeOwnershipError();
  }

  if (current.enabled) {
    await invoke('gui_tun_disable');
  }
  await updateAppConfig({ tun: { enabled: false } });
  return getGuiTunStatus();
}
