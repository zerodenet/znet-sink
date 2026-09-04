import {
  getAppConfig,
  guiApplyDnsConfig,
  guiValidateDnsConfig,
  getGuiCoreHealth,
  getGuiZeroCapabilities,
  updateAppConfig,
} from '$lib/services/core';
import { normalizeConfigValidationResponse } from '$lib/services/config-validation';
import {
  projectClientKernelFeatures,
  type ClientKernelFeatures,
} from '$lib/services/kernel-capabilities';
import type {
  DnsAddressFamilyPolicy,
  DnsConfig,
  DnsDraftIssue,
  DnsMode,
  DnsServerConfig,
  DnsServerType,
  DnsSettingsDraft,
  DnsSettingsInput,
} from '$lib/types/dns';

export type DnsKernelCompatibility = {
  status: 'supported' | 'unsupported' | 'unknown';
  apiVersion?: string;
  schemaVersion?: string;
  engineVersion?: string;
  detail?: string;
  limitations?: string[];
  features?: ClientKernelFeatures;
};

export interface DnsValidationContext {
  ruleSetTags?: ReadonlySet<string>;
  routeTargetTags?: ReadonlySet<string>;
  features?: ClientKernelFeatures;
}

export interface DnsAutomaticDefaults {
  features?: ClientKernelFeatures;
  addressFamily?: DnsAddressFamilyPolicy;
}

// Client-only semantic value. The Rust composition layer resolves it against
// the target profile's route.final before sending a configuration to Zero.
export const DNS_DETOUR_ROUTE_FINAL = '$route_final';

const DEFAULT_REVERSE_MAPPING = {
  max_entries: 1024,
  max_domains_per_address: 8,
  max_ttl_seconds: 300,
} as const;

const DNS_ADDRESS_FAMILY_POLICIES: readonly DnsAddressFamilyPolicy[] = [
  'ipv4_only',
  'ipv6_only',
  'prefer_ipv4',
  'prefer_ipv6',
];

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function clone<T>(value: T): T {
  // Svelte 5 state values are Proxies. `structuredClone` rejects those
  // proxies, which made the Fake-IP mode selector fail before save().
  return JSON.parse(JSON.stringify(value)) as T;
}

function defaultPort(type: DnsServerType): number | undefined {
  if (type === 'system') return undefined;
  if (type === 'udp') return 53;
  if (type === 'doh') return 443;
  return 853;
}

