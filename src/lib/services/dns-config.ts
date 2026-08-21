import {
  getAppConfig,
  guiApplyDnsConfig,
  guiValidateDnsConfig,
  updateAppConfig,
} from '$lib/services/core';
import { listProxyConfigs } from '$lib/services/config';
import { normalizeConfigValidationResponse } from '$lib/services/config-validation';
import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte';
import { reconcileGuiTunRuntime } from '$lib/services/tun';
import type {
  DnsConfig,
  DnsDraftIssue,
  DnsMode,
  DnsServerConfig,
  DnsServerType,
  DnsSettingsDraft,
} from '$lib/types/dns';

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function clone<T>(value: T): T {
  return structuredClone(value);
}

function defaultPort(type: DnsServerType): number | undefined {
  if (type === 'system') return undefined;
  if (type === 'udp') return 53;
  if (type === 'doh') return 443;
  return 853;
}

export function createDnsServer(type: DnsServerType = 'doh'): DnsServerConfig {
  if (type === 'system') return { type };
  const server: DnsServerConfig = {
    type,
    host: type === 'doh' ? 'cloudflare-dns.com' : '',
    port: defaultPort(type),
    bootstrap: type === 'doh' ? ['1.1.1.1', '1.0.0.1'] : [],
  };
  if (type === 'doh') server.path = '/dns-query';
  return server;
}

export function createDefaultDnsConfig(mode: Exclude<DnsMode, 'disabled'> = 'real'): DnsConfig {
  return {
    servers: { global: createDnsServer('doh') },
    default_server: 'global',
    dispatch: [],
    cache: { max_entries: 1024 },
    answer: mode === 'fake_ip'
      ? {
          type: 'fake_ip',
          cidr: '198.18.0.0/15',
          ttl_seconds: 86_400,
          exclude_domains: [],
        }
      : { type: 'real' },
  };
}

function parseDnsConfig(value: unknown): DnsConfig | null {
  if (!isObject(value) || !isObject(value.servers) || typeof value.default_server !== 'string') {
    return null;
  }
  const answer = isObject(value.answer) && value.answer.type === 'fake_ip'
    ? value.answer
    : isObject(value.answer) ? value.answer : { type: 'real' };
  return {
    ...clone(value),
    servers: clone(value.servers) as Record<string, DnsServerConfig>,
    default_server: value.default_server,
    dispatch: Array.isArray(value.dispatch) ? clone(value.dispatch) : [],
    answer: clone(answer) as DnsConfig['answer'],
  } as DnsConfig;
}

export function readDnsSettings(
  content: unknown,
  appDnsHijack = false,
): DnsSettingsDraft {
  const root = isObject(content) ? content : {};
  const runtime = isObject(root.runtime) ? root.runtime : {};
  const dns = parseDnsConfig(runtime.dns);
  const tun = isObject(runtime.tun) ? runtime.tun : null;
  if (!dns) {
    return {
      mode: 'disabled',
      dns: createDefaultDnsConfig('real'),
      dnsHijack: tun?.dns_hijack === true || appDnsHijack,
      advanced: false,
    };
  }
  const mode: DnsMode = dns.answer.type === 'fake_ip' ? 'fake_ip' : 'real';
  return {
    mode,
    dns,
    dnsHijack: tun ? tun.dns_hijack === true : appDnsHijack,
    advanced: dns.dispatch.length > 0 || Object.keys(dns.servers).length > 1,
  };
}

export function setDnsMode(draft: DnsSettingsDraft, mode: DnsMode): DnsSettingsDraft {
  const next = clone(draft);
  next.mode = mode;
  if (mode === 'real') next.dns.answer = { type: 'real' };
  if (mode === 'fake_ip') {
    const previous = next.dns.answer.type === 'fake_ip' ? next.dns.answer : undefined;
    next.dns.answer = {
      type: 'fake_ip',
      cidr: previous?.cidr ?? '198.18.0.0/15',
      ttl_seconds: previous?.ttl_seconds ?? 86_400,
      max_entries: previous?.max_entries,
      exclude_domains: previous?.exclude_domains ?? [],
    };
    next.dnsHijack = true;
  }
  if (mode === 'disabled') next.dnsHijack = false;
  return next;
}

export function projectDnsSettings(
  source: Record<string, unknown>,
  draft: DnsSettingsDraft,
): Record<string, unknown> {
  const result = clone(source);
  const runtime = isObject(result.runtime) ? result.runtime : {};
  result.runtime = runtime;
  if (draft.mode === 'disabled') delete runtime.dns;
  else runtime.dns = clone(draft.dns);

  if (isObject(runtime.tun)) {
    runtime.tun.dns_hijack = draft.mode !== 'disabled' && draft.dnsHijack;
  }
  return result;
}

export function renameDnsServer(
  dns: DnsConfig,
  oldName: string,
  requestedName: string,
): DnsConfig {
  const name = requestedName.trim();
  if (!name || name !== oldName && Object.hasOwn(dns.servers, name)) {
    throw new Error('DNS 服务器名称不能为空且不能重复');
  }
  if (!Object.hasOwn(dns.servers, oldName) || name === oldName) return clone(dns);
  const next = clone(dns);
  const entries = Object.entries(next.servers).map(([key, value]) => [
    key === oldName ? name : key,
    value,
  ] as const);
  next.servers = Object.fromEntries(entries);
  if (next.default_server === oldName) next.default_server = name;
  next.dispatch = next.dispatch.map((rule) => ({
    ...rule,
    server: rule.server === oldName ? name : rule.server,
  }));
  return next;
}

