/** Local evidence only. These DTOs must never be used as recovery/control permits. */
export type InspectionKind = 'workspace' | 'packet';
export const workspaceReasons = {
  'metadata-readable': '恢复元资料可读；候选正文和运行结果未在此校验。',
  'not-initialized': '尚无本地数据库；检查没有创建文件。',
  'io-unavailable': '本地存储读取失败；原材料保留。',
  'permission-denied': '系统拒绝读取本地存储。',
  'storage-busy': '本地存储正在被其他操作占用。',
  'wal-active': '数据库仍有写入协作材料；请稍后重新检查，不会忽略待合入数据或修改原文件。',
  'inspection-budget': '检查达到时间、容量或指令预算。',
  'invalid-container': '本地数据库容器无法校验。',
  'storage-unavailable': '无法完成本地存储只读检查。',
  'unsafe-file': '存储位置不是可安全读取的普通文件。',
  'changed-during-inspection': '检查期间存储发生变化；结果仅作历史参考。',
  'wrong-application': '数据库应用标识不匹配。',
  'unsupported-schema': '当前存储版本不支持此只读检查。',
  'untrusted-structure': '恢复表结构不匹配。',
  'invalid-metadata': '恢复元资料无法校验。',
} as const;
export const packetReasons = {
  ready: '关闭账本元资料可读；这不代表原会话已经结束。',
  'not-initialized': '尚无关闭账本；检查没有初始化文件。',
  'initialization-pending': '关闭账本初始化尚未确认。',
  'missing-initialized': '已初始化的关闭账本缺失。',
  'invalid-json': '关闭账本格式无法解析。',
  'untrusted-structure': '关闭账本结构无法校验。',
  'unsupported-schema': '此关闭账本版本不支持当前检查。',
  oversized: '关闭账本超过检查容量预算。',
  'unsafe-file': '账本位置不是可安全读取的普通文件。',
  'invalid-marker': '关闭账本初始化标记无法校验。',
  'coordination-missing': '关闭账本缺少必要的协调材料。',
  'lock-busy': '关闭账本正在被其他操作占用。',
  'lock-unsupported': '当前平台不支持所需的文件协调。',
  'permission-denied': '系统拒绝读取关闭账本。',
  'changed-during-inspection': '检查期间账本发生变化；结果仅作历史参考。',
  'publication-unconfirmed': '关闭账本的持久发布尚未确认。',
  'entropy-unavailable': '宿主身份无法确认；检查不授予控制权限。',
  'capacity-reached': '关闭账本记录预算已满。',
  'state-unavailable': '当前宿主账本状态无法读取。',
  'io-unavailable': '关闭账本读取失败。',
} as const;
interface Base {
  hostSessionId: string | null; sampledAtUnixMs: string; readOnly: true; barriersRetained: true;
}
export interface WorkspaceStorageInspection extends Base {
  state: 'metadata-readable' | 'not-initialized' | 'blocked';
  reasonCode: keyof typeof workspaceReasons; summary: string;
  evidence: {databasePresent: boolean|null; databaseBytes: string|null; applicationId: number|null; schemaVersion: number|null; metadataRows: number|null; pendingRows: number|null; candidateBodiesRead: false; snapshotStable: boolean};
}
export interface PacketLedgerInspection extends Base {
  state: 'ready' | 'not-initialized' | 'blocked'; reasonCode: keyof typeof packetReasons; summary: string;
  newCloseBlocked: boolean; inMemoryBarrierCount: number|null; publicationUnconfirmed: boolean;
  evidence: {journalPresent: boolean|null; markerPresent: boolean|null; markerState: 'not-read'|'absent'|'empty'|'initialized'|'untrusted'; lockState:'not-checked'|'not-present'|'acquired'|'busy'|'unsupported'|'unavailable'; journalBytes: string|null; journalSha256:string|null; schemaVersion:number|null; verifiedRecordCount:number|null; snapshotStable:boolean};
}
export type Inspection = WorkspaceStorageInspection | PacketLedgerInspection;
const invalid = () => { throw new Error('invalid-inspection'); };
const object = (v: unknown): Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v) ? v as Record<string, unknown> : invalid();
const bool = (v: unknown): boolean => typeof v === 'boolean' ? v : invalid();
const nullableBool = (v: unknown): boolean|null => v === null ? null : bool(v);
const count = (v: unknown, max = Number.MAX_SAFE_INTEGER): number|null => v === null ? null : typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 && v <= max ? v : invalid();
const decimal = (v: unknown): string => typeof v === 'string' && /^(0|[1-9]\d{0,19})$/.test(v) && BigInt(v) <= 18446744073709551615n ? v : invalid();
const nullableDecimal = (v: unknown): string|null => v === null ? null : decimal(v);
const oneOf = <T extends string>(v: unknown, values: readonly T[]): T => typeof v === 'string' && values.includes(v as T) ? v as T : invalid();
/** Copy only known metadata; never retain host messages, unknown fields or raw records. */
export function decodeInspection(kind: InspectionKind, value: unknown): Inspection {
  const v = object(value), e = object(v.evidence);
  if (v.readOnly !== true || v.barriersRetained !== true) invalid();
  const hostSessionId = v.hostSessionId === null ? null : typeof v.hostSessionId === 'string' && v.hostSessionId.length > 0 && v.hostSessionId.length <= 256 ? v.hostSessionId : invalid();
  const base = {hostSessionId, sampledAtUnixMs: decimal(v.sampledAtUnixMs), readOnly: true as const, barriersRetained: true as const};
  if (kind === 'workspace') {
    const reasonCode = oneOf(v.reasonCode, Object.keys(workspaceReasons) as (keyof typeof workspaceReasons)[]);
    const state = oneOf(v.state, ['metadata-readable','not-initialized','blocked'] as const);
    if ((state === 'metadata-readable') !== (reasonCode === 'metadata-readable') || (state === 'not-initialized') !== (reasonCode === 'not-initialized') || e.candidateBodiesRead !== false) invalid();
    const metadataRows = count(e.metadataRows,64), pendingRows = count(e.pendingRows,64);
    if (metadataRows !== null && pendingRows !== null && pendingRows > metadataRows) invalid();
    return {...base,state,reasonCode,summary:workspaceReasons[reasonCode],evidence:{databasePresent:nullableBool(e.databasePresent),databaseBytes:nullableDecimal(e.databaseBytes),applicationId:e.applicationId === null ? null : typeof e.applicationId === 'number' && Number.isInteger(e.applicationId) && e.applicationId >= -2147483648 && e.applicationId <= 2147483647 ? e.applicationId : invalid(),schemaVersion:count(e.schemaVersion,2147483647),metadataRows,pendingRows,candidateBodiesRead:false,snapshotStable:bool(e.snapshotStable)}};
  }
  const reasonCode = oneOf(v.reasonCode, Object.keys(packetReasons) as (keyof typeof packetReasons)[]);
  const state = oneOf(v.state,['ready','not-initialized','blocked'] as const);
  if ((state === 'ready') !== (reasonCode === 'ready') || (state === 'not-initialized') !== (reasonCode === 'not-initialized')) invalid();
  return {...base,state,reasonCode,summary:packetReasons[reasonCode],newCloseBlocked:bool(v.newCloseBlocked),inMemoryBarrierCount:count(v.inMemoryBarrierCount,128),publicationUnconfirmed:bool(v.publicationUnconfirmed),evidence:{journalPresent:nullableBool(e.journalPresent),markerPresent:nullableBool(e.markerPresent),markerState:oneOf(e.markerState,['not-read','absent','empty','initialized','untrusted'] as const),lockState:oneOf(e.lockState,['not-checked','not-present','acquired','busy','unsupported','unavailable'] as const),journalBytes:nullableDecimal(e.journalBytes),journalSha256:e.journalSha256 === null ? null : typeof e.journalSha256 === 'string' && /^[a-fA-F0-9]{64}$/.test(e.journalSha256) ? e.journalSha256 : invalid(),schemaVersion:count(e.schemaVersion,4294967295),verifiedRecordCount:count(e.verifiedRecordCount,128),snapshotStable:bool(e.snapshotStable)}};
}
export function inspectionTime(value: string): string {
  if (!/^(0|[1-9]\d*)$/.test(value) || BigInt(value) > 8640000000000000n) return '时间未提供';
  return new Date(Number(value)).toLocaleString('zh-CN');
}
export function scopeFingerprint(value: unknown): string|null {
  const v = object(value);
  if (typeof v.scopeGeneration !== 'number' || !Number.isSafeInteger(v.scopeGeneration) || v.scopeGeneration <= 0) return null;
  return JSON.stringify([v.scope, v.scopeGeneration]);
}
