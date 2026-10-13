import type { MobilePlatformSnapshot } from '../../src/lib/features/mobile/runtime';
export async function getMobilePlatformSnapshot(): Promise<MobilePlatformSnapshot> {
  return Object.assign({ platform: 'android', mobile: true, minimumAndroidApi: 24, runtimeOwner: 'android-vpn-service', coreDistribution: 'bundled-with-application', adaptationState: 'not-adapted', vpnSupported: false, reason: '移动数据面尚未适配，不能确认 VPN 接管。', kernelState: 'unavailable', vpnState: 'unknown', observationState: 'unsupported', coreInstanceId: null, profileId: null, configRevision: null }, { bundledKernelAvailable: false }) as MobilePlatformSnapshot;
}
export type { MobilePlatformSnapshot };
