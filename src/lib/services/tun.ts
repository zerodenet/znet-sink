import { invoke } from '@tauri-apps/api/core';
import {
  getAppConfig,
  getGuiConnectionStatus,
  getGuiCoreHealth,
  startCoreProcess,
  updateAppConfig,
} from './core';
import { listProxyConfigs } from './config';
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
  // ZNet-Sink must not silently replace invalid source intent with defaults.
  return {
    profileManaged: true,
    profileDesiredEnabled: runtime.tun !== null,
  };
}

async function resolveTunPolicy(): Promise<TunPolicy> {
  const [appConfig, profiles] = await Promise.all([getAppConfig(), listProxyConfigs()]);
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
    message: `${name} 已显式定义 runtime.tun（期望${state}），该配置优先于 ZNet-Sink 的本地 TUN 缺省设置。请编辑或切换该配置后再${action === 'enable' ? '开启' : '关闭'} TUN。`,
  };
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

  // Migrate a legacy command-managed session before making the persistent
  // effective config authoritative. The direct command remains compatibility
  // cleanup only; all new GUI starts use runtime.tun via app_config_update.
  if (current.enabled && !current.managedByConfig) {
    await invoke('gui_tun_disable');
  }

  await updateAppConfig({ tun: { enabled: true } });
  return getGuiTunStatus();
}

export async function disableGuiTun(): Promise<GuiManagedTunStatus> {
  const policy = await resolveTunPolicy();
  const current = await rawTunStatus();

  if (policy.profileManaged) {
    throw profileManagedError(policy, 'disable');
  }

  // Clean up a legacy/direct command-managed session first. Persisting false
  // alone cannot own or stop an independently command-managed Core TUN.
  if (current.enabled && !current.managedByConfig) {
    await invoke('gui_tun_disable');
  }

  await updateAppConfig({ tun: { enabled: false } });
  return getGuiTunStatus();
}
