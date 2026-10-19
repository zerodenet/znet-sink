import { listProxyConfigs, listSubscriptions, getProxyConfig, syncSubscription, setActiveProxyConfig } from './config';
import type { ConfigurationEntry } from '../../src/lib/features/configuration/catalog';
import { proxyConfigSignal } from '../../src/lib/services/proxy-config-signal.svelte';
export async function listConfigurations(): Promise<ConfigurationEntry[]> {
  void proxyConfigSignal.revision;
  const [profiles,sources] = await Promise.all([listProxyConfigs(),listSubscriptions()]);
  const entries: ConfigurationEntry[] = profiles.map(profile=>({id:`profile:${profile.id}`,profileId:profile.id,name:profile.name,
    active:profile.active,hasContent:profile.content!=null,format:profile.format,path:profile.path??null,updatedAtUnixMs:profile.updatedAtUnixMs,
    managedSource:profile.managedSource??null,sources:sources.filter(source=>source.targetProxyConfigId===profile.id),missingProfile:false}));
  for(const source of sources.filter(source=>!profiles.some(profile=>profile.id===source.targetProxyConfigId))) entries.push({
    id:`subscription:${source.id}`,profileId:null,name:source.name,active:false,hasContent:false,format:source.format,path:null,
    updatedAtUnixMs:source.updatedAtUnixMs,managedSource:source.managedSource??null,sources:[source],missingProfile:!!source.targetProxyConfigId,
  });
  return entries;
}
export async function activeConfigurationContent(entries: ConfigurationEntry[]) {
  const active=entries.find(entry=>entry.active);return active?.profileId?getProxyConfig(active.profileId):null;
}
export async function activateConfiguration(entry: ConfigurationEntry) {
  let id=entry.hasContent?entry.profileId:null;
  if(!id){const source=entry.sources.find(source=>source.enabled&&!source.managedSource);if(!source)throw new Error('请先更新配置');id=(await syncSubscription(source.id)).targetProxyConfigId??null;}
  if(!id)throw new Error('无可用配置');return setActiveProxyConfig(id);
}
