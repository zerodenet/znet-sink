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
}

export function getMobilePlatformSnapshot(): Promise<MobilePlatformSnapshot> {
  return invoke('gui_mobile_platform_snapshot');
}
