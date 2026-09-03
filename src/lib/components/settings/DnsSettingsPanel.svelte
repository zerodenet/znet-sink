<script lang="ts">
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { Textarea } from '$lib/components/ui/textarea';
  import { onMount } from 'svelte';
  import { AlertTriangle, Braces, ChevronDown, ChevronUp, FileDiff, Pencil, Plus, RefreshCw, RotateCcw, Save, Server, Trash2 } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import ErrorRecoveryActions from '$lib/components/core/ErrorRecoveryActions.svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { Switch } from '$lib/components/ui/switch';
  import {
    DNS_DETOUR_ROUTE_FINAL,
    applyGlobalDnsSettings,
    createDefaultDnsConfig,
    createRecommendedDnsConfig,
    createDnsServer,
    getDnsKernelCompatibility,
    getDnsAddressFamilyPolicy,
    loadGlobalDnsSettings,
    parseDnsConfig,
    persistGlobalDnsSettings,
    readDnsSettings,
    recommendedDnsAddressFamily,
    renameDnsServer,
    setDnsAddressFamilyPolicy,
    setDnsMode,
    validateDnsDraft,
  } from '$lib/services/dns-config';
  import { getEffectiveRuleSetOptions } from '$lib/services/config';
  import {
    getAppErrorInfo,
    getAppErrorMessage,
    getConfigPolicyGroups,
    guiInspectDnsEffectiveConfig,
    type DnsEffectiveConfigInspection,
  } from '$lib/services/core';
  import { compactConfigValue, effectiveConfigDiff } from '$lib/services/config-diff';
  import { getGuiTunStatus } from '$lib/services/tun';
  import { ruleSetSignal } from '$lib/services/rule-set-signal.svelte';
  import { store } from '$lib/services/store.svelte';
  import type { DnsKernelCompatibility } from '$lib/services/dns-config';
  import type { DnsAddressFamilyPolicy, DnsDispatchConfig, DnsMode, DnsPolicyConfig, DnsServerConfig, DnsServerType, DnsSettingsDraft, DnsSettingsInput } from '$lib/types/dns';
  import type { EffectiveRuleSetOption } from '$lib/types/domain';

  let loading = $state(true);
  let saving = $state(false);
  let error = $state('');
  let errorCode = $state<string | undefined>(undefined);
  let saved = $state(false);
  let savedPending = $state(false);
  let source = $state<DnsSettingsInput | null>(null);
  let draft = $state<DnsSettingsDraft | null>(null);
  let jsonDialogOpen = $state(false);
  let nativeJson = $state('');
  let nativeError = $state('');
  let serverDialogOpen = $state(false);
  let editingServerName = $state<string | null>(null);
  let serverNameDraft = $state('');
  let serverDraft = $state<DnsServerConfig>(createDnsServer('udp'));
  let serverDialogError = $state('');
  let dispatchDialogOpen = $state(false);
  let editingDispatchIndex = $state<number | null>(null);
  let dispatchEditorMode = $state<'form' | 'json'>('form');
  let dispatchConditionDraft = $state('');
  let dispatchConditionType = $state('domain');
  let dispatchConditionValuesDraft = $state('');
  let dispatchConditionTagDraft = $state('');
  let dispatchConditionBase = $state<Record<string, unknown>>({});
  let dispatchServerDraft = $state('');
  let dispatchDialogError = $state('');
  let compatibility = $state<DnsKernelCompatibility>({ status: 'unknown' });
  let ruleSetOptions = $state<EffectiveRuleSetOption[]>([]);
  let routeTargetOptions = $state<Array<{ tag: string; label: string }>>([]);
  let routeTargetsKnown = $state(false);
  let advancedOpen = $state(false);
  let effectiveDialogOpen = $state(false);
  let effectiveLoading = $state(false);
  let effectiveError = $state('');
  let effectiveInspection = $state<DnsEffectiveConfigInspection | null>(null);
  let automaticAddressFamilyPolicy = $state<DnsAddressFamilyPolicy>('prefer_ipv4');

  const ruleSetTags = $derived(new Set(ruleSetOptions.map((option) => option.tag)));
  const routeTargetTags = $derived(new Set(routeTargetOptions.map((option) => option.tag)));
  const issues = $derived(draft ? validateDnsDraft(draft, {
    ruleSetTags,
    routeTargetTags: routeTargetsKnown ? routeTargetTags : undefined,
    features: compatibility.features,
  }) : []);
  const errors = $derived(issues.filter((issue) => issue.severity === 'error'));
  const warnings = $derived(issues.filter((issue) => issue.severity === 'warning'));
  const danglingRuleTags = $derived(draft ? draft.dns.dispatch.flatMap((rule) => {
    const condition = rule.condition;
    const rawTag = condition.type === 'rule_set' ? condition.tag : condition.rule_set;
    const tag = typeof rawTag === 'string' ? rawTag.trim() : '';
    return tag && !ruleSetTags.has(tag) ? [tag] : [];
  }).filter((tag, index, items) => items.indexOf(tag) === index) : []);
  const effectiveDiff = $derived(effectiveInspection
    ? effectiveConfigDiff(
        effectiveInspection.baseConfig,
        effectiveInspection.effectiveConfig,
        effectiveInspection.sources,
      )
    : []);
  const usesSystemDns = $derived(draft
    ? Object.values(draft.dns.servers).some((server) => server.type === 'system')
    : false);
  const serverNames = $derived(draft ? Object.keys(draft.dns.servers) : []);
  const directNodeServerNames = $derived(draft
    ? serverNames.filter((name) => !draft?.dns.servers[name]?.detour)
    : []);
  const hasDnsDetour = $derived(draft
    ? Object.values(draft.dns.servers).some((server) => Boolean(server.detour?.trim()))
    : false);
  const addressFamilyPolicy = $derived(draft ? getDnsAddressFamilyPolicy(draft.dns) : 'prefer_ipv4');
  const modeDescription = $derived(draft
    ? ({
        disabled: '不接管域名解析。',
        real: '返回真实 IP，适合常规代理和内网。',
        fake_ip: '返回合成地址，由内核恢复域名并分流。',
      }[draft.mode] ?? '')
    : '');
  const serverTypeOptions: Array<{ value: DnsServerType; label: string }> = [
    { value: 'udp', label: 'UDP' },
    { value: 'doh', label: 'DoH' },
    { value: 'dot', label: 'DoT' },
    { value: 'doq', label: 'DoQ' },
    { value: 'system', label: 'system' },
  ];
  const addressFamilyOptions: Array<{
    value: DnsAddressFamilyPolicy;
    label: string;
    description: string;
  }> = [
    {
      value: 'prefer_ipv4',
      label: '双栈 · IPv4 优先',
      description: '同时保留 IPv4/IPv6，优先 IPv4。',
    },
    {
      value: 'prefer_ipv6',
      label: '双栈 · IPv6 优先',
      description: '同时保留 IPv4/IPv6，优先 IPv6。',
    },
    {
      value: 'ipv4_only',
      label: '仅 IPv4',
      description: '只使用 IPv4 解析结果。',
    },
    {
      value: 'ipv6_only',
      label: '仅 IPv6',
      description: '只使用 IPv6 解析结果。',
    },
  ];
  const addressFamilyDescription = $derived(
    addressFamilyOptions.find((option) => option.value === addressFamilyPolicy)?.description ?? '',
  );
  const dispatchConditionOptions = [
    { value: 'domain', label: '域名', placeholder: 'example.com' },
    { value: 'domain_keyword', label: '域名关键字', placeholder: 'internal' },
    { value: 'domain_regex', label: '域名正则', placeholder: '(?i)^api\\.example\\.com$' },
    { value: 'ip', label: 'IP / CIDR', placeholder: '10.0.0.0/8' },
    { value: 'geoip', label: 'GeoIP', placeholder: 'CN' },
    { value: 'sni', label: 'SNI', placeholder: 'example.com' },
    { value: 'inbound', label: '入站标签', placeholder: 'mixed-in' },
    { value: 'rule_set', label: '规则集', placeholder: 'AI-Suite' },
  ] as const;
  const unsetSelection = '__znet_unset__';

  function dnsDetourLabel(detour?: string): string {
    if (!detour) return '直接连接';
    if (detour === DNS_DETOUR_ROUTE_FINAL) return '跟随默认出站';
    if (detour === 'block') return '阻断';
    return routeTargetOptions.find((option) => option.tag === detour)?.label ?? detour;
  }
  type PolicyServerField = 'node_server' | 'direct_server';
  type PolicyFallbackField = 'fallback_servers' | 'node_fallback_servers' | 'direct_fallback_servers';
  type ReverseMappingField = 'max_entries' | 'max_domains_per_address' | 'max_ttl_seconds';

  function cloneDnsValue<T>(value: T): T {
    return JSON.parse(JSON.stringify(value)) as T;
  }

  function isFormConditionType(value: unknown): value is (typeof dispatchConditionOptions)[number]['value'] {
    return typeof value === 'string' && dispatchConditionOptions.some((option) => option.value === value);
  }

  function loadDispatchConditionForm(condition: Record<string, unknown>) {
    let type = condition.type;
    let values = condition.values;
    let tag = condition.tag;
    let base = cloneDnsValue(condition);

    if (!isFormConditionType(type)) {
      const legacyType = dispatchConditionOptions.find((option) => Object.hasOwn(condition, option.value))?.value;
      if (!legacyType) return false;
      type = legacyType;
      if (legacyType === 'rule_set') tag = condition[legacyType];
      else values = condition[legacyType];
      base = {};
    }

    if (!isFormConditionType(type)) return false;
    if (type === 'rule_set') {
      if (typeof tag !== 'string') return false;
      dispatchConditionTagDraft = tag;
      dispatchConditionValuesDraft = '';
    } else {
      if (!Array.isArray(values) || values.some((value) => typeof value !== 'string')) return false;
      dispatchConditionValuesDraft = values.join('\n');
      dispatchConditionTagDraft = '';
    }
    dispatchConditionType = type;
    dispatchConditionBase = base;
    return true;
  }

  function buildDispatchConditionFromForm(): Record<string, unknown> {
    const condition = cloneDnsValue(dispatchConditionBase);
    delete condition.values;
    delete condition.tag;
    delete condition.items;
    for (const option of dispatchConditionOptions) delete condition[option.value];
    condition.type = dispatchConditionType;
    if (dispatchConditionType === 'rule_set') {
      const tag = dispatchConditionTagDraft.trim();
      if (!tag || !ruleSetTags.has(tag)) throw new Error('请选择最终有效配置中的规则集');
      condition.tag = tag;
      return condition;
    }
    const values = dispatchConditionValuesDraft
      .split('\n')
      .map((value) => value.trim())
      .filter(Boolean);
    if (values.length === 0) throw new Error('请至少输入一个匹配值');
    condition.values = values;
    return condition;
  }

  function dispatchConditionSummary(condition: Record<string, unknown>): string {
    const labels: Record<string, string> = {
      domain: '域名',
      domain_keyword: '域名关键字',
      domain_regex: '域名正则',
      ip: 'IP / CIDR',
      geoip: 'GeoIP',
      sni: 'SNI',
      inbound: '入站',
      rule_set: '规则集',
      and: '同时满足',
      or: '满足任一',
      not: '排除',
    };
    const explicitType = typeof condition.type === 'string' ? condition.type : '';
    const type = explicitType || dispatchConditionOptions.find((option) => Object.hasOwn(condition, option.value))?.value || '';
    if (type === 'rule_set') {
      const tag = String(condition.tag ?? condition.rule_set ?? '').trim();
      const name = ruleSetOptions.find((option) => option.tag === tag)?.name;
      return `${labels[type]} · ${name || tag || '未选择'}`;
    }
    if (type === 'and' || type === 'or') {
      const items = condition.items ?? condition.conditions ?? condition.values;
      const count = Array.isArray(items) ? items.length : 0;
      return `${labels[type]} · ${count ? `${count} 个` : '多个'}条件`;
    }
    if (type === 'not') return labels[type];
    const rawValues = explicitType ? condition.values : condition[type];
    if (Array.isArray(rawValues)) {
      const values = rawValues.filter((value): value is string => typeof value === 'string');
      const visible = values.slice(0, 2).join('、');
      const more = values.length > 2 ? ` 等 ${values.length} 项` : '';
      return `${labels[type] ?? '匹配条件'} · ${visible || '未填写'}${more}`;
    }
    return labels[type] ?? '高级条件';
  }

  function ruleSetSourceLabel(ruleSetSource: EffectiveRuleSetOption['source']): string {
    return ({
      builtin: '内置',
      subscription: '订阅',
      remote: '外部来源',
      local: '本地',
      profile: '活动配置',
    } as const)[ruleSetSource];
  }

  function limitationLabel(code: string): string {
    return ({
      dns_encrypted_client_queries_not_intercepted: '无法劫持应用自带的加密 DNS 查询',
      dns_ech_hostname_recovery_unavailable: '无法从 ECH 恢复主机名',
      dns_doq_detour_unsupported: 'DoQ 上游暂不支持 detour',
      tun_dns_hijack_unavailable: '当前构建不支持 TUN DNS 劫持',
    } as Record<string, string>)[code] ?? code;
  }

  function changeDispatchConditionType(value: string) {
    if (!isFormConditionType(value)) return;
    dispatchConditionType = value;
    dispatchConditionBase = {};
    dispatchDialogError = '';
  }

  function switchDispatchEditorMode(mode: 'form' | 'json') {
    if (mode === dispatchEditorMode) return;
    dispatchDialogError = '';
    if (mode === 'json') {
      try {
        dispatchConditionDraft = JSON.stringify(buildDispatchConditionFromForm(), null, 2);
        dispatchEditorMode = 'json';
      } catch (cause) {
        dispatchDialogError = cause instanceof Error ? cause.message : '表单条件无效';
      }
      return;
    }
    try {
      const parsed = JSON.parse(dispatchConditionDraft);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('分流条件必须是 JSON 对象');
      if (!loadDispatchConditionForm(parsed as Record<string, unknown>)) {
        throw new Error('该条件包含复合或暂不支持的结构，请继续使用 JSON 模式');
      }
      dispatchEditorMode = 'form';
    } catch (cause) {
      dispatchDialogError = cause instanceof Error ? cause.message : '分流条件不是有效的 JSON 对象';
    }
  }

  function touch() {
    if (draft) draft = JSON.parse(JSON.stringify(draft)) as DnsSettingsDraft;
    saved = false;
    savedPending = false;
    error = '';
    errorCode = undefined;
    nativeError = '';
  }

  function syncNativeJson() {
    if (draft) nativeJson = JSON.stringify(draft.dns, null, 2);
    nativeError = '';
  }

  function openJsonEditor() {
    syncNativeJson();
    jsonDialogOpen = true;
    error = '';
    errorCode = undefined;
  }

  function closeJsonEditor() {
    jsonDialogOpen = false;
    nativeError = '';
  }

  function applyNativeJson() {
    if (!draft) return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(nativeJson);
    } catch (cause) {
      nativeError = cause instanceof Error ? cause.message : 'JSON 格式无效';
      return;
    }
    const config = parseDnsConfig(parsed);
    if (!config) {
      nativeError = '必须提供有效的 DNS 对象，至少包含 servers 和 default_server';
      return;
    }
    draft = readDnsSettings({
      enabled: draft.mode !== 'disabled',
      config,
      dnsHijack: draft.dnsHijack,
    });
    nativeJson = JSON.stringify(draft.dns, null, 2);
    saved = false;
    savedPending = false;
    error = '';
    nativeError = '';
    jsonDialogOpen = false;
  }

  async function load() {
    loading = true;
    error = '';
    errorCode = undefined;
    try {
      const [result, kernelCompatibility, tunStatus, effectiveRuleSets, configGroups] = await Promise.all([
        loadGlobalDnsSettings(),
        getDnsKernelCompatibility(),
        getGuiTunStatus().catch(() => null),
        getEffectiveRuleSetOptions().catch(() => []),
        getConfigPolicyGroups().catch(() => null),
      ]);
      automaticAddressFamilyPolicy = recommendedDnsAddressFamily(
        tunStatus?.ipv4Egress.availability ?? 'unknown',
        tunStatus?.ipv6Egress.availability ?? 'unknown',
      );
      const nextDraft = result.source.config
        ? result.draft
        : {
            ...result.draft,
            dns: createRecommendedDnsConfig(
              result.draft.mode === 'fake_ip' ? 'fake_ip' : 'real',
              { features: kernelCompatibility.features, addressFamily: automaticAddressFamilyPolicy },
            ),
          };
      source = result.source;
      draft = nextDraft;
      compatibility = kernelCompatibility;
      ruleSetOptions = effectiveRuleSets;
      routeTargetsKnown = configGroups !== null;
      const targets = new Map<string, { tag: string; label: string }>();
      targets.set('block', { tag: 'block', label: '阻断' });
      for (const group of configGroups ?? []) {
        targets.set(group.name, { tag: group.name, label: group.name });
      }
      routeTargetOptions = [...targets.values()].sort((left, right) => left.tag.localeCompare(right.tag, 'zh-CN'));
      jsonDialogOpen = false;
      nativeJson = JSON.stringify(result.source.config ?? nextDraft.dns, null, 2);
      nativeError = '';
      saved = false;
      savedPending = false;
    } catch (cause) {
      const info = getAppErrorInfo(cause, '加载 DNS 配置失败');
      errorCode = info.code;
      error = getAppErrorMessage(cause, info.message);
    } finally {
      loading = false;
    }
  }

  function changeMode(mode: DnsMode) {
    if (!draft) return;
    draft = setDnsMode(draft, mode, source?.config ? {} : {
      features: compatibility.features,
      addressFamily: automaticAddressFamilyPolicy,
    });
    saved = false;
    savedPending = false;
    error = '';
  }

  function changeDefaultServer(value: string) {
    if (!draft) return;
    draft.dns.default_server = value;
    touch();
  }

  async function refreshRuleSetReferences() {
    try {
      ruleSetOptions = await getEffectiveRuleSetOptions();
    } catch {
      // Keep the last known options visible; save still performs kernel validation.
    }
  }

  function resetToAutomaticDefault() {
    if (!draft) return;
    const resetMode = draft.mode === 'fake_ip' ? 'fake_ip' : 'real';
    draft.dns = createRecommendedDnsConfig(resetMode, {
      features: compatibility.features,
      addressFamily: automaticAddressFamilyPolicy,
    });
    if (draft.mode === 'fake_ip') {
      draft.dnsHijack = compatibility.features?.tunDnsSystemAuto.state !== 'unsupported';
    }
    touch();
  }

  function disableDnsHijackForCompatibility() {
    if (!draft) return;
    draft.dnsHijack = false;
    touch();
  }

  async function openEffectiveConfig() {
    if (!draft) return;
    effectiveDialogOpen = true;
    effectiveLoading = true;
    effectiveError = '';
    effectiveInspection = null;
    try {
      const input = JSON.parse(JSON.stringify({
        enabled: draft.mode !== 'disabled',
        config: draft.dns,
        dnsHijack: draft.mode !== 'disabled' && draft.dnsHijack,
      })) as DnsSettingsInput;
      effectiveInspection = await guiInspectDnsEffectiveConfig(input);
    } catch (cause) {
      effectiveError = getAppErrorMessage(cause, '生成最终有效配置失败');
    } finally {
      effectiveLoading = false;
    }
  }

  function changeAddressFamilyPolicy(value: string) {
    if (!draft) return;
    const option = addressFamilyOptions.find((candidate) => candidate.value === value);
    if (!option) return;
    draft.dns = setDnsAddressFamilyPolicy(draft.dns, option.value);
    touch();
  }

  function ensurePolicy(): DnsPolicyConfig | null {
    if (!draft) return null;
    draft.dns.policy ??= {};
    return draft.dns.policy;
  }

  function changePolicyTimeout(value: string) {
    const policy = ensurePolicy();
    if (!policy) return;
    policy.timeout_ms = value ? Number(value) : undefined;
    touch();
  }

  function changeServerTimeout(name: string, value: string) {
    const policy = ensurePolicy();
    if (!policy) return;
    const timeouts = { ...(policy.server_timeout_ms ?? {}) };
    if (value) timeouts[name] = Number(value);
    else delete timeouts[name];
    policy.server_timeout_ms = Object.keys(timeouts).length > 0 ? timeouts : undefined;
    touch();
  }

  function changePolicyServer(field: PolicyServerField, value: string) {
    const policy = ensurePolicy();
    if (!policy) return;
    policy[field] = value === unsetSelection ? undefined : value;
    touch();
  }

  function policyPrimaryFor(field: PolicyFallbackField): string | undefined {
    if (!draft?.dns.policy) return undefined;
    if (field === 'fallback_servers') return draft.dns.default_server;
    if (field === 'node_fallback_servers') return draft.dns.policy.node_server;
    if (field === 'direct_fallback_servers') return draft.dns.policy.direct_server;
    return undefined;
  }

  function addPolicyFallback(field: PolicyFallbackField) {
    const policy = ensurePolicy();
    if (!policy) return;
    const current = policy[field] ?? [];
    const primary = policyPrimaryFor(field);
    const options = field === 'node_fallback_servers' ? directNodeServerNames : serverNames;
    const candidate = options.find((name) => name !== primary && !current.includes(name));
    if (!candidate) {
      error = '没有可添加的 DNS 回退服务器';
      return;
    }
    policy[field] = [...current, candidate];
    touch();
  }

  function changePolicyFallback(field: PolicyFallbackField, index: number, value: string) {
    const policy = ensurePolicy();
    if (!policy) return;
    const values = [...(policy[field] ?? [])];
    values[index] = value;
    policy[field] = values;
    touch();
  }

  function movePolicyFallback(field: PolicyFallbackField, index: number, direction: -1 | 1) {
    const policy = ensurePolicy();
    if (!policy) return;
    const values = [...(policy[field] ?? [])];
    const target = index + direction;
    if (target < 0 || target >= values.length) return;
    [values[index], values[target]] = [values[target], values[index]];
    policy[field] = values;
    touch();
  }

  function removePolicyFallback(field: PolicyFallbackField, index: number) {
    const policy = ensurePolicy();
    if (!policy) return;
    policy[field] = (policy[field] ?? []).filter((_, current) => current !== index);
    touch();
  }

  function changeRejectedCidrs(value: string) {
    const policy = ensurePolicy();
    if (!policy) return;
    policy.reject_address_cidrs = value.split('\n').map((item) => item.trim()).filter(Boolean);
    touch();
  }

  function toggleReverseMapping(enabled: boolean) {
    if (!draft) return;
    draft.dns.reverse_mapping = enabled
      ? draft.dns.reverse_mapping ?? {
          max_entries: 1024,
          max_domains_per_address: 8,
          max_ttl_seconds: 300,
        }
      : undefined;
    touch();
  }

  function changeReverseMapping(field: ReverseMappingField, value: string) {
    if (!draft?.dns.reverse_mapping) return;
    draft.dns.reverse_mapping[field] = Number(value);
    touch();
  }

  function changeServerDraftType(type: DnsServerType) {
    const previous = serverDraft;
    const next = createDnsServer(type);
    if (type !== 'system' && previous.type !== 'system') {
      next.host = previous.host;
      next.bootstrap = previous.bootstrap;
      next.server_name = previous.server_name;
      if (type !== 'doq') next.detour = previous.detour;
    }
    serverDraft = next;
  }

  function nextServerName() {
    if (!draft) return;
    let index = 1;
    let name = 'server';
    while (Object.hasOwn(draft.dns.servers, name)) name = `server-${++index}`;
    return name;
  }

  function openAddServer() {
    serverDialogError = '';
    editingServerName = null;
    serverNameDraft = nextServerName() ?? 'server';
    serverDraft = createDnsServer('udp');
    serverDialogOpen = true;
  }

  function openEditServer(name: string) {
    if (!draft) return;
    serverDialogError = '';
    editingServerName = name;
    serverNameDraft = name;
    serverDraft = cloneDnsValue(draft.dns.servers[name]);
    serverDialogOpen = true;
  }

  function closeServerDialog() {
    serverDialogOpen = false;
    serverDialogError = '';
  }

  function saveServerDialog() {
    if (!draft) return;
    const nextName = serverNameDraft.trim();
    if (!nextName) {
      serverDialogError = '请输入服务器名称';
      return;
    }
    if (nextName !== editingServerName && Object.hasOwn(draft.dns.servers, nextName)) {
      serverDialogError = `服务器“${nextName}”已存在`;
      return;
    }
    if (serverDraft.type !== 'system' && !serverDraft.host?.trim()) {
      serverDialogError = '请输入服务器 Host';
      return;
    }
    if (serverDraft.type === 'system' || serverDraft.type === 'doq') delete serverDraft.detour;
    else if (serverDraft.detour) serverDraft.detour = serverDraft.detour.trim() || undefined;

    try {
      if (editingServerName && nextName !== editingServerName) {
        draft.dns = renameDnsServer(draft.dns, editingServerName, nextName);
      }
      draft.dns.servers[nextName] = cloneDnsValue(serverDraft);
      if (!draft.dns.default_server) draft.dns.default_server = nextName;
      touch();
      serverDialogOpen = false;
      editingServerName = null;
    } catch (cause) {
      serverDialogError = getAppErrorMessage(cause, '保存服务器失败');
    }
  }

  function describeServer(server: DnsServerConfig) {
    if (server.type === 'system') return '使用操作系统解析器';
    const endpoint = `${server.host || '未设置 Host'}${server.port ? `:${server.port}` : ''}`;
    const address = server.type === 'doh' ? `${endpoint}${server.path || '/dns-query'}` : endpoint;
    return server.detour ? `${address} · 经 ${server.detour}` : address;
  }

  function removeServer(name: string) {
    if (!draft) return;
    if (Object.keys(draft.dns.servers).length === 1) {
      error = '至少保留一个 DNS 服务器';
      return;
    }
    if (draft.dns.dispatch.some((rule) => rule.server === name)) {
      error = `服务器“${name}”仍被分流规则引用，请先修改或删除对应规则`;
      return;
    }
    const policy = draft.dns.policy;
    const policyReferences = [
      policy?.node_server,
      policy?.direct_server,
      ...(policy?.fallback_servers ?? []),
      ...(policy?.node_fallback_servers ?? []),
      ...(policy?.direct_fallback_servers ?? []),
      ...(policy?.server_timeout_ms && Object.hasOwn(policy.server_timeout_ms, name) ? [name] : []),
    ];
    if (policyReferences.includes(name)) {
      error = `服务器“${name}”仍被解析策略引用，请先调整主服务器、回退链或单独超时`;
      return;
    }
    delete draft.dns.servers[name];
    if (draft.dns.default_server === name) {
      draft.dns.default_server = Object.keys(draft.dns.servers)[0] ?? '';
    }
    touch();
  }

  function openAddDispatch() {
    if (!draft) return;
    editingDispatchIndex = null;
    const condition = { type: 'domain', values: ['example.com'] };
    loadDispatchConditionForm(condition);
    dispatchConditionDraft = JSON.stringify(condition, null, 2);
    dispatchEditorMode = 'form';
    dispatchServerDraft = draft.dns.default_server || serverNames[0] || '';
    dispatchDialogError = '';
    dispatchDialogOpen = true;
  }

  function openEditDispatch(index: number) {
    if (!draft) return;
    const rule = draft.dns.dispatch[index];
    if (!rule) return;
    editingDispatchIndex = index;
    dispatchConditionDraft = JSON.stringify(rule.condition, null, 2);
    dispatchEditorMode = loadDispatchConditionForm(rule.condition) ? 'form' : 'json';
    dispatchServerDraft = rule.server;
    dispatchDialogError = '';
    dispatchDialogOpen = true;
  }

  function closeDispatchDialog() {
    dispatchDialogOpen = false;
    dispatchDialogError = '';
  }

  function saveDispatchDialog() {
    if (!draft) return;
    try {
      const condition = dispatchEditorMode === 'form'
        ? buildDispatchConditionFromForm()
        : JSON.parse(dispatchConditionDraft);
      if (!condition || typeof condition !== 'object' || Array.isArray(condition)) {
        throw new Error('分流条件必须是有效的 JSON 对象');
      }
      if (!dispatchServerDraft || !Object.hasOwn(draft.dns.servers, dispatchServerDraft)) {
        dispatchDialogError = '请选择有效的 DNS 服务器';
        return;
      }
      const previous = editingDispatchIndex === null
        ? {}
        : cloneDnsValue(draft.dns.dispatch[editingDispatchIndex] ?? {});
      const nextRule: DnsDispatchConfig = {
        ...previous,
        condition: condition as Record<string, unknown>,
        server: dispatchServerDraft,
      };
      if (editingDispatchIndex === null) {
        draft.dns.dispatch.push(nextRule);
      } else {
        draft.dns.dispatch[editingDispatchIndex] = nextRule;
      }
      touch();
      dispatchDialogOpen = false;
      editingDispatchIndex = null;
    } catch (cause) {
      dispatchDialogError = cause instanceof Error ? cause.message : '分流条件必须是有效的 JSON 对象';
    }
  }

  function moveDispatch(index: number, direction: -1 | 1) {
    if (!draft) return;
    const target = index + direction;
    if (target < 0 || target >= draft.dns.dispatch.length) return;
    const rules = [...draft.dns.dispatch];
    [rules[index], rules[target]] = [rules[target], rules[index]];
    draft.dns.dispatch = rules;
    touch();
  }

  function removeDispatch(index: number) {
    if (!draft) return;
    draft.dns.dispatch = draft.dns.dispatch.filter((_, current) => current !== index);
    touch();
  }

  async function save() {
    if (!draft || !source || saving) return;
    saving = true;
    saved = false;
    savedPending = false;
    error = '';
    errorCode = undefined;
    nativeError = '';
    try {
      const nextDraft = draft;
      if (nextDraft.mode === 'fake_ip' && compatibility.status === 'unsupported') {
        source = await persistGlobalDnsSettings(source, nextDraft);
        error = '';
        savedPending = true;
      } else {
        source = await applyGlobalDnsSettings(source, nextDraft);
      }
      draft = nextDraft;
      nativeJson = JSON.stringify(nextDraft.dns, null, 2);
      saved = true;
    } catch (cause) {
      const info = getAppErrorInfo(cause, '保存 DNS 配置失败，已保留上次可用配置');
      errorCode = info.code;
      error = getAppErrorMessage(cause, info.message);
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void load();
    return ruleSetSignal.onChanged(() => void refreshRuleSetReferences());
  });