export function createDnsServer(type: DnsServerType = 'system'): DnsServerConfig {
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

export function recommendedDnsAddressFamily(
  ipv4Availability: 'unknown' | 'available' | 'unavailable',
  ipv6Availability: 'unknown' | 'available' | 'unavailable',
): DnsAddressFamilyPolicy {
  if (ipv4Availability === 'available' && ipv6Availability === 'unavailable') return 'ipv4_only';
  if (ipv4Availability === 'unavailable' && ipv6Availability === 'available') return 'ipv6_only';
  return 'prefer_ipv4';
}

export function createDefaultDnsConfig(
  mode: Exclude<DnsMode, 'disabled'> = 'real',
  defaults: DnsAutomaticDefaults = {},
): DnsConfig {
  const supportsFakeIpv6 = defaults.features?.dnsFakeIpDualStack.state === 'supported';
  const supportsReverseMapping = defaults.features?.dnsRealReverseMapping.state === 'supported';
  return {
    servers: { system: createDnsServer('system') },
    default_server: 'system',
    dispatch: [],
    cache: { max_entries: 1024 },
    reverse_mapping: supportsReverseMapping ? { ...DEFAULT_REVERSE_MAPPING } : undefined,
    policy: { address_family: defaults.addressFamily ?? 'prefer_ipv4' },
    answer: mode === 'fake_ip'
      ? {
          type: 'fake_ip',
          cidr: '198.18.0.0/15',
          ...(supportsFakeIpv6 ? { ipv6_cidr: 'fd00::/96' } : {}),
          ttl_seconds: 86_400,
          exclude_domains: [],
        }
      : { type: 'real' },
  };
}

export function createRecommendedDnsConfig(
  mode: Exclude<DnsMode, 'disabled'> = 'real',
  defaults: DnsAutomaticDefaults = {},
): DnsConfig {
  const dns = createDefaultDnsConfig(mode, defaults);
  dns.servers = {
    cloudflare: {
      type: 'doh',
      host: 'cloudflare-dns.com',
      port: 443,
      path: '/dns-query',
      bootstrap: ['1.1.1.1', '1.0.0.1'],
      detour: DNS_DETOUR_ROUTE_FINAL,
    },
    google: {
      type: 'doh',
      host: 'dns.google',
      port: 443,
      path: '/dns-query',
      bootstrap: ['8.8.8.8', '8.8.4.4'],
      detour: DNS_DETOUR_ROUTE_FINAL,
    },
    'cloudflare-bootstrap': {
      type: 'doh',
      host: 'cloudflare-dns.com',
      port: 443,
      path: '/dns-query',
      bootstrap: ['1.1.1.1', '1.0.0.1'],
    },
    'google-bootstrap': {
      type: 'doh',
      host: 'dns.google',
      port: 443,
      path: '/dns-query',
      bootstrap: ['8.8.8.8', '8.8.4.4'],
    },
    alidns: {
      type: 'doh',
      host: 'dns.alidns.com',
      port: 443,
      path: '/dns-query',
      bootstrap: ['223.5.5.5', '223.6.6.6'],
    },
    '114dns': {
      type: 'udp',
      host: '114.114.114.114',
      port: 53,
    },
    system: createDnsServer('system'),
  };
  dns.default_server = 'cloudflare';
  dns.policy = {
    ...dns.policy,
    fallback_servers: ['google', 'system'],
    node_server: 'system',
    node_fallback_servers: ['cloudflare-bootstrap', 'google-bootstrap'],
  };
  return dns;
}

export function parseDnsConfig(value: unknown): DnsConfig | null {
  if (!isObject(value) || !isObject(value.servers) || typeof value.default_server !== 'string') {
    return null;
  }
  // Client-created drafts retain optional keys with `undefined`, while the
  // same values disappear after JSON serialization. Treat both shapes alike
  // so validation never discards an in-memory draft and silently replaces it
  // with recommended defaults.
  if (value.dispatch !== undefined && !Array.isArray(value.dispatch)) return null;
  if (value.answer !== undefined && !isObject(value.answer)) return null;
  if (value.policy !== undefined && !isObject(value.policy)) return null;
  if (value.reverse_mapping !== undefined && !isObject(value.reverse_mapping)) return null;
  const answer = clone(isObject(value.answer) ? value.answer : { type: 'real' }) as DnsConfig['answer'];
  if (answer.type === 'fake_ip') {
    answer.exclude_domains = Array.isArray(answer.exclude_domains)
      ? answer.exclude_domains.filter((domain): domain is string => typeof domain === 'string')
      : [];
  }
  const reverseMapping = isObject(value.reverse_mapping)
    ? {
        ...clone(value.reverse_mapping),
        max_entries: typeof value.reverse_mapping.max_entries === 'number' ? value.reverse_mapping.max_entries : 1024,
        max_domains_per_address: typeof value.reverse_mapping.max_domains_per_address === 'number'
          ? value.reverse_mapping.max_domains_per_address
          : 8,
        max_ttl_seconds: typeof value.reverse_mapping.max_ttl_seconds === 'number'
          ? value.reverse_mapping.max_ttl_seconds
          : 300,
      }
    : undefined;
  return {
    ...clone(value),
    servers: clone(value.servers) as Record<string, DnsServerConfig>,
    default_server: value.default_server,
    dispatch: Array.isArray(value.dispatch) ? clone(value.dispatch) : [],
    reverse_mapping: reverseMapping,
    answer,
    policy: isObject(value.policy) ? clone(value.policy) : undefined,
  } as DnsConfig;
}

export function getDnsAddressFamilyPolicy(dns: DnsConfig): DnsAddressFamilyPolicy {
  const value = dns.policy?.address_family;
  return DNS_ADDRESS_FAMILY_POLICIES.includes(value as DnsAddressFamilyPolicy)
    ? value as DnsAddressFamilyPolicy
    : 'prefer_ipv4';
}

export function setDnsAddressFamilyPolicy(
  dns: DnsConfig,
  addressFamily: DnsAddressFamilyPolicy,
): DnsConfig {
  if (!DNS_ADDRESS_FAMILY_POLICIES.includes(addressFamily)) return clone(dns);
  const next = clone(dns);
  next.policy = {
    ...next.policy,
    address_family: addressFamily,
  };
  return next;
}

export function readDnsSettings(
  content: unknown,
  appDnsHijack = false,
): DnsSettingsDraft {
  const root = isObject(content) ? content : {};
  const enabled = root.enabled === true;
  const dns = parseDnsConfig(root.config);
  if (!dns) {
    return {
      mode: enabled ? 'real' : 'disabled',
      dns: createRecommendedDnsConfig('real'),
      dnsHijack: root.dnsHijack === true || appDnsHijack,
      advanced: enabled,
    };
  }
  const mode: DnsMode = dns.answer.type === 'fake_ip' ? 'fake_ip' : 'real';
  return {
    mode: enabled ? mode : 'disabled',
    dns,
    dnsHijack: root.dnsHijack === true || appDnsHijack,
    advanced: dns.dispatch.length > 0 || Object.keys(dns.servers).length > 1 || Boolean(dns.cache),
  };
}

export function setDnsMode(
  draft: DnsSettingsDraft,
  mode: DnsMode,
  defaults: DnsAutomaticDefaults = {},
): DnsSettingsDraft {
  const next = clone(draft);
  next.mode = mode;
  if (defaults.addressFamily) {
    next.dns = setDnsAddressFamilyPolicy(next.dns, defaults.addressFamily);
  }
  if (mode === 'real') next.dns.answer = { type: 'real' };
  if (mode === 'fake_ip') {
    const previous = next.dns.answer.type === 'fake_ip' ? next.dns.answer : undefined;
    next.dns.answer = {
      ...previous,
      type: 'fake_ip',
      cidr: previous?.cidr ?? '198.18.0.0/15',
      ipv6_cidr: previous?.ipv6_cidr
        ?? (defaults.features?.dnsFakeIpDualStack.state === 'supported' ? 'fd00::/96' : undefined),
      ttl_seconds: previous?.ttl_seconds ?? 86_400,
      max_entries: previous?.max_entries,
      exclude_domains: previous?.exclude_domains ?? [],
    };
    if (!next.dns.reverse_mapping
      && defaults.features?.dnsRealReverseMapping.state === 'supported') {
      next.dns.reverse_mapping = { ...DEFAULT_REVERSE_MAPPING };
    }
    next.dnsHijack = true;
  }
  if (mode === 'disabled') next.dnsHijack = false;
  return next;
}

export function projectDnsSettings(
  _source: DnsSettingsInput,
  draft: DnsSettingsDraft,
): DnsSettingsInput {
  return {
    enabled: draft.mode !== 'disabled',
    // Keep the edited object while disabled so a user can prepare DNS and
    // routing policy before enabling it. The effective config still omits
    // runtime.dns until enabled is true.
    config: clone(draft.dns),
    dnsHijack: draft.mode !== 'disabled' && draft.dnsHijack,
  };
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
  if (next.policy) {
    const renameReference = (value: string | undefined) => value === oldName ? name : value;
    const renameReferences = (values: string[] | undefined) => values?.map((value) => value === oldName ? name : value);
    next.policy.node_server = renameReference(next.policy.node_server);
    next.policy.direct_server = renameReference(next.policy.direct_server);
    next.policy.fallback_servers = renameReferences(next.policy.fallback_servers);
    next.policy.node_fallback_servers = renameReferences(next.policy.node_fallback_servers);
    next.policy.direct_fallback_servers = renameReferences(next.policy.direct_fallback_servers);
    if (next.policy.server_timeout_ms && Object.hasOwn(next.policy.server_timeout_ms, oldName)) {
      const timeout = next.policy.server_timeout_ms[oldName];
      delete next.policy.server_timeout_ms[oldName];
      next.policy.server_timeout_ms[name] = timeout;
    }
  }
  return next;
}

function isIpAddress(value: string): boolean {
  return /^\d{1,3}(?:\.\d{1,3}){3}$/.test(value)
    || /^[0-9a-f:]+$/i.test(value);
}

export function validateDnsDraft(
  draft: DnsSettingsDraft,
  context: DnsValidationContext = {},
): DnsDraftIssue[] {
  if (draft.mode === 'disabled') return [];
  const issues: DnsDraftIssue[] = [];
  const addressFamily = draft.dns.policy?.address_family;
  if (addressFamily !== undefined
    && !DNS_ADDRESS_FAMILY_POLICIES.includes(addressFamily as DnsAddressFamilyPolicy)) {
    issues.push({
      field: 'policy.address_family',
      message: '地址族策略必须是受支持的 IPv4/IPv6 模式',
      severity: 'error',
    });
  }
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
      const detour = server.detour?.trim();
      if (detour && server.type === 'doq') {
        issues.push({
          field: `servers.${name}.detour`,
          message: 'DoQ 暂不支持通过出站转发，请移除 detour 或改用 DoH/DoT',
          severity: 'error',
        });
      } else if (detour
        && detour !== DNS_DETOUR_ROUTE_FINAL
        && context.routeTargetTags
        && !context.routeTargetTags.has(detour)) {
        issues.push({
          field: `servers.${name}.detour`,
          message: `出站 ${detour} 已不存在或未进入活动配置`,
          severity: 'error',
        });
      }
    }
  }
  const policy = draft.dns.policy;
  if (policy) {
    const timeoutInRange = (value: number | undefined) => value === undefined
      || Number.isInteger(value) && value >= 1 && value <= 120_000;
    if (!timeoutInRange(policy.timeout_ms)) {
      issues.push({ field: 'policy.timeout_ms', message: '查询超时必须是 1-120000 毫秒的整数', severity: 'error' });
    }
    for (const [name, timeout] of Object.entries(policy.server_timeout_ms ?? {})) {
      if (!Object.hasOwn(draft.dns.servers, name)) {
        issues.push({ field: `policy.server_timeout_ms.${name}`, message: `服务器 ${name} 不存在`, severity: 'error' });
      } else if (!timeoutInRange(timeout)) {
        issues.push({ field: `policy.server_timeout_ms.${name}`, message: '查询超时必须是 1-120000 毫秒的整数', severity: 'error' });
      }
    }

    const validateFallbacks = (field: string, values: string[] | undefined, primary?: string) => {
      const seen = new Set<string>();
      for (const name of values ?? []) {
        if (!Object.hasOwn(draft.dns.servers, name)) {
          issues.push({ field, message: `服务器 ${name} 不存在`, severity: 'error' });
        } else if (seen.has(name)) {
          issues.push({ field, message: `回退链重复包含服务器 ${name}`, severity: 'error' });
        } else if (primary === name) {
          issues.push({ field, message: `回退链不能重复主服务器 ${name}`, severity: 'error' });
        }
        seen.add(name);
      }
    };
    const validatePrimary = (field: string, value: string | undefined) => {
      if (value && !Object.hasOwn(draft.dns.servers, value)) {
        issues.push({ field, message: `服务器 ${value} 不存在`, severity: 'error' });
      }
    };
    validateFallbacks('policy.fallback_servers', policy.fallback_servers, draft.dns.default_server);
    validatePrimary('policy.node_server', policy.node_server);
    validatePrimary('policy.direct_server', policy.direct_server);
    validateFallbacks('policy.node_fallback_servers', policy.node_fallback_servers, policy.node_server);
    validateFallbacks('policy.direct_fallback_servers', policy.direct_fallback_servers, policy.direct_server);
    if ((policy.node_fallback_servers?.length ?? 0) > 0 && !policy.node_server) {
      issues.push({ field: 'policy.node_fallback_servers', message: '配置节点回退链前必须选择节点解析服务器', severity: 'error' });
    }
    if ((policy.direct_fallback_servers?.length ?? 0) > 0 && !policy.direct_server) {
      issues.push({ field: 'policy.direct_fallback_servers', message: '配置直连回退链前必须选择直连解析服务器', severity: 'error' });
    }
    for (const cidr of policy.reject_address_cidrs ?? []) {
      if (!cidr.includes('/')) {
        issues.push({ field: 'policy.reject_address_cidrs', message: `${cidr} 不是有效的 CIDR`, severity: 'error' });
      }
    }
  }
  const detouredServers = Object.entries(draft.dns.servers)
    .filter(([, server]) => Boolean(server.detour?.trim()));
  if (detouredServers.length > 0) {
    const nodeServer = policy?.node_server;
    if (!nodeServer) {
      issues.push({
        field: 'policy.node_server',
        message: '存在通过出站转发的 DNS 上游，必须选择一个直连的节点解析服务器以避免递归',
        severity: 'error',
      });
    }
    for (const name of [nodeServer, ...(policy?.node_fallback_servers ?? [])].filter(Boolean) as string[]) {
      if (draft.dns.servers[name]?.detour) {
        issues.push({
          field: 'policy.node_server',
          message: `节点解析服务器 ${name} 不能再通过 detour 转发`,
          severity: 'error',
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
    } else if (rule.condition.type === 'rule_set' || typeof rule.condition.rule_set === 'string') {
      const rawTag = rule.condition.type === 'rule_set'
        ? rule.condition.tag
        : rule.condition.rule_set;
      const tag = typeof rawTag === 'string' ? rawTag.trim() : '';
      if (!tag) {
        issues.push({ field: `dispatch.${index}.condition.tag`, message: '请选择有效规则集', severity: 'error' });
      } else if (context.ruleSetTags && !context.ruleSetTags.has(tag)) {
        issues.push({
          field: `dispatch.${index}.condition.tag`,
          message: `规则集 ${tag} 已不存在或未进入最终有效配置`,
          severity: 'error',
        });
      }
    }
  });
  if (draft.dns.dispatch.length > 0 && context.features?.dnsSplitDispatch.state === 'unsupported') {
    issues.push({ field: 'dispatch', message: '当前内核不支持 DNS 分流', severity: 'error' });
  }
  if (draft.dns.policy?.address_family
    && context.features?.dnsAddressFamilyPolicy.state === 'unsupported') {
    issues.push({ field: 'policy.address_family', message: '当前内核不支持 DNS 地址族策略', severity: 'error' });
  }
  if (draft.dnsHijack && context.features?.tunDnsHijack.state === 'unsupported') {
    issues.push({ field: 'dnsHijack', message: '当前内核不支持 TUN DNS 劫持', severity: 'error' });
  }
  const usesSystemDns = Object.values(draft.dns.servers).some((server) => server.type === 'system');
  if (draft.dnsHijack && usesSystemDns
    && context.features?.tunDnsSystemAuto.state === 'unsupported') {
    issues.push({
      field: 'dnsHijack',
      message: '当前内核不能在 TUN DNS 劫持时自动排除 system DNS；请升级内核、关闭劫持或改用显式网络 DNS',
      severity: 'error',
    });
  }
  if (draft.dns.cache) {
    if (!Number.isInteger(draft.dns.cache.max_entries) || draft.dns.cache.max_entries < 1) {
      issues.push({ field: 'cache.max_entries', message: '缓存容量必须是大于 0 的整数', severity: 'error' });
    }
    if (draft.dns.cache.max_ttl_seconds !== undefined
      && (!Number.isInteger(draft.dns.cache.max_ttl_seconds) || draft.dns.cache.max_ttl_seconds < 1)) {
      issues.push({ field: 'cache.max_ttl_seconds', message: '最大 TTL 必须是大于 0 的整数', severity: 'error' });
    }
  }
  if (draft.dns.reverse_mapping) {
    const reverse = draft.dns.reverse_mapping;
    if (!Number.isInteger(reverse.max_entries) || reverse.max_entries < 1) {
      issues.push({ field: 'reverse_mapping.max_entries', message: '真实地址映射容量必须是大于 0 的整数', severity: 'error' });
    }
    if (!Number.isInteger(reverse.max_domains_per_address) || reverse.max_domains_per_address < 2) {
      issues.push({ field: 'reverse_mapping.max_domains_per_address', message: '每个地址至少保留 2 个候选域名，才能识别共享地址歧义', severity: 'error' });
    }
    if (!Number.isInteger(reverse.max_ttl_seconds) || reverse.max_ttl_seconds < 1) {
      issues.push({ field: 'reverse_mapping.max_ttl_seconds', message: '映射 TTL 必须是大于 0 的整数', severity: 'error' });
    }
    if (context.features?.dnsRealReverseMapping.state === 'unsupported') {
      issues.push({ field: 'reverse_mapping', message: '当前内核不支持真实地址反向映射', severity: 'error' });
    }
  }
  if (draft.mode === 'fake_ip' && draft.dns.answer.type === 'fake_ip') {
    if (!draft.dns.answer.cidr.includes('/')) {
      issues.push({ field: 'answer.cidr', message: 'Fake-IP 地址池必须使用 CIDR', severity: 'error' });
    }
    if (draft.dns.answer.ipv6_cidr !== undefined
      && (!draft.dns.answer.ipv6_cidr.includes('/') || !draft.dns.answer.ipv6_cidr.includes(':'))) {
      issues.push({ field: 'answer.ipv6_cidr', message: 'FakeIPv6 地址池必须使用 IPv6 CIDR', severity: 'error' });
    }
    if (draft.dns.answer.ipv6_cidr
      && context.features?.dnsFakeIpDualStack.state === 'unsupported') {
      issues.push({ field: 'answer.ipv6_cidr', message: '当前内核不支持双栈 Fake-IP', severity: 'error' });
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

export async function loadGlobalDnsSettings(): Promise<{
  source: DnsSettingsInput;
  draft: DnsSettingsDraft;
}> {
  const appConfig = await getAppConfig();
  const source = {
    enabled: appConfig.dns.enabled,
    config: appConfig.dns.config,
    dnsHijack: appConfig.dns.dnsHijack,
  };
  return {
    source,
    draft: readDnsSettings(source),
  };
}

function normalizeCapability(value: string): string {
  return value.toLowerCase().replace(/[._-]/g, '');
}

/**
 * Negotiate DNS support with the running kernel. Older kernels may not expose
 * capabilities at all, so absence of metadata is deliberately treated as
 * unknown instead of blocking the editor.
 */
export async function getDnsKernelCompatibility(): Promise<DnsKernelCompatibility> {
  const [capabilityResult, healthResult] = await Promise.allSettled([
    getGuiZeroCapabilities(),
    getGuiCoreHealth(),
  ]);
  const health = healthResult.status === 'fulfilled' ? healthResult.value : undefined;
  if (capabilityResult.status === 'rejected') {
    return {
      status: 'unknown',
      engineVersion: health?.engineVersion,
      detail: '当前内核未提供能力查询接口，保存时会继续进行兼容校验。',
    };
  }

  const capabilities = capabilityResult.value;
  const apiVersion = capabilities.apiVersion;
  const schemaVersion = capabilities.schemaVersion;
  const engineVersion = health?.engineVersion;
  const limitations = capabilities.globalLimitations.filter((limitation) =>
    limitation.startsWith('dns_') || limitation.startsWith('tun_dns_'),
  );
  const features = projectClientKernelFeatures(capabilities);
  if (!capabilities.available) {
    return {
      status: 'unknown',
      apiVersion,
      schemaVersion,
      engineVersion,
      limitations,
      features,
      detail: capabilities.error || '内核当前不可用，暂时无法确认 DNS 能力。',
    };
  }

  const capabilityContract = capabilities.contracts?.capabilities;
  if (!capabilityContract) {
    return {
      status: 'unknown',
      apiVersion,
      schemaVersion,
      engineVersion,
      limitations,
      features,
      detail: '内核未发布稳定能力契约版本，将按旧版兼容路径在保存时校验。',
    };
  }
  if (capabilityContract.minimumSupported > 1 || capabilityContract.current < 1) {
    return {
      status: 'unknown',
      apiVersion,
      schemaVersion,
      engineVersion,
      limitations,
      features,
      detail: `内核能力契约 v${capabilityContract.minimumSupported}–v${capabilityContract.current} 与客户端支持的 V1 不相交。`,
    };
  }

  const declared = [...capabilities.buildFeatures, ...capabilities.features]
    .map(normalizeCapability);
  const hasDns = declared.some((feature) =>
    feature === 'dns'
      || feature.startsWith('dns')
      || feature.includes('fakeip')
      || feature === 'tundnssystemauto'
  );
  return {
    status: hasDns ? 'supported' : capabilities.buildFeatures.length > 0 ? 'unsupported' : 'unknown',
    apiVersion,
    schemaVersion,
    engineVersion,
    limitations,
    features,
    detail: hasDns
      ? undefined
      : capabilities.buildFeatures.length > 0
        ? '当前内核未声明 DNS/Fake-IP 能力。'
        : '当前内核没有返回可识别的构建能力。',
  };
}

/** Persist a draft for a kernel that cannot apply it yet. */
export async function persistGlobalDnsSettings(
  source: DnsSettingsInput,
  draft: DnsSettingsDraft,
): Promise<DnsSettingsInput> {
  const errors = validateDnsDraft(draft).filter((issue) => issue.severity === 'error');
  if (errors.length) throw new Error(errors.map((issue) => issue.message).join('；'));
  const next = projectDnsSettings(source, draft);
  await updateAppConfig({
    dns: {
      enabled: next.enabled,
      config: next.config ?? null,
      dnsHijack: next.dnsHijack,
    },
  });
  return next;
}

export async function applyGlobalDnsSettings(
  source: DnsSettingsInput,
  draft: DnsSettingsDraft,
): Promise<DnsSettingsInput> {
  const errors = validateDnsDraft(draft).filter((issue) => issue.severity === 'error');
  if (errors.length) throw new Error(errors.map((issue) => issue.message).join('；'));
  const next = projectDnsSettings(source, draft);
  const validation = normalizeConfigValidationResponse(await guiValidateDnsConfig(next));
  if (!validation.valid) throw new Error(validation.errors.map((error) => error.message).join('；'));
  const result = await guiApplyDnsConfig(next) as Record<string, unknown>;
  if (result?.ok === false) {
    const error = isObject(result.error) ? result.error.message : undefined;
    throw new Error(typeof error === 'string' ? error : '内核拒绝了 DNS 配置');
  }

  return next;
}
