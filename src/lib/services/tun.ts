import { invoke } from '@tauri-apps/api/core';
import {
  getAppConfig,
  getGuiConnectionStatus,
  getGuiCoreHealth,
  getGuiZeroCapabilities,
  startCoreProcess,
  updateAppConfig,
} from './core';
import type { AppConfig } from '$lib/types/app-config';
import type { GuiTunStatus } from '$lib/types/gui-api';
import type { GuiManagedTunStatus, TunConfigSource } from '$lib/types/tun';
import {
  projectClientKernelFeatures,
  type ClientKernelFeatures,
} from '$lib/services/kernel-capabilities';

const CORE_READY_TIMEOUT_MS = 8_000;
const CORE_READY_INTERVAL_MS = 100;
const TUN_STATE_RECONCILE_TIMEOUT_MS = 15_000;
const TUN_STATE_RECONCILE_INTERVAL_MS = 250;

interface TunPolicy {
  appConfig: AppConfig;

}

export interface TunDnsHijackReadiness {
  state: 'ready' | 'blocked' | 'unknown';
  code?: string;
  message: string;
  features?: ClientKernelFeatures;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}

async function resolveTunPolicy(): Promise<TunPolicy> {
  return {appConfig: await getAppConfig()};
}

function enrichTunStatus(status: GuiTunStatus, policy: TunPolicy): GuiManagedTunStatus {
  let configSource: TunConfigSource | undefined;
  let configSourceName: string | undefined;

  if (policy.appConfig.tun.enabled === true) {
    configSource = 'app';
    configSourceName = 'ZNet-Sink';
  } else if (status.enabled) {
    // Compatibility/direct-control state that is not represented by either
    // current persistent source. Keep it visible without claiming ownership.
    configSource = 'runtime';
  }

  const desiredEnabled = policy.appConfig.tun.enabled === true
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

function isTransientCoreIpcError(error: unknown): boolean {
  const code = isObject(error) && typeof error.code === 'string' ? error.code : '';
  if (code === 'timeout' || code === 'connection_closed' || code === 'core_unavailable') {
    return true;
  }
  const message = isObject(error) && typeof error.message === 'string'
    ? error.message
    : String(error ?? '');
  return message.includes('core IPC request timed out')
    || message.includes('core IPC connection closed');
}

async function waitForTunStateAfterTransientIpcError(
  expectedEnabled: boolean,
  error: unknown,
): Promise<GuiManagedTunStatus | null> {
  const deadline = Date.now() + (
    isTransientCoreIpcError(error) ? TUN_STATE_RECONCILE_TIMEOUT_MS : 0
  );

  do {
    const status = await getGuiTunStatus().catch(() => null);
    if (status?.enabled === expectedEnabled && (expectedEnabled ? status.healthy : !status.lastError)) return status;
    if (Date.now() >= deadline) return null;
    await new Promise((resolve) => setTimeout(resolve, TUN_STATE_RECONCILE_INTERVAL_MS));
  } while (true);
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

function runtimeOwnershipError(): { code: string; message: string } {
  return {
    code: 'tun_runtime_ownership_mismatch',
    message: 'Zero 当前仍运行旧配置托管的 TUN，尚未切换为客户端接管。请重新应用当前配置或重启内核后再操作。',
  };
}

export function evaluateTunDnsHijackReadiness(
  dns: AppConfig['dns'],
  features?: ClientKernelFeatures,
): TunDnsHijackReadiness {
  if (!dns.enabled || !dns.config || Object.keys(dns.config.servers).length === 0) {
    return {
      state: 'blocked',
      code: 'tun_dns_hijack_requires_dns',
      message: '开启 TUN DNS 劫持前，请先在 DNS 设置中启用并保存 Real DNS 或 Fake-IP。',
      features,
    };
  }
  if (features?.tunDnsHijack.state === 'unsupported') {
    return {
      state: 'blocked',
      code: 'feature_disabled',
      message: '当前内核未声明 TUN DNS 劫持能力，请升级内核或关闭 DNS 劫持。',
      features,
    };
  }
  const usesSystemDns = Object.values(dns.config.servers).some((server) => server.type === 'system');
  if (usesSystemDns && features?.tunDnsSystemAuto.state === 'unsupported') {
    return {
      state: 'blocked',
      code: 'tun_dns_system_auto_unsupported',
      message: '当前内核不能为 TUN 自动排除 system DNS。请升级内核、关闭 DNS 劫持或改用显式网络 DNS。',
      features,
    };
  }
  if (!features || features.tunDnsHijack.state === 'unknown'
    || (usesSystemDns && features.tunDnsSystemAuto.state === 'unknown')) {
    return {
      state: 'unknown',
      message: '当前内核的 TUN DNS 能力未知，启动时将继续由内核进行最终校验。',
      features,
    };
  }
  return {
    state: 'ready',
    message: usesSystemDns
      ? 'system DNS 将由内核自动发现真实上游并排除 TUN 自捕获。'
      : '当前 DNS 后端可用于 TUN DNS 劫持。',
    features,
  };
}

export async function inspectTunDnsHijackReadiness(
  dns?: AppConfig['dns'],
): Promise<TunDnsHijackReadiness> {
  const [appConfig, capabilities] = await Promise.all([
    dns ? Promise.resolve(null) : getAppConfig(),
    getGuiZeroCapabilities().catch(() => null),
  ]);
  return evaluateTunDnsHijackReadiness(
    dns ?? appConfig!.dns,
    projectClientKernelFeatures(capabilities),
  );
}

async function validateAppDnsHijackPrecondition(policy: TunPolicy): Promise<void> {
  if (!policy.appConfig.tun.dnsHijack) return;
  const readiness = await inspectTunDnsHijackReadiness(policy.appConfig.dns);
  if (readiness.state === 'blocked') {
    throw {
      code: readiness.code,
      message: readiness.message,
    };
  }
}

export async function getGuiTunStatus(): Promise<GuiManagedTunStatus> {
  const status = await rawTunStatus();
  return enrichTunStatus(status, await resolveTunPolicy());
}

/**
 * Reconcile the current Core instance with the persisted local desired state.
 * Profile/static runtime.tun is removed by composition; the local preference is
 * replayed through tun.start/tun.stop and never through config.apply.
 *
 * `enabled: undefined` is a migration state, not an implicit OFF. Until the
 * user explicitly toggles TUN, preserve whatever command-managed state is
 * already observed instead of mutating it during an unrelated config refresh.
 */
export async function reconcileGuiTunRuntime(): Promise<GuiManagedTunStatus> {
  const policy = await resolveTunPolicy();
  let current = await rawTunStatus();

  if (current.enabled && current.managedByConfig) {
    throw runtimeOwnershipError();
  }

  const desired = policy.appConfig.tun.enabled;
  if (desired === undefined) {
    return enrichTunStatus(current, policy);
  }

  if (desired && !current.enabled) {
    await validateAppDnsHijackPrecondition(policy);
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

  await validateAppDnsHijackPrecondition(policy);
  if (current.enabled && current.managedByConfig) {
    throw runtimeOwnershipError();
  }

  const previousDesired = policy.appConfig.tun.enabled === true;
  await updateAppConfig({ tun: { enabled: true } });

  try {
    if (!current.enabled) {
      await invoke('gui_tun_enable');
    }
    const confirmed = await getGuiTunStatus();
    if (!confirmed.enabled || !confirmed.healthy) throw { code: 'tun_start_unconfirmed', message: 'Zero 未确认 TUN 已健康启动' };
    return confirmed;
  } catch (error) {
    // tun.start can finish its platform route work after the IPC response
    // deadline. Reconcile the authoritative runtime state before reporting a
    // false failure or rolling the persisted desired state back to OFF.
    const reconciled = await waitForTunStateAfterTransientIpcError(true, error);
    if (reconciled) return reconciled;

    // A failed explicit enable should not leave a new persisted ON intent that
    // will be replayed on every subsequent Core generation. AppConfig's patch
    // model cannot restore legacy undefined, so failure falls back to explicit
    // OFF, which is safer than an unintended future auto-start.
    await updateAppConfig({ tun: { enabled: previousDesired } }).catch(() => undefined);
    throw error;
  }
}

export async function recoverGuiTun(): Promise<GuiManagedTunStatus> {
  await invoke('gui_tun_recover');
  const confirmed = await getGuiTunStatus();
  if (!confirmed.enabled || !confirmed.healthy || confirmed.lastError) {
    throw { code: 'tun_recovery_unconfirmed', message: confirmed.lastError || '网络重检已完成，但 TUN 尚未恢复；自动恢复将继续尝试' };
  }
  return confirmed;
}

export async function disableGuiTun(): Promise<GuiManagedTunStatus> {
  const policy = await resolveTunPolicy();

  // Cancelling a saved ON intent must work even while the runtime is
  // unreachable. Failure to confirm the stop remains an error, not an OFF
  // snapshot, but the next Core generation must not replay the old intent.
  const current = await rawTunStatus().catch(() => null);
  if (current?.enabled && current.managedByConfig) {
    throw runtimeOwnershipError();
  }
  await updateAppConfig({ tun: { enabled: false } });

  if (!current || current.enabled) {
    try {
      await invoke('gui_tun_disable');
    } catch (error) {
      // As with enable, a late response is not a failed operation when Zero's
      // subsequent status already confirms that TUN is stopped.
      const reconciled = await waitForTunStateAfterTransientIpcError(false, error);
      if (!reconciled) throw error;
    }
  }
  const confirmed = await getGuiTunStatus();
  if (confirmed.enabled || confirmed.lastError) throw { code: 'tun_stop_unconfirmed', message: confirmed.lastError ? `TUN 关闭未完成：${confirmed.lastError}；已取消自动恢复` : 'Zero 未确认 TUN 已关闭；已取消自动恢复' };
  return confirmed;
}