</script>

<div class="panel-head">
  <div>
    <h2>域名解析</h2>
    <p>选择解析模式、DNS 服务和分流规则。</p>
  </div>
  <div class="head-actions">
    <Button variant="outline" size="sm" onclick={resetToAutomaticDefault} disabled={loading || saving || !draft}>
      <RotateCcw />恢复自动默认
    </Button>
    <Button variant="ghost" size="icon-sm" onclick={load} disabled={loading || saving} aria-label="重新加载 DNS 配置">
      <RefreshCw class={loading ? 'spin' : ''} />
    </Button>
  </div>
</div>

{#if loading}
  <div class="state">加载配置中…</div>
{:else if error && !draft}
  <div class="load-error" role="alert">
    <AlertTriangle />
    <span>{error}</span>
    <ErrorRecoveryActions code={errorCode} context="dns" onretry={load} />
  </div>
{:else if draft}
  {#if compatibility.status === 'unsupported' && draft.mode === 'fake_ip'}
    <div class="issues warning" role="status">
      <div>当前内核未声明 DNS 与 Fake-IP 能力。配置仍会保存到客户端，升级内核后重新点击“保存并应用”即可生效。</div>
      {#if compatibility.engineVersion || compatibility.apiVersion}<small>内核 {compatibility.engineVersion ?? '未知版本'} · API {compatibility.apiVersion ?? '未知'}</small>{/if}
    </div>
  {:else if compatibility.status === 'unknown' && draft.mode === 'fake_ip'}
    <div class="issues warning" role="status">无法确认当前内核版本的 Fake-IP 能力，保存时会继续尝试兼容校验；若内核拒绝，配置不会被覆盖。</div>
  {/if}

  <section class="section mode-section">
    <div class="mode-copy">
      <div class="section-title">基础模式</div>
      <p>{modeDescription}</p>
    </div>
    <SegmentedControl.Root value={draft.mode} onValueChange={(value) => changeMode(value as DnsMode)} aria-label="DNS 基础模式">
      {#each [
        ['disabled', '关闭'],
        ['real', 'Real DNS'],
        ['fake_ip', 'Fake-IP'],
      ] as item}
        <SegmentedControl.Item value={item[0]}>{item[1]}</SegmentedControl.Item>
      {/each}
    </SegmentedControl.Root>
  </section>

  {#if draft.mode === 'disabled'}
    <div class="disabled-note">DNS 配置已停用，当前设置会保留。</div>
  {:else}
    <section class="section row-section">
      <div><strong>DNS 劫持</strong><span>让 TUN 模式统一处理 DNS 请求。</span></div>
      <Switch checked={draft.dnsHijack} onCheckedChange={(checked) => { if (draft) { draft.dnsHijack = checked; touch(); } }} disabled={compatibility.features?.tunDnsHijack.state === 'unsupported'} aria-label="DNS 劫持" />
    </section>
  {/if}

  {#if draft.dnsHijack && usesSystemDns && compatibility.features?.tunDnsSystemAuto.state === 'unsupported'}
    <div class="issues error" role="alert">
      <div>当前内核无法在 TUN DNS 劫持时自动排除 system DNS，保存已被阻止。</div>
      <div class="inline-actions">
        <Button variant="outline" size="sm" onclick={disableDnsHijackForCompatibility}>关闭 DNS 劫持</Button>
        <Button variant="outline" size="sm" onclick={() => store.openSettings('core')}>升级内核</Button>
      </div>
    </div>
  {/if}

  {#if danglingRuleTags.length > 0}
    <div class="issues error" role="alert">
      <div>以下 DNS 分流引用已失效：{danglingRuleTags.join('、')}。请选择现有规则集或删除对应分流。</div>
      <div class="inline-actions">
        <Button variant="outline" size="sm" onclick={() => (store.activeTab = 'rules')}>管理规则集</Button>
        <Button variant="ghost" size="sm" onclick={load}>刷新引用</Button>
      </div>
    </div>
  {/if}

  <section class="section row-section">
    <div>
      <strong>DNS 应答地址族</strong>
      <span>{addressFamilyDescription}</span>
    </div>
    <Select.Root
      type="single"
      value={addressFamilyPolicy}
      disabled={compatibility.features?.dnsAddressFamilyPolicy.state === 'unsupported'}
      onValueChange={(value) => { if (value) changeAddressFamilyPolicy(value); }}
    >
      <Select.Trigger aria-label="DNS 应答地址族策略">
        {addressFamilyOptions.find((option) => option.value === addressFamilyPolicy)?.label ?? addressFamilyPolicy}
      </Select.Trigger>
      <Select.Content>
        {#each addressFamilyOptions as option}
          <Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  </section>

  <section class="advanced-toggle">
    <button data-slot="surface-button" type="button" aria-expanded={advancedOpen} onclick={() => (advancedOpen = !advancedOpen)}>
      <span><strong>高级选项</strong><small>解析链、缓存与 JSON</small></span>
      <ChevronDown class={advancedOpen ? 'expanded' : ''} />
    </button>
    {#if advancedOpen}
      <div class="advanced-actions">
        <Button variant="ghost" size="sm" onclick={openEffectiveConfig}><FileDiff />最终配置</Button>
        <Button variant="ghost" size="sm" onclick={openJsonEditor}><Braces />JSON</Button>
      </div>
    {/if}
  </section>

  {#if advancedOpen}
    <section class="section policy-section">
    <div class="section-head">
      <div>
        <div class="section-title">解析策略</div>
        <p>配置查询超时、失败回退，以及节点连接与直连目标使用的独立 DNS 链路。</p>
      </div>
    </div>
    {#if hasDnsDetour}
      <div class="policy-note"><AlertTriangle />检测到 DNS 上游通过出站转发。节点解析链必须使用不带 detour 的服务器，避免解析代理节点时形成递归。</div>
    {/if}
    <div class="field-grid policy-grid">
      <label>
        <span>默认查询超时（毫秒）</span>
        <Input type="number" min="1" max="120000" value={draft.dns.policy?.timeout_ms ?? 5000} oninput={(event) => changePolicyTimeout(event.currentTarget.value)} />
      </label>
      <label class="wide">
        <span>拒绝响应地址（每行一个 CIDR）</span>
        <Textarea class="font-mono" value={(draft.dns.policy?.reject_address_cidrs ?? []).join('\n')} placeholder="例如 0.0.0.0/32" oninput={(event) => changeRejectedCidrs(event.currentTarget.value)}></Textarea>
        <small>仅校验真实 DNS 上游响应：命中后不缓存并继续回退。它不会排除 Fake-IP，也不会绕过系统代理或 TUN；内网域名请使用 Fake-IP“排除域名”，内网网段请使用“TUN 排除网段”。</small>
      </label>
    </div>

    <div class="policy-block">
      <div class="policy-block-head">
        <div><strong>通用回退链</strong><span>默认或分流选中的服务器失败后，按顺序尝试。</span></div>
        <Button variant="outline" size="sm" onclick={() => addPolicyFallback('fallback_servers')}><Plus />添加</Button>
      </div>
      <div class="fallback-list">
        {#each draft.dns.policy?.fallback_servers ?? [] as server, index (`fallback-${index}`)}
          <div class="fallback-row">
            <span class="fallback-order">#{index + 1}</span>
            <Select.Root type="single" value={server} onValueChange={(value) => { if (value) changePolicyFallback('fallback_servers', index, value); }}>
              <Select.Trigger aria-label={`通用回退服务器 ${index + 1}`}>{server}</Select.Trigger>
              <Select.Content>{#each serverNames as name}<Select.Item value={name} label={name}>{name}</Select.Item>{/each}</Select.Content>
            </Select.Root>
            <Button variant="ghost" size="icon-xs" onclick={() => movePolicyFallback('fallback_servers', index, -1)} disabled={index === 0}><ChevronUp /></Button>
            <Button variant="ghost" size="icon-xs" onclick={() => movePolicyFallback('fallback_servers', index, 1)} disabled={index === (draft.dns.policy?.fallback_servers?.length ?? 0) - 1}><ChevronDown /></Button>
            <Button variant="ghost" size="icon-xs" onclick={() => removePolicyFallback('fallback_servers', index)} aria-label="删除通用回退服务器"><Trash2 /></Button>
          </div>
        {/each}
        {#if (draft.dns.policy?.fallback_servers?.length ?? 0) === 0}<div class="empty compact">未配置通用回退。</div>{/if}
      </div>
    </div>

    <div class="policy-role-grid">
      <div class="policy-block">
        <div class="policy-block-head">
          <div><strong>节点解析</strong><span>代理节点和 QUIC 载体域名；使用 detour 时必须指定。</span></div>
          <Button variant="outline" size="sm" onclick={() => addPolicyFallback('node_fallback_servers')} disabled={!draft.dns.policy?.node_server}><Plus />回退</Button>
        </div>
        <label class="policy-primary">
          <span>主服务器</span>
          <Select.Root type="single" value={draft.dns.policy?.node_server ?? unsetSelection} onValueChange={(value) => { if (value) changePolicyServer('node_server', value); }}>
            <Select.Trigger aria-label="节点解析服务器">{draft.dns.policy?.node_server ?? '沿用默认分流'}</Select.Trigger>
            <Select.Content>
              <Select.Item value={unsetSelection} label="沿用默认分流">沿用默认分流</Select.Item>
              {#each directNodeServerNames as name}<Select.Item value={name} label={name}>{name}</Select.Item>{/each}
            </Select.Content>
          </Select.Root>
        </label>
        <div class="fallback-list">
          {#each draft.dns.policy?.node_fallback_servers ?? [] as server, index (`node-${index}`)}
            <div class="fallback-row">
              <span class="fallback-order">#{index + 1}</span>
              <Select.Root type="single" value={server} onValueChange={(value) => { if (value) changePolicyFallback('node_fallback_servers', index, value); }}>
                <Select.Trigger aria-label={`节点回退服务器 ${index + 1}`}>{server}</Select.Trigger>
                <Select.Content>{#each directNodeServerNames as name}<Select.Item value={name} label={name}>{name}</Select.Item>{/each}</Select.Content>
              </Select.Root>
              <Button variant="ghost" size="icon-xs" onclick={() => movePolicyFallback('node_fallback_servers', index, -1)} disabled={index === 0}><ChevronUp /></Button>
              <Button variant="ghost" size="icon-xs" onclick={() => movePolicyFallback('node_fallback_servers', index, 1)} disabled={index === (draft.dns.policy?.node_fallback_servers?.length ?? 0) - 1}><ChevronDown /></Button>
              <Button variant="ghost" size="icon-xs" onclick={() => removePolicyFallback('node_fallback_servers', index)} aria-label="删除节点回退服务器"><Trash2 /></Button>
            </div>
          {/each}
        </div>
      </div>

      <div class="policy-block">
        <div class="policy-block-head">
          <div><strong>直连解析</strong><span>恢复可信域名后，为 direct 出站重新选择可用地址。</span></div>
          <Button variant="outline" size="sm" onclick={() => addPolicyFallback('direct_fallback_servers')} disabled={!draft.dns.policy?.direct_server}><Plus />回退</Button>
        </div>
        <label class="policy-primary">
          <span>主服务器</span>
          <Select.Root type="single" value={draft.dns.policy?.direct_server ?? unsetSelection} onValueChange={(value) => { if (value) changePolicyServer('direct_server', value); }}>
            <Select.Trigger aria-label="直连解析服务器">{draft.dns.policy?.direct_server ?? '沿用默认分流'}</Select.Trigger>
            <Select.Content>
              <Select.Item value={unsetSelection} label="沿用默认分流">沿用默认分流</Select.Item>
              {#each serverNames as name}<Select.Item value={name} label={name}>{name}</Select.Item>{/each}
            </Select.Content>
          </Select.Root>
        </label>
        <div class="fallback-list">
          {#each draft.dns.policy?.direct_fallback_servers ?? [] as server, index (`direct-${index}`)}
            <div class="fallback-row">
              <span class="fallback-order">#{index + 1}</span>
              <Select.Root type="single" value={server} onValueChange={(value) => { if (value) changePolicyFallback('direct_fallback_servers', index, value); }}>
                <Select.Trigger aria-label={`直连回退服务器 ${index + 1}`}>{server}</Select.Trigger>
                <Select.Content>{#each serverNames as name}<Select.Item value={name} label={name}>{name}</Select.Item>{/each}</Select.Content>
              </Select.Root>
              <Button variant="ghost" size="icon-xs" onclick={() => movePolicyFallback('direct_fallback_servers', index, -1)} disabled={index === 0}><ChevronUp /></Button>
              <Button variant="ghost" size="icon-xs" onclick={() => movePolicyFallback('direct_fallback_servers', index, 1)} disabled={index === (draft.dns.policy?.direct_fallback_servers?.length ?? 0) - 1}><ChevronDown /></Button>
              <Button variant="ghost" size="icon-xs" onclick={() => removePolicyFallback('direct_fallback_servers', index)} aria-label="删除直连回退服务器"><Trash2 /></Button>
            </div>
          {/each}
        </div>
      </div>
    </div>

    <div class="policy-block timeout-block">
      <div class="policy-block-head"><div><strong>单独超时</strong><span>留空时使用默认查询超时。</span></div></div>
      <div class="timeout-grid">
        {#each serverNames as name}
          <label><span>{name}</span><Input type="number" min="1" max="120000" value={draft.dns.policy?.server_timeout_ms?.[name] ?? ''} placeholder={`${draft.dns.policy?.timeout_ms ?? 5000}`} oninput={(event) => changeServerTimeout(name, event.currentTarget.value)} /></label>
        {/each}
      </div>
    </div>
    </section>
  {/if}

  <section class="section">
      <div class="section-head">
        <div><div class="section-title">DNS 服务器</div><p>选择默认解析服务，或添加自定义服务器。</p></div>
        <Button variant="outline" size="sm" onclick={openAddServer}><Plus />新增</Button>
      </div>
      <div class="server-list">
        {#each serverNames as name (name)}
          {@const server = draft.dns.servers[name]}
          <article class="server-row">
            <div class="server-icon"><Server /></div>
            <div class="server-summary">
              <div class="server-name">
                <strong>{name}</strong>
                <span class="server-type">{serverTypeOptions.find((option) => option.value === server.type)?.label ?? server.type}</span>
                {#if draft.dns.default_server === name}<span class="default-badge">默认</span>{/if}
              </div>
              <span>{describeServer(server)}</span>
            </div>
            <div class="server-actions">
              <Button variant="ghost" size="icon-sm" onclick={() => openEditServer(name)} aria-label={`编辑 ${name}`}><Pencil /></Button>
              <Button variant="ghost" size="icon-sm" onclick={() => removeServer(name)} aria-label={`删除 ${name}`}><Trash2 /></Button>
            </div>
          </article>
        {/each}
      </div>
      <div class="default-row">
        <span>默认服务器</span>
        <Select.Root
          type="single"
          value={draft.dns.default_server}
          onValueChange={(value) => { if (value) changeDefaultServer(value); }}
        >
          <Select.Trigger aria-label="默认 DNS 服务器">{draft.dns.default_server}</Select.Trigger>
          <Select.Content>
            {#each serverNames as name}
              <Select.Item value={name} label={name}>{name}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
  </section>

  {#if advancedOpen}
    <section class="section">
        <div class="section-head"><div><div class="section-title">DNS 缓存</div><p>控制内核缓存的容量和最长保留时间。</p></div></div>
        <div class="field-grid">
          <label><span>最大缓存条目</span><Input type="number" value={draft.dns.cache?.max_entries ?? 1024} oninput={(event) => { if (draft) { draft.dns.cache = { ...draft.dns.cache, max_entries: Number(event.currentTarget.value) }; touch(); } }} /></label>
          <label><span>最大 TTL（秒，可选）</span><Input type="number" value={draft.dns.cache?.max_ttl_seconds ?? ''} oninput={(event) => { if (draft?.dns.cache) { draft.dns.cache.max_ttl_seconds = event.currentTarget.value ? Number(event.currentTarget.value) : undefined; touch(); } }} /></label>
        </div>
    </section>

    <section class="section">
        <div class="section-head">
          <div><div class="section-title">真实地址映射</div><p>为透明连接保留 IP 与域名的对应关系。</p></div>
          <Switch checked={Boolean(draft.dns.reverse_mapping)} disabled={compatibility.features?.dnsRealReverseMapping.state === 'unsupported'} onCheckedChange={toggleReverseMapping} aria-label="真实地址映射" />
        </div>
        {#if draft.dns.reverse_mapping}
          <div class="field-grid reverse-grid">
            <label><span>最大地址数</span><Input type="number" min="1" value={draft.dns.reverse_mapping.max_entries} oninput={(event) => changeReverseMapping('max_entries', event.currentTarget.value)} /></label>
            <label><span>每个地址的候选域名数</span><Input type="number" min="2" value={draft.dns.reverse_mapping.max_domains_per_address} oninput={(event) => changeReverseMapping('max_domains_per_address', event.currentTarget.value)} /></label>
            <label><span>最长保留时间（秒）</span><Input type="number" min="1" value={draft.dns.reverse_mapping.max_ttl_seconds} oninput={(event) => changeReverseMapping('max_ttl_seconds', event.currentTarget.value)} /></label>
          </div>
        {/if}
    </section>
  {/if}

      {#if draft.mode === 'fake_ip' && draft.dns.answer.type === 'fake_ip'}
        <section class="section">
          <div class="section-title">Fake-IP</div>
          <div class="field-grid">
            <label><span>IPv4 CIDR</span><Input bind:value={draft.dns.answer.cidr} oninput={touch} /></label>
            <label><span>IPv6 CIDR <small>可选，启用 AAAA 合成</small></span><Input value={draft.dns.answer.ipv6_cidr ?? ''} placeholder="fd00::/96" disabled={compatibility.features?.dnsFakeIpDualStack.state === 'unsupported'} oninput={(event) => { if (draft?.dns.answer.type === 'fake_ip') { draft.dns.answer.ipv6_cidr = event.currentTarget.value.trim() || undefined; touch(); } }} /></label>
            <label><span>TTL（秒）</span><Input type="number" bind:value={draft.dns.answer.ttl_seconds} oninput={touch} /></label>
            <label><span>最大映射数（可选）</span><Input type="number" value={draft.dns.answer.max_entries ?? ''} oninput={(event) => { if (draft?.dns.answer.type === 'fake_ip') { draft.dns.answer.max_entries = event.currentTarget.value ? Number(event.currentTarget.value) : undefined; touch(); } }} /></label>
            <label class="wide"><span>排除域名（每行一个）</span><Textarea class="font-mono" value={(draft.dns.answer.exclude_domains ?? []).join('\n')} oninput={(event) => { if (draft?.dns.answer.type === 'fake_ip') { draft.dns.answer.exclude_domains = event.currentTarget.value.split('\n').map((value) => value.trim()).filter(Boolean); touch(); } }}></Textarea><small>这些域名返回真实 DNS 结果，不分配 Fake-IP。按目标网段绕过 TUN 请到 TUN 设置配置排除 CIDR。</small></label>
          </div>
        </section>
      {/if}

      <section class="section">
        <div class="section-head"><div><div class="section-title">解析分流</div><p>按顺序匹配域名或规则集。</p></div><Button variant="outline" size="sm" onclick={openAddDispatch} disabled={compatibility.features?.dnsSplitDispatch.state === 'unsupported'}><Plus />新增</Button></div>
        <div class="dispatch-list">
          {#each draft.dns.dispatch as rule, index (index)}
            <article class="dispatch-card">
              <div class="dispatch-order"><span>#{index + 1}</span><Button variant="ghost" size="icon-xs" onclick={() => moveDispatch(index, -1)} disabled={index === 0}><ChevronUp /></Button><Button variant="ghost" size="icon-xs" onclick={() => moveDispatch(index, 1)} disabled={index === draft.dns.dispatch.length - 1}><ChevronDown /></Button></div>
              <div class="dispatch-summary">
                <strong>{dispatchConditionSummary(rule.condition)}</strong>
                <span>使用 {rule.server}</span>
              </div>
              <div class="dispatch-actions">
                <Button variant="ghost" size="icon-sm" onclick={() => openEditDispatch(index)} aria-label={`编辑第 ${index + 1} 条分流规则`}><Pencil /></Button>
                <Button variant="ghost" size="icon-sm" onclick={() => removeDispatch(index)} aria-label="删除分流规则"><Trash2 /></Button>
              </div>
            </article>
          {/each}
          {#if draft.dns.dispatch.length === 0}<div class="empty">没有分流规则，所有查询使用默认服务器。</div>{/if}
        </div>
      </section>

  {#if compatibility.limitations?.length}
    <div class="boundary"><AlertTriangle /><span>{compatibility.limitations.map(limitationLabel).join('；')}</span></div>
  {/if}
  {#if warnings.length}<div class="issues warning">{#each warnings as issue}<div>{issue.message}</div>{/each}</div>{/if}
  {#if errors.length}<div class="issues error">{#each errors as issue}<div>{issue.field}：{issue.message}</div>{/each}</div>{/if}
  {#if error}
    <div class="issues error" role="alert">
      <div>{error}</div>
      <ErrorRecoveryActions code={errorCode} context="dns" onretry={save} />
    </div>
  {/if}
  <div class="actions"><Button onclick={save} disabled={saving || errors.length > 0}><Save />{saving ? '保存并应用中…' : saved ? savedPending ? '已保存，待内核' : '已保存并应用' : '保存并应用'}</Button></div>
{/if}

<Dialog.Root bind:open={serverDialogOpen}>
  <Dialog.Content class="sm:max-w-[560px]">
    <form
      class="server-dialog-form"
      onsubmit={(event) => {
        event.preventDefault();
        saveServerDialog();
      }}
    >
      <Dialog.Header>
        <Dialog.Title>{editingServerName ? '编辑 DNS 服务器' : '新增 DNS 服务器'}</Dialog.Title>
        <Dialog.Description>配置 DNS 地址和连接方式。</Dialog.Description>
      </Dialog.Header>
      <Dialog.Body class="server-dialog-body">
        <div class="dialog-field-grid">
          <label>
            <span>服务器名称</span>
            <Input bind:value={serverNameDraft} placeholder="例如 cloudflare" autofocus />
          </label>
          <label>
            <span>协议</span>
            <Select.Root
              type="single"
              value={serverDraft.type}
              onValueChange={(value) => { if (value) changeServerDraftType(value as DnsServerType); }}
            >
              <Select.Trigger class="w-full" aria-label="DNS 服务器协议">
                {serverTypeOptions.find((option) => option.value === serverDraft.type)?.label ?? serverDraft.type}
              </Select.Trigger>
              <Select.Content>
                {#each serverTypeOptions as option}
                  <Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </label>
        </div>

        {#if serverDraft.type === 'system'}
          <div class="system-note">system 使用操作系统 DNS，可作为公共 DNS 不可用时的降级选项。</div>
        {:else}
          <div class="dialog-field-grid">
            <label>
              <span>Host</span>
              <Input
                value={serverDraft.host ?? ''}
                oninput={(event) => { serverDraft = { ...serverDraft, host: event.currentTarget.value }; }}
                placeholder="1.1.1.1 或 dns.example"
              />
            </label>
            <label>
              <span>端口</span>
              <Input
                type="number"
                value={serverDraft.port ?? ''}
                oninput={(event) => { serverDraft = { ...serverDraft, port: event.currentTarget.value ? Number(event.currentTarget.value) : undefined }; }}
              />
            </label>
            {#if serverDraft.type === 'doh'}
              <label>
                <span>Path</span>
                <Input
                  value={serverDraft.path ?? '/dns-query'}
                  oninput={(event) => { serverDraft = { ...serverDraft, path: event.currentTarget.value }; }}
                  placeholder="/dns-query"
                />
              </label>
            {/if}
            {#if serverDraft.type !== 'udp'}
              <label>
                <span>Server Name <small>可选</small></span>
                <Input
                  value={serverDraft.server_name ?? ''}
                  oninput={(event) => { serverDraft = { ...serverDraft, server_name: event.currentTarget.value || undefined }; }}
                  placeholder="TLS 服务器名称"
                />
              </label>
            {/if}
            <label class="wide">
              <span>Bootstrap IP <small>逗号分隔</small></span>
              <Input
                value={(serverDraft.bootstrap ?? []).join(', ')}
                oninput={(event) => { serverDraft = { ...serverDraft, bootstrap: event.currentTarget.value.split(',').map((value) => value.trim()).filter(Boolean) }; }}
                placeholder="1.1.1.1, 1.0.0.1"
              />
                <small>Host 为域名时，可填写用于首次连接的 IP。</small>
            </label>
            {#if serverDraft.type !== 'doq'}
              <label class="wide">
              <span>连接方式</span>
                <Select.Root
                  type="single"
                  value={serverDraft.detour ?? unsetSelection}
                  onValueChange={(value) => { if (value) serverDraft = { ...serverDraft, detour: value === unsetSelection ? undefined : value }; }}
                >
                  <Select.Trigger class="w-full" aria-label="DNS 上游经由出站">{dnsDetourLabel(serverDraft.detour)}</Select.Trigger>
                  <Select.Content>
                    <Select.Item value={unsetSelection} label="直接连接">直接连接</Select.Item>
                    <Select.Item value={DNS_DETOUR_ROUTE_FINAL} label="跟随默认出站">跟随默认出站</Select.Item>
                    {#each routeTargetOptions as target}
                      <Select.Item value={target.tag} label={target.tag === 'block' ? target.label : `策略组 · ${target.label}`}>
                        {target.tag === 'block' ? target.label : `策略组 · ${target.label}`}
                      </Select.Item>
                    {/each}
                    {#if serverDraft.detour && serverDraft.detour !== DNS_DETOUR_ROUTE_FINAL && !routeTargetTags.has(serverDraft.detour)}
                      <Select.Item value={serverDraft.detour} label={`${serverDraft.detour}（已失效）`}>{serverDraft.detour}（已失效）</Select.Item>
                    {/if}
                  </Select.Content>
                </Select.Root>
                <small>默认出站会随当前代理配置切换。</small>
              </label>
            {:else}
              <div class="system-note wide">DoQ 暂不支持经由代理连接。</div>
            {/if}
          </div>
        {/if}

        {#if serverDialogError}<div class="dialog-error" role="alert">{serverDialogError}</div>{/if}
      </Dialog.Body>
      <Dialog.Footer>
        <Button type="button" variant="outline" onclick={closeServerDialog}>取消</Button>
        <Button type="submit">{editingServerName ? '保存修改' : '添加服务器'}</Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={dispatchDialogOpen}>
  <Dialog.Content class="sm:max-w-[620px]">
    <form
      class="dispatch-dialog-form"
      onsubmit={(event) => {
        event.preventDefault();
        saveDispatchDialog();
      }}
    >
      <Dialog.Header>
        <Dialog.Title>{editingDispatchIndex === null ? '新增 DNS 分流' : '编辑 DNS 分流'}</Dialog.Title>
        <Dialog.Description>按顺序匹配，命中后使用指定 DNS。</Dialog.Description>
      </Dialog.Header>
      <Dialog.Body class="dispatch-dialog-body">
        <SegmentedControl.Root value={dispatchEditorMode} onValueChange={(value) => switchDispatchEditorMode(value as 'form' | 'json')} aria-label="DNS 分流条件编辑方式">
          <SegmentedControl.Item value="form">表单</SegmentedControl.Item>
          <SegmentedControl.Item value="json">高级 JSON</SegmentedControl.Item>
        </SegmentedControl.Root>
        {#if dispatchEditorMode === 'form'}
          <div class="dispatch-condition-form">
            <label class="dispatch-dialog-field">
              <span>条件类型</span>
              <Select.Root type="single" value={dispatchConditionType} onValueChange={(value) => { if (value) changeDispatchConditionType(value); }}>
                <Select.Trigger aria-label="DNS 分流条件类型">{dispatchConditionOptions.find((option) => option.value === dispatchConditionType)?.label ?? dispatchConditionType}</Select.Trigger>
                <Select.Content>
                  {#each dispatchConditionOptions as option}
                    <Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
                  {/each}
                </Select.Content>
              </Select.Root>
            </label>
            {#if dispatchConditionType === 'rule_set'}
              <label class="dispatch-dialog-field">
                <span>有效规则集</span>
                {#if ruleSetOptions.length > 0}
                  <Select.Root
                    type="single"
                    value={dispatchConditionTagDraft}
                    onValueChange={(value) => { if (value) dispatchConditionTagDraft = value; }}
                  >
                    <Select.Trigger aria-label="DNS 分流规则集">
                      {(ruleSetOptions.find((option) => option.tag === dispatchConditionTagDraft)?.name ?? dispatchConditionTagDraft) || '选择规则集'}
                    </Select.Trigger>
                    <Select.Content>
                      {#each ruleSetOptions as option (option.tag)}
                        <Select.Item value={option.tag} label={`${option.name} · ${ruleSetSourceLabel(option.source)}`}>
                          {option.name} · {ruleSetSourceLabel(option.source)} · {option.tag}
                        </Select.Item>
                      {/each}
                    </Select.Content>
                  </Select.Root>
                  <small>只显示当前可用的规则集。</small>
                  {#if dispatchConditionTagDraft && !ruleSetTags.has(dispatchConditionTagDraft)}
                    <div class="reference-warning">当前引用 {dispatchConditionTagDraft} 已失效，请重新选择。</div>
                  {/if}
                {:else}
                  <div class="empty-reference">
                    <span>当前最终有效配置中没有可选择的规则集。</span>
                    <Button type="button" variant="outline" size="sm" onclick={() => (store.activeTab = 'rules')}>前往规则页</Button>
                  </div>
                {/if}
              </label>
            {:else}
              <label class="dispatch-dialog-field">
                <span>匹配值 <small>每行一个</small></span>
                <Textarea
                  class="font-mono min-h-28"
                  bind:value={dispatchConditionValuesDraft}
                  placeholder={dispatchConditionOptions.find((option) => option.value === dispatchConditionType)?.placeholder ?? ''}
                ></Textarea>
              </label>
            {/if}
          </div>
        {:else}
          <label class="dispatch-dialog-field">
            <span>匹配条件 JSON</span>
            <Textarea class="font-mono min-h-40" bind:value={dispatchConditionDraft} spellcheck="false"></Textarea>
            <small>支持 <code>and</code> / <code>or</code> 复合条件以及新版内核字段。</small>
          </label>
        {/if}
        <label class="dispatch-dialog-field">
          <span>DNS 服务器</span>
          <Select.Root
            type="single"
            value={dispatchServerDraft}
            onValueChange={(value) => { if (value) dispatchServerDraft = value; }}
          >
            <Select.Trigger aria-label="分流 DNS 服务器">{dispatchServerDraft || '选择服务器'}</Select.Trigger>
            <Select.Content>
              {#each serverNames as name}
                <Select.Item value={name} label={name}>{name}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </label>
        {#if dispatchDialogError}<div class="dialog-error" role="alert">{dispatchDialogError}</div>{/if}
      </Dialog.Body>
      <Dialog.Footer>
        <Button type="button" variant="outline" onclick={closeDispatchDialog}>取消</Button>
        <Button type="submit">{editingDispatchIndex === null ? '添加分流' : '保存修改'}</Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={effectiveDialogOpen}>
  <Dialog.Content class="sm:max-w-[1000px]">
    <Dialog.Header>
      <Dialog.Title>最终有效配置解释</Dialog.Title>
      <Dialog.Description>只读预览，不会保存或应用；用于核对基础配置经过客户端覆盖后实际交给当前内核的内容。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Body class="effective-dialog-body">
      {#if effectiveLoading}
        <div class="state">正在组合有效配置…</div>
      {:else if effectiveError}
        <div class="dialog-error" role="alert">{effectiveError}</div>
      {:else if effectiveInspection?.reason === 'no_active_proxy_profile'}
        <div class="state">当前没有活动代理配置，因此暂时无法生成最终有效配置。</div>
      {:else if effectiveInspection}
        <div class="effective-meta">
          <span>基础：{effectiveInspection.activeProfileName ?? '未知配置'}</span>
          {#each effectiveInspection.sources as configSource (configSource.id)}
            <span class:source-enabled={configSource.enabled}>{configSource.label}：{configSource.enabled ? '生效' : '未启用'}{configSource.count !== undefined ? ` · ${configSource.count}` : ''}</span>
          {/each}
        </div>
        <section class="diff-section">
          <div class="section-title">字段差异（{effectiveDiff.length}）</div>
          {#if effectiveDiff.length > 0}
            <div class="diff-list">
              {#each effectiveDiff as item (item.path)}
                <div class="diff-item">
                  <div><code>{item.path}</code><span>{item.source}</span></div>
                  <small title={compactConfigValue(item.before)}>基础：{compactConfigValue(item.before)}</small>
                  <small title={compactConfigValue(item.after)}>最终：{compactConfigValue(item.after)}</small>
                </div>
              {/each}
            </div>
          {:else}
            <div class="state compact">客户端覆盖没有改变当前基础配置。</div>
          {/if}
        </section>
        <div class="config-preview-grid">
          <section><strong>基础配置</strong><pre>{JSON.stringify(effectiveInspection.baseConfig, null, 2)}</pre></section>
          <section><strong>最终有效配置</strong><pre>{JSON.stringify(effectiveInspection.effectiveConfig, null, 2)}</pre></section>
        </div>
      {/if}
    </Dialog.Body>
    <Dialog.Footer>
      <Button type="button" variant="outline" onclick={() => (effectiveDialogOpen = false)}>关闭</Button>
      <Button type="button" onclick={openEffectiveConfig} disabled={effectiveLoading}><RefreshCw />重新生成</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={jsonDialogOpen}>
  <Dialog.Content class="sm:max-w-[760px]">
    <Dialog.Header>
      <Dialog.Title>编辑内核原生 DNS JSON</Dialog.Title>
      <Dialog.Description>应用后会更新表单草稿；仍需在主页面点击“保存并应用”才会提交到内核。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Body class="json-dialog-body">
      <Textarea class="font-mono min-h-[min(54vh,480px)] resize-none" bind:value={nativeJson} spellcheck="false" aria-label="内核原生 DNS JSON 配置"></Textarea>
      {#if nativeError}<div class="dialog-error" role="alert">{nativeError}</div>{/if}
    </Dialog.Body>
    <Dialog.Footer>
      <Button type="button" variant="outline" onclick={closeJsonEditor}>取消</Button>
      <Button type="button" onclick={applyNativeJson}>应用到表单</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .panel-head,
  .section-head,
  .head-actions,
  .mode-section,
  .server-row,
  .server-name,
  .server-actions,
  .dispatch-actions,
  .dispatch-card,
  .row-section,
  .actions,
  .default-row {
    display: flex;
    align-items: center;
  }

  .panel-head,
  .section-head,
  .mode-section,
  .row-section {
    justify-content: space-between;
  }

  .panel-head {
    gap: 12px;
    margin-bottom: 12px;
  }

  .panel-head h2 {
    margin: 0;
    font-size: 16px;
  }

  .panel-head p,
  .section-head p {
    margin: 3px 0 0;
    color: var(--muted-foreground);
    font-size: 11.5px;
  }

  .head-actions {
    flex: none;
    gap: 4px;
  }

  .head-actions :global(svg),
  .server-actions :global(svg),
  .dispatch-actions :global(svg),
  .server-icon :global(svg) {
    width: 14px;
    height: 14px;
  }

  .effective-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 10px;
  }

  .effective-meta span {
    padding: 3px 7px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .effective-meta span.source-enabled {
    border-color: color-mix(in srgb, var(--primary) 35%, var(--border));
    color: var(--primary);
  }

  .inline-actions { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 7px; }

  .reference-warning,
  .empty-reference {
    margin-top: 7px;
    padding: 8px 9px;
    border: 1px solid rgba(239, 68, 68, .3);
    border-radius: 7px;
    color: var(--destructive);
    font-size: 11px;
  }

  .empty-reference { display: flex; align-items: center; justify-content: space-between; gap: 8px; color: var(--muted-foreground); border-color: var(--border); }

  .diff-section { display: flex; flex-direction: column; gap: 7px; }
  .diff-list { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 7px; }
  .diff-item { min-width: 0; padding: 8px 9px; border: 1px solid var(--border); border-radius: 7px; background: color-mix(in srgb, var(--muted) 22%, transparent); }
  .diff-item > div { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-bottom: 5px; }
  .diff-item code { overflow-wrap: anywhere; color: var(--foreground); font-size: 10.5px; }
  .diff-item span { flex: none; color: var(--primary); font-size: 9.5px; }
  .diff-item small { display: block; overflow: hidden; color: var(--muted-foreground); font-family: var(--font-mono); font-size: 9.5px; text-overflow: ellipsis; white-space: nowrap; }
  .config-preview-grid { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 10px; }
  .config-preview-grid section { min-width: 0; }
  .config-preview-grid strong { display: block; margin-bottom: 5px; font-size: 11px; }
  .config-preview-grid pre { max-height: 360px; overflow: auto; margin: 0; padding: 10px; border: 1px solid var(--border); border-radius: 7px; background: color-mix(in srgb, var(--muted) 30%, transparent); font-size: 9.5px; line-height: 1.45; white-space: pre; }
  .state.compact { min-height: 0; padding: 10px; }

  .section {
    padding: 14px 0;
    border-top: 1px solid var(--border);
  }

  .section-title {
    margin-bottom: 8px;
    color: var(--muted-foreground);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .07em;
    text-transform: uppercase;
  }

  .mode-section {
    gap: 18px;
  }

  .mode-copy {
    min-width: 0;
  }

  .mode-copy .section-title {
    margin-bottom: 3px;
  }

  .mode-copy p {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.45;
  }

  .disabled-note {
    margin: 0 0 2px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in srgb, var(--muted) 28%, transparent);
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.45;
  }

  .row-section > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row-section span,
  .system-note {
    color: var(--muted-foreground);
    font-size: 11px;
    line-height: 1.45;
  }

  .row-section :global([data-slot=select-trigger]) {
    min-width: 190px;
  }

  .advanced-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: 4px;
    padding: 5px 6px 5px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in srgb, var(--muted) 20%, transparent);
  }

  .advanced-toggle > button {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 2px 0;
    border: 0;
    background: transparent;
    color: var(--foreground);
    cursor: pointer;
    font: inherit;
    text-align: left;
  }

  .advanced-toggle > button span {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  .advanced-toggle strong {
    font-size: 11.5px;
  }

  .advanced-toggle small {
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .advanced-toggle > button :global(svg) {
    width: 14px;
    height: 14px;
    flex: none;
    transition: transform .15s ease;
  }

  .advanced-toggle > button :global(svg.expanded) {
    transform: rotate(180deg);
  }

  .advanced-actions {
    display: flex;
    flex: none;
    gap: 2px;
  }

  .advanced-actions :global(svg) {
    width: 13px;
    height: 13px;
  }

  .server-list,
  .dispatch-list {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .server-row {
    gap: 10px;
    min-height: 52px;
    padding: 8px 9px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: color-mix(in srgb, var(--muted) 24%, transparent);
  }

  .server-row:hover {
    border-color: color-mix(in srgb, var(--foreground) 16%, var(--border));
    background: color-mix(in srgb, var(--muted) 38%, transparent);
  }

  .server-icon {
    display: grid;
    width: 30px;
    height: 30px;
    flex: none;
    place-items: center;
    border-radius: 7px;
    background: var(--background);
    color: var(--muted-foreground);
    box-shadow: inset 0 0 0 1px var(--border);
  }

  .server-summary {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;
  }

  .server-summary > span {
    overflow: hidden;
    color: var(--muted-foreground);
    font-family: ui-monospace, monospace;
    font-size: 10.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .server-name {
    min-width: 0;
    gap: 6px;
  }

  .server-name strong {
    overflow: hidden;
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .server-type,
  .default-badge {
    flex: none;
    padding: 1px 5px;
    border-radius: 999px;
    background: var(--background);
    color: var(--muted-foreground);
    font-size: 9.5px;
    line-height: 1.5;
    box-shadow: inset 0 0 0 1px var(--border);
  }

  .default-badge {
    background: color-mix(in srgb, var(--primary) 10%, var(--background));
    color: var(--primary);
  }

  .server-actions {
    flex: none;
    gap: 2px;
  }

  .default-row {
    justify-content: flex-end;
    gap: 9px;
    margin-top: 10px;
  }

  .default-row span,
  .field-grid label span,
  .dialog-field-grid label > span {
    color: var(--muted-foreground);
    font-size: 10.5px;
  }

  .default-row :global([data-slot=select-trigger]) {
    min-width: 180px;
  }

  .field-grid,
  .dialog-field-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 9px;
  }

  .field-grid {
    margin-top: 10px;
  }

  .reverse-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .field-grid label,
  .dialog-field-grid label {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 5px;
  }

  .field-grid .wide,
  .dialog-field-grid .wide {
    grid-column: 1 / -1;
  }

  .policy-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .policy-note {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 9px 10px;
    border: 1px solid color-mix(in srgb, #f59e0b 35%, var(--border));
    border-radius: 8px;
    background: color-mix(in srgb, #f59e0b 8%, transparent);
    color: var(--muted-foreground);
    font-size: 10.5px;
    line-height: 1.45;
  }

  .policy-note :global(svg) {
    width: 14px;
    height: 14px;
    flex: none;
    color: #f59e0b;
  }

  .policy-grid {
    margin-top: 0;
  }

  .policy-block {
    min-width: 0;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: color-mix(in srgb, var(--muted) 20%, transparent);
  }

  .policy-block-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 8px;
  }

  .policy-block-head > div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  .policy-block-head strong {
    font-size: 11.5px;
  }

  .policy-block-head span,
  .policy-primary > span,
  .timeout-grid label > span {
    color: var(--muted-foreground);
    font-size: 10px;
    line-height: 1.4;
  }

  .policy-role-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  .policy-primary {
    display: grid;
    grid-template-columns: 72px minmax(0, 1fr);
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .policy-primary :global([data-slot=select-trigger]),
  .fallback-row :global([data-slot=select-trigger]) {
    width: 100%;
  }

  .fallback-list {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .fallback-row {
    display: grid;
    grid-template-columns: 25px minmax(0, 1fr) auto auto auto;
    align-items: center;
    gap: 3px;
  }

  .fallback-order {
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .empty.compact {
    min-height: 0;
    padding: 6px 0 1px;
    text-align: left;
  }

  .timeout-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
    gap: 7px;
  }

  .timeout-grid label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .dispatch-card {
    gap: 8px;
    min-height: 54px;
    padding: 8px 9px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: color-mix(in srgb, var(--muted) 32%, transparent);
  }

  .dispatch-summary {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 4px;
  }

  .dispatch-summary strong {
    font-size: 11.5px;
  }

  .dispatch-summary span {
    overflow: hidden;
    color: var(--muted-foreground);
    font-size: 10.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dispatch-actions {
    flex: none;
    gap: 2px;
  }

  .dispatch-order {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .dispatch-order span {
    width: 28px;
    color: var(--muted-foreground);
    font-size: 11px;
  }

  .server-dialog-form,
  .dispatch-dialog-form {
    display: contents;
  }

  :global(.server-dialog-body),
  :global(.dispatch-dialog-body),
  :global(.effective-dialog-body),
  :global(.json-dialog-body) {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  :global(.effective-dialog-body) { max-height: min(72vh, 760px); overflow: auto; }

  :global(.server-dialog-body),
  :global(.dispatch-dialog-body) {
    max-height: min(68dvh, 640px);
    padding-right: 3px;
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  .dialog-field-grid + .dialog-field-grid {
    padding-top: 2px;
  }

  .dialog-field-grid small {
    color: var(--muted-foreground);
    font-size: 10px;
    font-weight: 400;
    line-height: 1.4;
  }

  .dispatch-dialog-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .dispatch-condition-form {
    display: grid;
    grid-template-columns: minmax(150px, .65fr) minmax(0, 1.35fr);
    gap: 12px;
  }

  .dispatch-condition-form :global([data-slot=select-trigger]) {
    width: 100%;
  }

  .dispatch-dialog-field > span {
    color: var(--muted-foreground);
    font-size: 10.5px;
  }

  .dispatch-dialog-field small {
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .system-note {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in srgb, var(--muted) 28%, transparent);
  }

  .dialog-error {
    padding: 9px 10px;
    border: 1px solid rgba(239, 68, 68, .3);
    border-radius: 8px;
    color: var(--destructive);
    font-size: 11.5px;
  }

  .empty,
  .state {
    padding: 20px;
    color: var(--muted-foreground);
    font-size: 12px;
    text-align: center;
  }

  .load-error {
    display: flex;
    min-height: 120px;
    align-items: center;
    justify-content: center;
    gap: 9px;
    padding: 20px;
    color: var(--destructive);
    font-size: 12px;
    text-align: center;
  }

  .load-error :global(svg) {
    width: 16px;
    flex: none;
  }

  .load-error span {
    max-width: 440px;
    overflow-wrap: anywhere;
  }

  .boundary,
  .issues {
    display: flex;
    gap: 7px;
    margin-top: 12px;
    padding: 9px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    color: var(--muted-foreground);
    font-size: 11.5px;
  }

  .boundary :global(svg) {
    width: 14px;
    flex: none;
  }

  .issues {
    display: block;
  }

  .issues.warning {
    border-color: rgba(245, 158, 11, .3);
    color: #b7791f;
  }

  .issues.error {
    border-color: rgba(239, 68, 68, .3);
    color: var(--destructive);
  }

  .actions {
    position: sticky;
    bottom: 0;
    z-index: 2;
    justify-content: flex-end;
    margin-top: 14px;
    padding: 10px 0 2px;
    background: linear-gradient(to bottom, transparent 0, var(--card) 10px, var(--card) 100%);
  }

  @media (max-width: 900px) {
    .config-preview-grid { grid-template-columns: 1fr; }
    .mode-section {
      align-items: stretch;
      flex-direction: column;
      gap: 10px;
    }

    .field-grid,
    .dialog-field-grid,
    .dispatch-condition-form,
    .policy-role-grid,
    .reverse-grid {
      grid-template-columns: 1fr;
    }

    .field-grid .wide,
    .dialog-field-grid .wide {
      grid-column: auto;
    }

    .dispatch-card {
      align-items: stretch;
      flex-direction: column;
    }

    .advanced-toggle {
      align-items: stretch;
      flex-direction: column;
    }

    .advanced-actions {
      justify-content: flex-end;
    }
  }
</style>
