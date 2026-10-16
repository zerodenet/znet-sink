// Native Packet conversations are independent of TCP/UDP Flow records.
export const PACKET_PAGE_LIMIT = 500;
export const PACKET_CLOSE_BUDGET = 128;
export interface PacketEndpoint { endpointId: string; generation: string | null }
export interface PacketPhysicalTarget { routeId: string; coreInstanceId: string; configRevision: string }
export interface PacketCloseRecord {
  target: PacketPhysicalTarget; operationId: string; originProcessId: string; originProfileId: string | null;
  originClientScopeGeneration: string; reservedAtUnixMs: string; recovered: boolean;
}
export interface PacketLedgerRecovery { state: 'ready' | 'blocked'; reason: string | null; recoveredCount: number | null }
export interface PacketRoute extends PacketPhysicalTarget {
  closeOperationId?: string | null;
  inboundTag: string; outboundTag: string; endpoints: PacketEndpoint[];
  source: string; destination: string; ipProtocol: number; translated: boolean;
  startedAtUnixMs: string; state: 'active' | 'closing' | 'closed';
}
export interface PacketPageIdentity {
  profileId: string | null; coreInstanceId: string; configRevision: string; clientScopeGeneration: number; hostSessionId: string;
}
export interface PacketPageQuery { offset: number; limit: number; expected: PacketPageIdentity | null }
export interface PacketSnapshot {
  offset: number; limit: number; clientScopeGeneration: number; hostSessionId: string;
  closeRecords: PacketCloseRecord[]; closeLedgerRecovery: PacketLedgerRecovery;
  blockedTargets: PacketPhysicalTarget[]; closeLedgerFull: boolean;
  supported: boolean; canClose: boolean; reason: string | null; closeReason: string | null;
  profileId: string | null; coreInstanceId: string | null; configRevision: string | null;
  observedAtUnixMs: string | null; routes: PacketRoute[]; total: number | null; nextOffset: number | null;
}
export interface PacketTarget extends PacketPhysicalTarget { profileId: string | null; clientScopeGeneration: number; hostSessionId: string; closeOperationId?: string | null }
export interface PacketCloseResult { status: 'confirmed' | 'unconfirmed'; message: string; route: PacketRoute | null; operationId?: string | null }
export interface PacketGateway {
  snapshot(query?: PacketPageQuery): Promise<PacketSnapshot>;
  detail(target: PacketTarget): Promise<PacketRoute>;
  close(target: PacketTarget): Promise<PacketCloseResult>;
}
export function physicalKey(target: PacketPhysicalTarget): string {
  return JSON.stringify([target.coreInstanceId, target.routeId, target.configRevision]);
}
export const safeGeneration = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value > 0;
const u32 = (value: unknown): value is number => typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= 0xffffffff;
const u64 = (value: unknown): value is string => typeof value === 'string' && /^(0|[1-9]\d{0,19})$/.test(value) && BigInt(value) <= 18446744073709551615n;
const opaque = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0 && value.length <= 1024;
const identity = (target: PacketPhysicalTarget): boolean => typeof target.routeId === 'string' && target.routeId.trim().length > 0 && target.routeId.length <= 1024 && typeof target.coreInstanceId === 'string' && target.coreInstanceId.trim().length > 0 && target.coreInstanceId.length <= 1024 && u64(target.configRevision);
export function validatePacketPage(snapshot: PacketSnapshot, query: PacketPageQuery): void {
  if (!opaque(snapshot.hostSessionId) || !Array.isArray(snapshot.closeRecords) || snapshot.closeRecords.length > PACKET_CLOSE_BUDGET
    || snapshot.closeRecords.some(record => !identity(record.target) || !opaque(record.operationId) || !opaque(record.originProcessId) || (record.originProfileId !== null && !opaque(record.originProfileId)) || !u64(record.originClientScopeGeneration) || !u64(record.reservedAtUnixMs) || typeof record.recovered !== 'boolean')
    || new Set(snapshot.closeRecords.map(record => physicalKey(record.target))).size !== snapshot.closeRecords.length
    || !snapshot.closeLedgerRecovery || !['ready','blocked'].includes(snapshot.closeLedgerRecovery.state) || (snapshot.closeLedgerRecovery.recoveredCount !== null && (!u32(snapshot.closeLedgerRecovery.recoveredCount) || snapshot.closeLedgerRecovery.recoveredCount > PACKET_CLOSE_BUDGET)) || (snapshot.closeLedgerRecovery.state === 'ready' && snapshot.closeLedgerRecovery.recoveredCount === null)) throw new Error('Packet 关闭账本或宿主会话身份缺少兼容的有界恢复事实');
  if (!safeGeneration(snapshot.clientScopeGeneration) || snapshot.offset !== query.offset || snapshot.limit !== query.limit || !u32(snapshot.offset) || !Number.isInteger(snapshot.limit) || snapshot.limit < 1 || snapshot.limit > PACKET_PAGE_LIMIT || !Array.isArray(snapshot.routes) || snapshot.routes.length > snapshot.limit || !Array.isArray(snapshot.blockedTargets) || snapshot.blockedTargets.length > PACKET_CLOSE_BUDGET || snapshot.blockedTargets.some(target => !identity(target))) throw new Error('Packet 分页响应缺少兼容的身份或有界范围');
  if (query.expected && snapshot.hostSessionId !== query.expected.hostSessionId) throw new Error('宿主会话已变化，请返回第一页重新检查');
  if (!snapshot.supported) return;
  const end = snapshot.offset + snapshot.routes.length;
  if (!u32(snapshot.total) || (snapshot.nextOffset !== null && !u32(snapshot.nextOffset)) || snapshot.nextOffset !== (end < snapshot.total ? end : null) || (end < snapshot.total && !snapshot.routes.length) || snapshot.routes.length > snapshot.total || (snapshot.routes.length > 0 && (snapshot.offset >= snapshot.total || end > snapshot.total)) || !snapshot.coreInstanceId || !u64(snapshot.configRevision) || !u64(snapshot.observedAtUnixMs)) throw new Error('Packet 分页位置或计数无效');
  const ids = new Set<string>();
  for (const route of snapshot.routes) {
    if (!identity(route) || route.coreInstanceId !== snapshot.coreInstanceId || ids.has(route.routeId) || !u64(route.startedAtUnixMs) || !['active','closing','closed'].includes(route.state) || route.endpoints.some(endpoint => endpoint.generation !== null && !u64(endpoint.generation))) throw new Error('Packet 会话身份或无损计数不兼容');
    ids.add(route.routeId);
  }
  const expected = query.expected;
  if (expected && (snapshot.hostSessionId !== expected.hostSessionId || snapshot.profileId !== expected.profileId || snapshot.coreInstanceId !== expected.coreInstanceId || snapshot.configRevision !== expected.configRevision || snapshot.clientScopeGeneration !== expected.clientScopeGeneration)) throw new Error('分页所属宿主会话、配置、实例或代次已变化；请返回第一页重新检查');
}
export function pageIdentity(snapshot: PacketSnapshot): PacketPageIdentity | null {
  return snapshot.supported && snapshot.coreInstanceId && snapshot.configRevision ? { profileId: snapshot.profileId, coreInstanceId: snapshot.coreInstanceId, configRevision: snapshot.configRevision, clientScopeGeneration: snapshot.clientScopeGeneration, hostSessionId: snapshot.hostSessionId } : null;
}
export function targetOf(snapshot: PacketSnapshot, route: PacketPhysicalTarget): PacketTarget {
  return { profileId: snapshot.profileId, clientScopeGeneration: snapshot.clientScopeGeneration, hostSessionId: snapshot.hostSessionId, routeId: route.routeId, coreInstanceId: route.coreInstanceId, configRevision: route.configRevision };
}
export function closeReason(snapshot: PacketSnapshot | null, route: PacketRoute, stale: boolean): string | null {
  if (stale || !snapshot || !safeGeneration(snapshot.clientScopeGeneration)) return '观测状态未确认，请重新检查';
  if (!opaque(snapshot.hostSessionId)) return '宿主会话身份未确认，请返回第一页重新检查';
  if (snapshot.closeLedgerRecovery.state !== 'ready') return snapshot.closeLedgerRecovery.reason ?? '关闭账本恢复未确认；新增关闭暂不可用';
  if (!snapshot.supported) return snapshot.reason ?? '当前内核未声明 Packet 路径管理能力';
  if (!snapshot.canClose) return snapshot.closeReason ?? snapshot.reason ?? '当前实例未允许关闭 Packet 会话';
  if (snapshot.coreInstanceId !== route.coreInstanceId || snapshot.configRevision !== route.configRevision) return '会话创建后的配置已变化，请刷新并重新核对';
  if (route.state !== 'active') return route.state === 'closing' ? '会话正在关闭，请重新检查' : '会话已结束';
  return null;
}
export function packetTime(value: string | null): string {
  if (value === null || !u64(value)) return '—';
  const n = BigInt(value);
  return n > 8640000000000000n ? value : new Date(Number(n)).toLocaleString();
}
