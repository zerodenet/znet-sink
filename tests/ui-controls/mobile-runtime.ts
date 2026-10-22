import type { MobilePlatformSnapshot } from '../../src/lib/features/mobile/runtime';
export async function getMobilePlatformSnapshot(): Promise<MobilePlatformSnapshot> {
  return Object.assign({ platform: 'android', mobile: true, minimumAndroidApi: 24, runtimeOwner: 'android-vpn-service', coreDistribution: 'bundled-with-application', adaptationState: 'not-adapted', vpnSupported: false, reason: '移动数据面尚未适配，不能确认 VPN 接管。', kernelState: 'unavailable', vpnState: 'unknown', observationState: 'unsupported', coreInstanceId: null, profileId: null, configRevision: null }, {
    bundledKernelAvailable: false,
    unsupportedCapabilities: [{capabilityId:'vpn.forwarding',status:'unsupported',limitationKind:'not-adapted',reasonCode:'mobile_tunnel_and_egress_not_adapted',reason:'VPN 暂不支持：系统隧道接入及隧道外网络保护尚未适配，不能确认接管。',dependencyIds:['CORE-01','CORE-02','CLIENT-01']}],
  }) as MobilePlatformSnapshot;
}
export async function getMobileSubscriptionScheduleSnapshot() {
  return {foreground:true,checking:false,foregroundReconciliation:'native-lifecycle',backgroundScheduling:'not-adapted',storageError:false,generation:'1',lastPassStartedAtUnixMs:null,lastPassFinishedAtUnixMs:null,deferred:0};
}
export type { MobilePlatformSnapshot };
