// Native Packet sessions are independent of TCP/UDP Flow records.
export interface PacketEndpoint { endpointId: string; generation: string | null }
export interface PacketRoute {
  routeId: string; coreInstanceId: string; configRevision: string;
  inboundTag: string; outboundTag: string; endpoints: PacketEndpoint[];
  source: string; destination: string; ipProtocol: number; translated: boolean;
  startedAtUnixMs: string; state: 'active' | 'closing' | 'closed';
}
export interface PacketSnapshot {
  supported: boolean; canClose: boolean; reason: string | null; closeReason: string | null;
  profileId: string | null; coreInstanceId: string | null; configRevision: string | null;
  observedAtUnixMs: string | null; routes: PacketRoute[]; total: number | null; nextOffset: number | null;
}
export interface PacketTarget { profileId: string | null; routeId: string; coreInstanceId: string; configRevision: string }
export interface PacketCloseResult { status: 'confirmed' | 'unconfirmed'; message: string; route: PacketRoute | null }
export interface PacketGateway {
  snapshot(): Promise<PacketSnapshot>;
  detail(target: PacketTarget): Promise<PacketRoute>;
  close(target: PacketTarget): Promise<PacketCloseResult>;
}
export function targetOf(snapshot: PacketSnapshot, route: PacketRoute): PacketTarget {
  return { profileId: snapshot.profileId, routeId: route.routeId, coreInstanceId: route.coreInstanceId, configRevision: route.configRevision };
}
export function closeReason(snapshot: PacketSnapshot | null, route: PacketRoute, stale: boolean): string | null {
  if (stale || !snapshot) return '观测状态未确认，请重新检查';
  if (!snapshot.supported) return snapshot.reason ?? '当前内核未声明 Packet 路径管理能力';
  if (!snapshot.canClose) return snapshot.closeReason ?? snapshot.reason ?? '当前实例未允许关闭 Packet 会话';
  if (snapshot.coreInstanceId !== route.coreInstanceId || snapshot.configRevision !== route.configRevision) return '会话创建后的配置已变化，请刷新并重新核对';
  if (route.state !== 'active') return route.state === 'closing' ? '会话正在关闭，请重新检查' : '会话已结束';
  return null;
}
export function packetTime(value: string | null): string {
  if (value === null || !/^\d+$/.test(value)) return '—';
  const n = BigInt(value);
  // Date has a smaller valid range than u64; keep unknown clocks lossless.
  if (n > 8640000000000000n) return value;
  return new Date(Number(n)).toLocaleString();
}
