import { invoke } from '@tauri-apps/api/core';

export interface MobilePlatformSnapshot {
  platform: 'android' | 'ios' | 'desktop';
  mobile: boolean;
  minimumAndroidApi: number;
  runtimeOwner: string;
  coreDistribution: string;
  bundledKernelAvailable?: boolean;
  adaptationState: string;
  vpnSupported: boolean;
  reason: string | null;
  kernelState: string;
  vpnState: string;
  observationState: string;
  coreInstanceId: string | null;
  profileId: string | null;
  configRevision: string | null;
  unsupportedCapabilities?: MobileUnsupportedCapability[];
}

export interface MobileUnsupportedCapability {
  capabilityId: string;
  status: 'unsupported';
  limitationKind: 'not-adapted' | 'not-supported';
  reasonCode: string;
  reason: string;
  dependencyIds: string[];
}

export function getMobilePlatformSnapshot(): Promise<MobilePlatformSnapshot> {
  return invoke('gui_mobile_platform_snapshot');
}

export interface MobileSubscriptionScheduleSnapshot {
  foreground: boolean;
  checking: boolean;
  foregroundReconciliation: 'native-lifecycle';
  backgroundScheduling: 'not-adapted';
  storageError: boolean;
  generation: string;
  lastPassStartedAtUnixMs: number | null;
  lastPassFinishedAtUnixMs: number | null;
  deferred: number;
}

export async function getMobileSubscriptionScheduleSnapshot(): Promise<MobileSubscriptionScheduleSnapshot> {
  const value = await invoke<MobileSubscriptionScheduleSnapshot>('gui_mobile_subscription_schedule_snapshot');
  if (!value || value.foregroundReconciliation !== 'native-lifecycle' || value.backgroundScheduling !== 'not-adapted'
    || typeof value.storageError !== 'boolean' || typeof value.foreground !== 'boolean' || typeof value.checking !== 'boolean'
    || !/^\d+$/.test(value.generation) || !Number.isSafeInteger(value.deferred) || value.deferred < 0) {
    throw new Error('原生前台调度状态未确认，请重新读取');
  }
  return value;
}
