export type LocalStoreKind = 'logs' | 'history' | 'capture';
export interface LocalStoreStorage {
  kind: LocalStoreKind; state: 'sampled' | 'unavailable'; bytes: string | null; files: number | null;
  byteLimit: string | null; recordLimit: number | null; maxAgeMs: string | null;
  enforcement: 'append-rotation' | 'append-and-read-rotation' | 'segment-rotation';
}
export interface LocalDataSummary { sampledAtUnixMs: string; stores: LocalStoreStorage[] }
const decimal = (value: unknown): value is string => typeof value === 'string' && /^(0|[1-9]\d{0,19})$/.test(value) && BigInt(value) <= 18446744073709551615n;
const count = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
export function decodeLocalData(value: unknown): LocalDataSummary {
  const input = value as Partial<LocalDataSummary> | null;
  if (!input || !decimal(input.sampledAtUnixMs) || !Array.isArray(input.stores) || input.stores.length !== 3) throw Error('invalid-local-storage-summary');
  const kinds = new Set<string>();
  const stores = input.stores.map(row => {
    if (!row || !['logs', 'history', 'capture'].includes(row.kind) || kinds.has(row.kind)
        || !['sampled', 'unavailable'].includes(row.state)
        || (row.state === 'sampled' ? !decimal(row.bytes) || !count(row.files) : row.bytes !== null || row.files !== null)
        || (row.byteLimit !== null && !decimal(row.byteLimit)) || (row.maxAgeMs !== null && !decimal(row.maxAgeMs))
        || (row.recordLimit !== null && !count(row.recordLimit))
        || !['append-rotation', 'append-and-read-rotation', 'segment-rotation'].includes(row.enforcement)) throw Error('invalid-local-storage-summary');
    kinds.add(row.kind);
    // Copy only public facts, never unknown fields, paths or free error text.
    return {kind: row.kind, state: row.state, bytes: row.bytes, files: row.files, byteLimit: row.byteLimit,
      recordLimit: row.recordLimit, maxAgeMs: row.maxAgeMs, enforcement: row.enforcement};
  });
  return {sampledAtUnixMs: input.sampledAtUnixMs, stores};
}
export function storageAge(value: string | null): string {
  if (value === null) return '未设置固定时长';
  const age = BigInt(value), day = 86_400_000n;
  return age % day === 0n ? `${age / day} 天` : `${value} 毫秒`;
}