function isIpAddress(value: string): boolean {
  return /^\d{1,3}(?:\.\d{1,3}){3}$/.test(value)
    || /^[0-9a-f:]+$/i.test(value);
}

export function validateDnsDraft(draft: DnsSettingsDraft): DnsDraftIssue[] {
  if (draft.mode === 'disabled') return [];
  const issues: DnsDraftIssue[] = [];
  const names = Object.keys(draft.dns.servers);
  if (names.length === 0) {
    issues.push({ field: 'servers', message: '至少需要一个 DNS 服务器', severity: 'error' });
  }
  if (!Object.hasOwn(draft.dns.servers, draft.dns.default_server)) {
    issues.push({ field: 'default_server', message: '默认服务器不存在', severity: 'error' });
  }
  for (const [name, server] of Object.entries(draft.dns.servers)) {
    if (!name.trim()) {
      issues.push({ field: 'servers', message: '服务器名称不能为空', severity: 'error' });
    }
    if (server.type !== 'system') {
      const host = server.host?.trim() ?? '';
      if (!host) issues.push({ field: `servers.${name}.host`, message: '服务器地址不能为空', severity: 'error' });
      if (!server.port || server.port < 1 || server.port > 65_535) {
        issues.push({ field: `servers.${name}.port`, message: '端口必须在 1-65535 之间', severity: 'error' });
      }
      if (host && !isIpAddress(host) && !(server.bootstrap?.length)) {
        issues.push({
          field: `servers.${name}.bootstrap`,
          message: '域名形式的端点建议至少提供一个 bootstrap IP；最终以内核校验为准',
          severity: 'warning',
        });
      }
    }
  }
  draft.dns.dispatch.forEach((rule, index) => {
    if (!Object.hasOwn(draft.dns.servers, rule.server)) {
      issues.push({ field: `dispatch.${index}.server`, message: '分流规则引用了不存在的服务器', severity: 'error' });
    }
    if (!isObject(rule.condition)) {
      issues.push({ field: `dispatch.${index}.condition`, message: '分流条件必须是 JSON 对象', severity: 'error' });
    }
  });
  if (draft.mode === 'fake_ip' && draft.dns.answer.type === 'fake_ip') {
    if (!draft.dns.answer.cidr.includes('/')) {
      issues.push({ field: 'answer.cidr', message: 'Fake-IP 地址池必须使用 CIDR', severity: 'error' });
    }
    if (draft.dns.answer.ttl_seconds < 1) {
      issues.push({ field: 'answer.ttl_seconds', message: 'TTL 必须大于 0', severity: 'error' });
    }
    if (draft.dns.answer.max_entries !== undefined && draft.dns.answer.max_entries < 1) {
      issues.push({ field: 'answer.max_entries', message: '映射容量必须大于 0', severity: 'error' });
    }
  }
  return issues;
}

export async function loadActiveDnsSettings(): Promise<{
  source: Record<string, unknown>;
  profileName: string;
  draft: DnsSettingsDraft;
}> {
  const [profiles, appConfig] = await Promise.all([listProxyConfigs(), getAppConfig()]);
  const active = profiles.find((profile) => profile.active);
  if (!active || !isObject(active.content)) throw new Error('当前没有可编辑的活动 Zero 配置');
  return {
    source: clone(active.content),
    profileName: active.name,
    draft: readDnsSettings(active.content, appConfig.tun.dnsHijack),
  };
}

export async function applyActiveDnsSettings(
  source: Record<string, unknown>,
  draft: DnsSettingsDraft,
): Promise<Record<string, unknown>> {
  const errors = validateDnsDraft(draft).filter((issue) => issue.severity === 'error');
  if (errors.length) throw new Error(errors.map((issue) => issue.message).join('；'));
  const next = projectDnsSettings(source, draft);
  const validation = normalizeConfigValidationResponse(await guiValidateDnsConfig(next));
  if (!validation.valid) throw new Error(validation.errors.map((error) => error.message).join('；'));

  const runtime = isObject(next.runtime) ? next.runtime : {};
  const profileOwnsTun = Object.hasOwn(runtime, 'tun');
  const appConfig = await getAppConfig();
  const desiredHijack = draft.mode !== 'disabled' && draft.dnsHijack;
  const result = await guiApplyDnsConfig(next) as Record<string, unknown>;
  if (result?.ok === false) {
    const error = isObject(result.error) ? result.error.message : undefined;
    throw new Error(typeof error === 'string' ? error : '内核拒绝了 DNS 配置');
  }

  try {
    if (!profileOwnsTun && appConfig.tun.dnsHijack !== desiredHijack) {
      await updateAppConfig({ tun: { dnsHijack: desiredHijack } });
      await reconcileGuiTunRuntime();
    }
  } catch (error) {
    await guiApplyDnsConfig(source).catch(() => undefined);
    await updateAppConfig({ tun: { dnsHijack: appConfig.tun.dnsHijack } }).catch(() => undefined);
    throw error;
  }
  proxyConfigSignal.markChanged(true);
  return next;
}
