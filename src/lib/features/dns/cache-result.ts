import type { DnsCacheEntry, DnsCacheResult } from '$lib/types/diagnostics';
const object = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
const decimal = (v: unknown): v is string => typeof v === 'string' && /^(0|[1-9][0-9]{0,19})$/.test(v) && BigInt(v) <= 18446744073709551615n;
const strings = (v: unknown): v is string[] => Array.isArray(v) && v.length <= 500 && v.every(s => typeof s === 'string' && s.length <= 128);
const domain = (v: unknown): v is string => typeof v === 'string' && v.length > 0 && v.length <= 4096;
export const cacheCompatibilityError = 'DNS 缓存结果不完整或格式未适配；没有推断为空缓存或零计数';
export function parseDnsCache(value: unknown): DnsCacheResult {
  if (!object(value) || typeof value.enabled !== 'boolean') throw new Error(cacheCompatibilityError);
  if ('domain' in value) {
    if (!domain(value.domain) || typeof value.hit !== 'boolean' || !strings(value.addresses)
      || !(value.ttl_seconds === null || decimal(value.ttl_seconds))
      || (value.hit && value.ttl_seconds === null)
      || (!value.hit && (value.addresses.length !== 0 || value.ttl_seconds !== null))) throw new Error(cacheCompatibilityError);
    return { enabled: value.enabled, domain: value.domain, hit: value.hit, addresses: [...value.addresses], ttl_seconds: value.ttl_seconds };
  }
  if (!Array.isArray(value.entries) || value.entries.length > 500 || !Number.isSafeInteger(value.count)
    || value.count !== value.entries.length) throw new Error(cacheCompatibilityError);
  const entries: DnsCacheEntry[] = value.entries.map(entry => {
    if (!object(entry) || !domain(entry.domain) || !strings(entry.addresses) || !decimal(entry.ttl_seconds)) throw new Error(cacheCompatibilityError);
    return { domain: entry.domain, addresses: [...entry.addresses], ttl_seconds: entry.ttl_seconds };
  });
  return { enabled: value.enabled, entries, count: value.count as number };
}
