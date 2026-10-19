import { invoke } from '@tauri-apps/api/core';
import { proxyConfigSignal } from './proxy-config-signal.svelte';
import type { ConfigurationEntry } from '$lib/features/configuration/catalog';
export type { ConfigurationEntry };
export function listConfigurations(): Promise<ConfigurationEntry[]> {
  void proxyConfigSignal.revision;
  return invoke('configuration_list');
}

import { getProxyConfig, setActiveProxyConfig, syncSubscription } from './config';
import type { ProxyConfigProfile } from '$lib/types/domain';

/** Both overview modes use the same entry identity and explicit activation. */
export async function activateConfiguration(entry: ConfigurationEntry): Promise<ProxyConfigProfile> {
  let profileId = entry.hasContent ? entry.profileId : null;
  if (!profileId) {
    const source = entry.sources.find(item => !item.managedSource && item.enabled);
    if (!source) throw new Error('此配置尚无可用内容，请先通过来源插件或配置管理更新');
    const synced = await syncSubscription(source.id);
    profileId = synced.targetProxyConfigId ?? null;
  }
  if (!profileId) throw new Error('更新后仍未生成可用配置');
  return setActiveProxyConfig(profileId);
}
export async function activeConfigurationContent(entries: ConfigurationEntry[]): Promise<ProxyConfigProfile | null> {
  const active = entries.find(entry => entry.active && entry.profileId);
  return active?.profileId ? getProxyConfig(active.profileId) : null;
}
