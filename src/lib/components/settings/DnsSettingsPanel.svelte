<script lang="ts">
  import { onMount } from 'svelte';
  import { AlertTriangle, Braces, ChevronDown, ChevronUp, Pencil, Plus, RefreshCw, Save, Server, Trash2 } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { Switch } from '$lib/components/ui/switch';
  import {
    applyGlobalDnsSettings,
    createDnsServer,
    getDnsKernelCompatibility,
    getDnsAddressFamilyPolicy,
    loadGlobalDnsSettings,
    parseDnsConfig,
    persistGlobalDnsSettings,
    readDnsSettings,
    renameDnsServer,
    setDnsAddressFamilyPolicy,
    setDnsMode,
    validateDnsDraft,
  } from '$lib/services/dns-config';
  import { getAppErrorMessage } from '$lib/services/core';
  import type { DnsKernelCompatibility } from '$lib/services/dns-config';
  import type { DnsAddressFamilyPolicy, DnsDispatchConfig, DnsMode, DnsServerConfig, DnsServerType, DnsSettingsDraft, DnsSettingsInput } from '$lib/types/dns';

  let loading = $state(true);
  let saving = $state(false);
  let error = $state('');
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

  const issues = $derived(draft ? validateDnsDraft(draft) : []);
  const errors = $derived(issues.filter((issue) => issue.severity === 'error'));
  const warnings = $derived(issues.filter((issue) => issue.severity === 'warning'));
  const serverNames = $derived(draft ? Object.keys(draft.dns.servers) : []);
  const addressFamilyPolicy = $derived(draft ? getDnsAddressFamilyPolicy(draft.dns) : 'prefer_ipv4');
  const modeDescription = $derived(draft
    ? ({
        disabled: '暂不注入 DNS 配置，保留当前编辑内容供下次启用。',
        real: '由 Zero 返回真实 DNS 解析结果，可按需启用 TUN DNS 劫持。',
        fake_ip: '使用合成地址并恢复原始域名，同时联动 TUN DNS 劫持。',
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
      description: '保留 IPv4/IPv6；TUN direct 的物理 IPv6 出口不可用时，以可信域名回退 IPv4。',
    },
    {
      value: 'prefer_ipv6',
      label: '双栈 · IPv6 优先',
      description: '优先使用原生 IPv6；仅在物理 IPv6 出口明确不可用时，以可信域名回退 IPv4。',
    },
    {
      value: 'ipv4_only',
      label: '仅 IPv4',
      description: '禁用 IPv6 DNS 结果，适合没有可用 IPv6 网络的兼容场景。',
    },
    {
      value: 'ipv6_only',
      label: '仅 IPv6',
      description: '只使用 IPv6 DNS 结果，并明确禁止自动回退 IPv4。',
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
      if (!tag) throw new Error('请输入规则集标签');
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
    try {
      const [result, kernelCompatibility] = await Promise.all([
        loadGlobalDnsSettings(),
        getDnsKernelCompatibility(),
      ]);
      source = result.source;
      draft = result.draft;
      compatibility = kernelCompatibility;
      jsonDialogOpen = false;
      nativeJson = JSON.stringify(result.source.config ?? result.draft.dns, null, 2);
      nativeError = '';
      saved = false;
      savedPending = false;
    } catch (cause) {
      error = getAppErrorMessage(cause, '加载 DNS 配置失败');
    } finally {
      loading = false;
    }
  }

  function changeMode(mode: DnsMode) {
    if (!draft) return;
    draft = setDnsMode(draft, mode);
    saved = false;
    savedPending = false;
    error = '';
  }

  function changeDefaultServer(value: string) {
    if (!draft) return;
    draft.dns.default_server = value;
    touch();
  }

  function changeAddressFamilyPolicy(value: string) {
    if (!draft) return;
    const option = addressFamilyOptions.find((candidate) => candidate.value === value);
    if (!option) return;
    draft.dns = setDnsAddressFamilyPolicy(draft.dns, option.value);
    touch();
  }

  function changeServerDraftType(type: DnsServerType) {
    const previous = serverDraft;
    const next = createDnsServer(type);
    if (type !== 'system' && previous.type !== 'system') {
      next.host = previous.host;
      next.bootstrap = previous.bootstrap;
      next.server_name = previous.server_name;
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
    return server.type === 'doh' ? `${endpoint}${server.path || '/dns-query'}` : endpoint;
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
      error = getAppErrorMessage(cause, '保存 DNS 配置失败，已保留上次可用配置');
    } finally {
      saving = false;
    }
  }

  onMount(() => void load());
</script>

<div class="panel-head">
  <div>
    <h2>内核 DNS 与 Fake-IP</h2>
    <p>客户端覆盖，不写回代理配置；应用时注入 Zero 有效配置并由内核校验</p>
  </div>
  <div class="head-actions">
    <Button variant="outline" size="sm" onclick={openJsonEditor} disabled={loading || saving || !draft}>
      <Braces />编辑 JSON
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
    <Button variant="outline" size="sm" onclick={load}>重试</Button>
  </div>
{:else if draft}
  <p class="workflow-hint">配置流程：选择基础模式 → 编辑 DNS、Fake-IP 和分流策略 → 点击底部“保存并应用”。高级用户可从右上角打开 Zero 原生 JSON。</p>

  {#if compatibility.status === 'unsupported' && draft.mode === 'fake_ip'}
    <div class="issues warning" role="status">
      <div>当前内核未声明 DNS/Fake-IP 能力。配置仍会保存到客户端，升级内核后重新点击“保存并应用”即可生效。</div>
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
    <div class="mode-control" role="radiogroup" aria-label="DNS 基础模式">
      {#each [
        ['disabled', '关闭'],
        ['real', 'Real DNS'],
        ['fake_ip', 'Fake-IP'],
      ] as item}
        <button
          class:active={draft.mode === item[0]}
          type="button"
          role="radio"
          aria-checked={draft.mode === item[0]}
          onclick={() => changeMode(item[0] as DnsMode)}
        >{item[1]}</button>
      {/each}
    </div>
  </section>

  {#if draft.mode === 'disabled'}
    <div class="disabled-note">当前未启用 DNS 覆盖；下面的服务器、缓存和分流策略仍可编辑，保存后会作为下次启用时的配置。</div>
  {:else}
    <section class="section row-section">
      <div><strong>DNS 劫持</strong><span>Fake-IP 基础模式会自动开启；Real DNS 可按需开启。</span></div>
      <Switch checked={draft.dnsHijack} onCheckedChange={(checked) => { if (draft) { draft.dnsHijack = checked; touch(); } }} aria-label="DNS 劫持" />
    </section>
  {/if}

  <section class="section row-section">
    <div>
      <strong>IPv6 兼容策略</strong>
      <span>{addressFamilyDescription}</span>
    </div>
    <Select.Root
      type="single"
      value={addressFamilyPolicy}
      onValueChange={(value) => { if (value) changeAddressFamilyPolicy(value); }}
    >
      <Select.Trigger aria-label="IPv6 兼容策略">
        {addressFamilyOptions.find((option) => option.value === addressFamilyPolicy)?.label ?? addressFamilyPolicy}
      </Select.Trigger>
      <Select.Content>
        {#each addressFamilyOptions as option}
          <Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  </section>

  <section class="section">
      <div class="section-head">
        <div><div class="section-title">命名服务器</div><p>支持 UDP、DoH、DoT、DoQ 和 system；名称用于默认服务器与分流引用。</p></div>
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

      <section class="section">
        <div class="section-head"><div><div class="section-title">DNS 缓存</div><p>控制内核缓存的容量和最长保留时间。</p></div></div>
        <div class="field-grid">
          <label><span>最大缓存条目</span><Input type="number" value={draft.dns.cache?.max_entries ?? 1024} oninput={(event) => { if (draft) { draft.dns.cache = { ...draft.dns.cache, max_entries: Number(event.currentTarget.value) }; touch(); } }} /></label>
          <label><span>最大 TTL（秒，可选）</span><Input type="number" value={draft.dns.cache?.max_ttl_seconds ?? ''} oninput={(event) => { if (draft?.dns.cache) { draft.dns.cache.max_ttl_seconds = event.currentTarget.value ? Number(event.currentTarget.value) : undefined; touch(); } }} /></label>
        </div>
      </section>

      {#if draft.mode === 'fake_ip' && draft.dns.answer.type === 'fake_ip'}
        <section class="section">
          <div class="section-title">Fake-IP</div>
          <div class="field-grid">
            <label><span>CIDR</span><Input bind:value={draft.dns.answer.cidr} oninput={touch} /></label>
            <label><span>TTL（秒）</span><Input type="number" bind:value={draft.dns.answer.ttl_seconds} oninput={touch} /></label>
            <label><span>最大映射数（可选）</span><Input type="number" value={draft.dns.answer.max_entries ?? ''} oninput={(event) => { if (draft?.dns.answer.type === 'fake_ip') { draft.dns.answer.max_entries = event.currentTarget.value ? Number(event.currentTarget.value) : undefined; touch(); } }} /></label>
            <label class="wide"><span>排除域名（每行一个）</span><textarea value={(draft.dns.answer.exclude_domains ?? []).join('\n')} oninput={(event) => { if (draft?.dns.answer.type === 'fake_ip') { draft.dns.answer.exclude_domains = event.currentTarget.value.split('\n').map((value) => value.trim()).filter(Boolean); touch(); } }}></textarea></label>
          </div>
        </section>
      {/if}

      <section class="section">
        <div class="section-head"><div><div class="section-title">有序 DNS 分流</div><p>First-match-wins。条件直接使用 Zero 共享规则模型，客户端不另做匹配。</p></div><Button variant="outline" size="sm" onclick={openAddDispatch}><Plus />新增</Button></div>
        <div class="dispatch-list">
          {#each draft.dns.dispatch as rule, index (index)}
            <article class="dispatch-card">
              <div class="dispatch-order"><span>#{index + 1}</span><Button variant="ghost" size="icon-xs" onclick={() => moveDispatch(index, -1)} disabled={index === 0}><ChevronUp /></Button><Button variant="ghost" size="icon-xs" onclick={() => moveDispatch(index, 1)} disabled={index === draft.dns.dispatch.length - 1}><ChevronDown /></Button></div>
              <div class="dispatch-summary">
                <strong>{rule.server}</strong>
                <code>{JSON.stringify(rule.condition)}</code>
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

  <div class="boundary"><AlertTriangle /><span>53 端口 DNS 劫持无法覆盖应用自带的 DoH / DoT / DoQ，也不能从 ECH 中恢复域名。</span></div>
  {#if warnings.length}<div class="issues warning">{#each warnings as issue}<div>{issue.message}</div>{/each}</div>{/if}
  {#if errors.length}<div class="issues error">{#each errors as issue}<div>{issue.field}：{issue.message}</div>{/each}</div>{/if}
  {#if error}<div class="issues error" role="alert">{error}</div>{/if}
  <div class="actions"><Button onclick={save} disabled={saving || errors.length > 0}><Save />{saving ? '保存并应用中…' : saved ? savedPending ? '已保存，待内核' : '已保存并应用' : '保存并应用'}</Button></div>
{/if}

<Dialog.Root bind:open={serverDialogOpen}>
  <Dialog.Content class="sm:max-w-[620px]">
    <form
      class="server-dialog-form"
      onsubmit={(event) => {
        event.preventDefault();
        saveServerDialog();
      }}
    >
      <Dialog.Header>
        <Dialog.Title>{editingServerName ? '编辑 DNS 服务器' : '新增 DNS 服务器'}</Dialog.Title>
        <Dialog.Description>服务器名称会被默认路由和分流规则引用；协议切换只显示相关字段。</Dialog.Description>
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
          <div class="system-note">system 使用操作系统解析器，不需要网络端点。严格 TUN/DNS 劫持场景是否允许由 Zero 校验。</div>
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
              <small>Host 使用域名时建议提供 bootstrap，最终仍由 Zero 校验。</small>
            </label>
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
  <Dialog.Content class="sm:max-w-[680px]">
    <form
      class="dispatch-dialog-form"
      onsubmit={(event) => {
        event.preventDefault();
        saveDispatchDialog();
      }}
    >
      <Dialog.Header>
        <Dialog.Title>{editingDispatchIndex === null ? '新增 DNS 分流' : '编辑 DNS 分流'}</Dialog.Title>
        <Dialog.Description>规则按列表顺序匹配；条件使用 Zero 共享规则模型的 JSON 对象。</Dialog.Description>
      </Dialog.Header>
      <Dialog.Body class="dispatch-dialog-body">
        <div class="dispatch-editor-tabs" role="tablist" aria-label="DNS 分流条件编辑方式">
          <button type="button" role="tab" aria-selected={dispatchEditorMode === 'form'} data-active={dispatchEditorMode === 'form'} onclick={() => switchDispatchEditorMode('form')}>表单</button>
          <button type="button" role="tab" aria-selected={dispatchEditorMode === 'json'} data-active={dispatchEditorMode === 'json'} onclick={() => switchDispatchEditorMode('json')}>JSON</button>
        </div>
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
                <span>规则集标签</span>
                <Input bind:value={dispatchConditionTagDraft} placeholder="AI-Suite" />
                <small>引用 route.rule_sets 中已有的规则集 tag。</small>
              </label>
            {:else}
              <label class="dispatch-dialog-field">
                <span>匹配值 <small>每行一个</small></span>
                <textarea
                  class="condition dispatch-values-editor"
                  bind:value={dispatchConditionValuesDraft}
                  placeholder={dispatchConditionOptions.find((option) => option.value === dispatchConditionType)?.placeholder ?? ''}
                ></textarea>
              </label>
            {/if}
          </div>
        {:else}
          <label class="dispatch-dialog-field">
            <span>匹配条件 JSON</span>
            <textarea class="condition dispatch-condition-editor" bind:value={dispatchConditionDraft} spellcheck="false"></textarea>
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

<Dialog.Root bind:open={jsonDialogOpen}>
  <Dialog.Content class="sm:max-w-[760px]">
    <Dialog.Header>
      <Dialog.Title>编辑 Zero 原生 DNS JSON</Dialog.Title>
      <Dialog.Description>应用后会更新表单草稿；仍需在主页面点击“保存并应用”才会提交到内核。</Dialog.Description>
    </Dialog.Header>
    <Dialog.Body class="json-dialog-body">
      <textarea class="native-json" bind:value={nativeJson} spellcheck="false" aria-label="Zero 原生 DNS JSON 配置"></textarea>
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
  .mode-control,
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

  .workflow-hint {
    margin: 0 0 4px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: color-mix(in srgb, var(--primary) 5%, transparent);
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.45;
  }

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

  .mode-control {
    flex: none;
    gap: 2px;
    padding: 3px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in srgb, var(--muted) 40%, transparent);
  }

  .mode-control button {
    min-width: 72px;
    padding: 6px 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
  }

  .mode-control button:hover {
    color: var(--foreground);
  }

  .mode-control button.active {
    background: var(--background);
    box-shadow: 0 1px 2px rgba(0, 0, 0, .08);
    color: var(--foreground);
    font-weight: 600;
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

  .field-grid textarea {
    min-height: 76px;
    padding: 8px;
    resize: vertical;
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

  .dispatch-summary code {
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

  .condition {
    min-height: 74px;
    flex: 1;
    padding: 7px;
    font-family: ui-monospace, monospace;
    font-size: 11px;
    resize: vertical;
  }

  .server-dialog-form,
  .dispatch-dialog-form {
    display: contents;
  }

  :global(.server-dialog-body),
  :global(.dispatch-dialog-body),
  :global(.json-dialog-body) {
    display: flex;
    flex-direction: column;
    gap: 14px;
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

  .dispatch-editor-tabs {
    display: flex;
    width: fit-content;
    gap: 2px;
    padding: 3px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in srgb, var(--muted) 40%, transparent);
  }

  .dispatch-editor-tabs button {
    min-width: 72px;
    padding: 6px 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
  }

  .dispatch-editor-tabs button[data-active='true'] {
    background: var(--background);
    box-shadow: 0 1px 2px rgba(0, 0, 0, .08);
    color: var(--foreground);
    font-weight: 600;
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

  .dispatch-condition-editor {
    min-height: 180px;
    width: 100%;
  }

  .dispatch-values-editor {
    min-height: 112px;
    width: 100%;
  }

  .system-note {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in srgb, var(--muted) 28%, transparent);
  }

  .native-json {
    width: 100%;
    min-height: min(54vh, 480px);
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    outline: none;
    background: var(--background);
    color: var(--foreground);
    font-family: ui-monospace, monospace;
    font-size: 12px;
    line-height: 1.55;
    resize: none;
  }

  .native-json:focus {
    border-color: var(--ring);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ring) 15%, transparent);
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
    .mode-section {
      align-items: stretch;
      flex-direction: column;
      gap: 10px;
    }

    .mode-control {
      align-self: flex-start;
    }

    .field-grid,
    .dialog-field-grid,
    .dispatch-condition-form {
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
  }
</style>
