import type { ManagedSubscriptionSource, SubscriptionProfile } from '$lib/types/domain';

/** One configuration, with zero or more historical update sources. No raw JSON in list IPC. */
export interface ConfigurationEntry {
  id: string;
  profileId: string | null;
  name: string;
  active: boolean;
  hasContent: boolean;
  format: string;
  path: string | null;
  updatedAtUnixMs: number;
  managedSource: ManagedSubscriptionSource | null;
  sources: SubscriptionProfile[];
  missingProfile: boolean;
}
export type ConfigurationKind = 'local' | 'subscription' | 'managed';
export function configurationKind(entry: ConfigurationEntry): ConfigurationKind {
  if (entry.managedSource || entry.sources.some(source => source.managedSource)) return 'managed';
  return entry.sources.length ? 'subscription' : 'local';
}
export function filterConfigurations(entries: ConfigurationEntry[], kind: string, query: string): ConfigurationEntry[] {
  const term = query.trim().toLowerCase();
  return entries.filter(entry => (kind === 'all' || configurationKind(entry) === kind)
    && (!term || [entry.name, entry.path, entry.profileId, ...entry.sources.flatMap(source =>
      [source.name, source.managedSource?.sourceName, source.managedSource?.pluginId])]
      .some(value => value?.toLowerCase().includes(term))));
}
export function pluginLabel(pluginId: string): string {
  const parts = pluginId.split('.').filter(Boolean);
  const part = parts.at(-1) === 'znet-sink' ? parts.at(-2) : parts.at(-1);
  return (part ?? pluginId).split(/[-_]/).map(value => value.charAt(0).toUpperCase() + value.slice(1)).join(' ');
}
export function canonicalSubscriptionFormat(value: string): string {
  const format = value.trim().toLowerCase();
  if (['zero', 'zero-json', 'zero-base64-json', 'base64-json', 'znet-sink', 'znet-sink-base64'].includes(format)) return 'zero';
  if (format.includes('clash') || ['yaml', 'base64-yaml'].includes(format)) return 'clash';
  return format || 'auto';
}
export function subscriptionUsedBytes(source: SubscriptionProfile): number | null {
  if (source.usedBytes != null) return source.usedBytes;
  if (source.uploadBytes == null && source.downloadBytes == null) return null;
  return (source.uploadBytes ?? 0) + (source.downloadBytes ?? 0);
}

export function configurationOptions(entries: ConfigurationEntry[]): { value: string; label: string; disabled: boolean }[] {
  return entries.map(entry => ({ value: entry.id,
    label: `${entry.name}${entry.hasContent ? '' : ' · 待更新'}`,
    disabled: !entry.hasContent && !entry.sources.some(source => source.enabled && !source.managedSource),
  }));
}

/** A confirmed command stays authoritative when a later catalog read fails. */
export function acceptActivatedConfiguration(
  entries: ConfigurationEntry[], selectedId: string,
  profile: import('$lib/types/domain').ProxyConfigProfile,
): ConfigurationEntry[] {
  const sources = new Map<string, SubscriptionProfile>();
  for (const entry of entries.filter(item => item.id === selectedId || item.profileId === profile.id)) {
    for (const source of entry.sources) sources.set(source.id, { ...source, targetProxyConfigId: profile.id });
  }
  const confirmed: ConfigurationEntry = {
    id: `profile:${profile.id}`, profileId: profile.id, name: profile.name,
    active: true, hasContent: profile.content != null, format: profile.format,
    path: profile.path ?? null, updatedAtUnixMs: profile.updatedAtUnixMs,
    managedSource: profile.managedSource ?? null, sources: [...sources.values()], missingProfile: false,
  };
  let inserted = false;
  const next = entries.flatMap(entry => {
    if (entry.id === selectedId || entry.profileId === profile.id) {
      if (inserted) return [];
      inserted = true; return [confirmed];
    }
    return [{ ...entry, active: false }];
  });
  return inserted ? next : [...next, confirmed];
}
