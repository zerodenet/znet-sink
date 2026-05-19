import { invoke } from '@tauri-apps/api/core';

export interface ProxyConfig {
  id: string;
  name: string;
  type: string;
  server: string;
  port: number;
  uuid?: string;
  password?: string;
  cipher?: string;
}

export interface Subscription {
  id: string;
  name: string;
  url: string;
  last_sync?: number;
  node_count: number;
}

export interface RuleSet {
  id: string;
  name: string;
  type: string;
  path: string;
  rule_count: number;
}

// 代理配置
export async function listProxyConfigs(): Promise<ProxyConfig[]> {
  return invoke('proxy_config_list');
}

export async function getProxyConfig(id: string): Promise<ProxyConfig> {
  return invoke('proxy_config_get', { id });
}

export async function upsertProxyConfig(config: Partial<ProxyConfig>): Promise<void> {
  return invoke('proxy_config_upsert', { config });
}

export async function setActiveProxyConfig(id: string): Promise<void> {
  return invoke('proxy_config_set_active', { id });
}

export async function removeProxyConfig(id: string): Promise<void> {
  return invoke('proxy_config_remove', { id });
}

// 订阅
export async function listSubscriptions(): Promise<Subscription[]> {
  return invoke('subscription_list');
}

export async function getSubscription(id: string): Promise<Subscription> {
  return invoke('subscription_get', { id });
}

export async function upsertSubscription(sub: Partial<Subscription>): Promise<void> {
  return invoke('subscription_upsert', { subscription: sub });
}

export async function syncSubscription(id: string): Promise<void> {
  return invoke('subscription_sync', { id });
}

export async function removeSubscription(id: string): Promise<void> {
  return invoke('subscription_remove', { id });
}

// 规则集
export async function listRuleSets(): Promise<RuleSet[]> {
  return invoke('rule_set_list');
}

export async function getRuleSet(id: string): Promise<RuleSet> {
  return invoke('rule_set_get', { id });
}

export async function upsertRuleSet(ruleSet: Partial<RuleSet>): Promise<void> {
  return invoke('rule_set_upsert', { ruleSet });
}

export async function removeRuleSet(id: string): Promise<void> {
  return invoke('rule_set_remove', { id });
}
