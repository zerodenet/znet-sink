<script lang="ts">
  import { onMount } from 'svelte';
  import { AlertTriangle, ChevronDown, ChevronUp, Plus, RefreshCw, Save, Trash2 } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Switch } from '$lib/components/ui/switch';
  import {
    applyActiveDnsSettings,
    createDnsServer,
    loadActiveDnsSettings,
    renameDnsServer,
    setDnsMode,
    validateDnsDraft,
  } from '$lib/services/dns-config';
  import { getAppErrorMessage } from '$lib/services/core';
  import type { DnsMode, DnsServerConfig, DnsServerType, DnsSettingsDraft } from '$lib/types/dns';

  let loading = $state(true);
  let saving = $state(false);
  let error = $state('');
  let saved = $state(false);
  let profileName = $state('');
  let source = $state<Record<string, unknown> | null>(null);
  let draft = $state<DnsSettingsDraft | null>(null);

  const issues = $derived(draft ? validateDnsDraft(draft) : []);
  const errors = $derived(issues.filter((issue) => issue.severity === 'error'));
  const warnings = $derived(issues.filter((issue) => issue.severity === 'warning'));
  const serverNames = $derived(draft ? Object.keys(draft.dns.servers) : []);

  function touch() {
    if (draft) draft = structuredClone(draft);
    saved = false;
    error = '';
  }

  async function load() {
    loading = true;
    error = '';
    try {
      const result = await loadActiveDnsSettings();
      source = result.source;
      profileName = result.profileName;
      draft = result.draft;
      saved = false;
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
    error = '';
  }

  function updateServer(name: string, patch: Partial<DnsServerConfig>) {
    if (!draft) return;
    draft.dns.servers[name] = { ...draft.dns.servers[name], ...patch };
    touch();
  }

  function changeServerType(name: string, type: DnsServerType) {
    if (!draft) return;
    const previous = draft.dns.servers[name];
    const next = createDnsServer(type);
    if (type !== 'system' && previous.type !== 'system') {
      next.host = previous.host;
      next.bootstrap = previous.bootstrap;
      next.server_name = previous.server_name;
    }
    draft.dns.servers[name] = next;
    touch();
  }

  function renameServer(oldName: string, value: string) {
    if (!draft || value.trim() === oldName) return;
    try {
      draft.dns = renameDnsServer(draft.dns, oldName, value);
      touch();
    } catch (cause) {
      error = getAppErrorMessage(cause, '重命名服务器失败');
    }
  }

  function addServer() {
    if (!draft) return;
    let index = 1;
    let name = 'server';
    while (Object.hasOwn(draft.dns.servers, name)) name = `server-${++index}`;
    draft.dns.servers[name] = createDnsServer('udp');
    if (!draft.dns.default_server) draft.dns.default_server = name;
    touch();
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

  function addDispatch() {
    if (!draft) return;
    draft.dns.dispatch.push({ condition: { domain: ['example.com'] }, server: draft.dns.default_server });
    touch();
  }

  function updateDispatchCondition(index: number, raw: string) {
    if (!draft) return;
    try {
      const parsed = JSON.parse(raw);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error();
      draft.dns.dispatch[index].condition = parsed;
      touch();
    } catch {
      error = `第 ${index + 1} 条分流条件不是有效的 JSON 对象`;
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
    if (!draft || !source || errors.length || saving) return;
    saving = true;
    saved = false;
    error = '';
    try {
      source = await applyActiveDnsSettings(source, draft);
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
    <h2>DNS 与 Fake-IP</h2>
    <p>{profileName ? `编辑活动配置“${profileName}”的 runtime.dns` : '编辑活动配置的 runtime.dns'}</p>
  </div>
  <Button variant="ghost" size="icon-sm" onclick={load} disabled={loading || saving} aria-label="重新加载 DNS 配置">
    <RefreshCw class={loading ? 'spin' : ''} />
  </Button>
</div>

{#if loading}
  <div class="state">加载配置中…</div>
{:else if draft}
  <section class="section">
    <div class="section-title">基础模式</div>
    <div class="mode-grid">
      {#each [
        ['disabled', '关闭', '不生成 runtime.dns，保持现有行为'],
        ['real', 'Real DNS', '由 Zero 返回真实解析结果'],
        ['fake_ip', 'Fake-IP', '使用合成地址并恢复原始域名'],
      ] as item}
        <button class:active={draft.mode === item[0]} type="button" onclick={() => changeMode(item[0] as DnsMode)}>
          <strong>{item[1]}</strong><span>{item[2]}</span>
        </button>
      {/each}
    </div>
  </section>

  {#if draft.mode !== 'disabled'}
    <section class="section row-section">
      <div><strong>DNS 劫持</strong><span>Fake-IP 基础模式会自动开启；Real DNS 可按需开启。</span></div>
      <Switch checked={draft.dnsHijack} onCheckedChange={(checked) => { if (draft) { draft.dnsHijack = checked; touch(); } }} aria-label="DNS 劫持" />
    </section>

    <section class="section">
      <div class="section-head">
        <div><div class="section-title">命名服务器</div><p>支持 UDP、DoH、DoT、DoQ 和 system；名称用于默认服务器与分流引用。</p></div>
        <Button variant="outline" size="sm" onclick={addServer}><Plus />新增</Button>
      </div>
      <div class="server-list">
        {#each serverNames as name (name)}
          {@const server = draft.dns.servers[name]}
          <article class="server-card">
            <div class="server-top">
              <Input value={name} onblur={(event) => renameServer(name, event.currentTarget.value)} aria-label="服务器名称" />
              <select value={server.type} onchange={(event) => changeServerType(name, event.currentTarget.value as DnsServerType)}>
                <option value="udp">UDP</option><option value="doh">DoH</option><option value="dot">DoT</option><option value="doq">DoQ</option><option value="system">system</option>
              </select>
              <Button variant="ghost" size="icon-sm" onclick={() => removeServer(name)} aria-label={`删除 ${name}`}><Trash2 /></Button>
            </div>
            {#if server.type !== 'system'}
              <div class="field-grid">
                <label><span>Host</span><Input value={server.host ?? ''} oninput={(event) => updateServer(name, { host: event.currentTarget.value })} placeholder="1.1.1.1 或 dns.example" /></label>
                <label><span>端口</span><Input type="number" value={server.port ?? ''} oninput={(event) => updateServer(name, { port: Number(event.currentTarget.value) })} /></label>
                {#if server.type === 'doh'}<label><span>Path</span><Input value={server.path ?? '/dns-query'} oninput={(event) => updateServer(name, { path: event.currentTarget.value })} /></label>{/if}
                {#if server.type !== 'udp'}<label><span>Server Name</span><Input value={server.server_name ?? ''} oninput={(event) => updateServer(name, { server_name: event.currentTarget.value || undefined })} placeholder="可选 TLS 名称" /></label>{/if}
                <label class="wide"><span>Bootstrap IP（逗号分隔）</span><Input value={(server.bootstrap ?? []).join(', ')} oninput={(event) => updateServer(name, { bootstrap: event.currentTarget.value.split(',').map((value) => value.trim()).filter(Boolean) })} placeholder="1.1.1.1, 1.0.0.1" /></label>
              </div>
            {:else}
              <p class="system-note">system 使用操作系统解析器；严格 TUN/DNS 劫持场景是否允许由 Zero 校验。</p>
            {/if}
          </article>
        {/each}
      </div>
      <label class="default-row"><span>默认服务器</span><select bind:value={draft.dns.default_server} onchange={touch}>{#each serverNames as name}<option value={name}>{name}</option>{/each}</select></label>
    </section>

    <section class="section row-section">
      <div><strong>高级配置</strong><span>服务器分流、缓存和 Fake-IP 生命周期参数。</span></div>
      <Switch checked={draft.advanced} onCheckedChange={(checked) => { if (draft) { draft.advanced = checked; touch(); } }} aria-label="高级 DNS 配置" />
    </section>

    {#if draft.advanced}
      <section class="section">
        <div class="section-head"><div><div class="section-title">缓存</div></div></div>
        <div class="field-grid">
          <label><span>最大缓存条目</span><Input type="number" value={draft.dns.cache?.max_entries ?? 256} oninput={(event) => { if (draft) { draft.dns.cache = { ...draft.dns.cache, max_entries: Number(event.currentTarget.value) }; touch(); } }} /></label>
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
            <label class="wide"><span>排除域名（每行一个）</span><textarea value={draft.dns.answer.exclude_domains.join('\n')} oninput={(event) => { if (draft?.dns.answer.type === 'fake_ip') { draft.dns.answer.exclude_domains = event.currentTarget.value.split('\n').map((value) => value.trim()).filter(Boolean); touch(); } }}></textarea></label>
          </div>
        </section>
      {/if}

      <section class="section">
        <div class="section-head"><div><div class="section-title">有序 DNS 分流</div><p>First-match-wins。条件直接使用 Zero 共享规则模型，客户端不另做匹配。</p></div><Button variant="outline" size="sm" onclick={addDispatch}><Plus />新增</Button></div>
        <div class="dispatch-list">
          {#each draft.dns.dispatch as rule, index (index)}
            <article class="dispatch-card">
              <div class="dispatch-order"><span>#{index + 1}</span><Button variant="ghost" size="icon-xs" onclick={() => moveDispatch(index, -1)} disabled={index === 0}><ChevronUp /></Button><Button variant="ghost" size="icon-xs" onclick={() => moveDispatch(index, 1)} disabled={index === draft.dns.dispatch.length - 1}><ChevronDown /></Button></div>
              <textarea class="condition" value={JSON.stringify(rule.condition, null, 2)} onblur={(event) => updateDispatchCondition(index, event.currentTarget.value)}></textarea>
              <select bind:value={rule.server} onchange={touch}>{#each serverNames as name}<option value={name}>{name}</option>{/each}</select>
              <Button variant="ghost" size="icon-sm" onclick={() => removeDispatch(index)} aria-label="删除分流规则"><Trash2 /></Button>
            </article>
          {/each}
          {#if draft.dns.dispatch.length === 0}<div class="empty">没有分流规则，所有查询使用默认服务器。</div>{/if}
        </div>
      </section>
    {/if}
  {/if}

  <div class="boundary"><AlertTriangle /><span>53 端口 DNS 劫持无法覆盖应用自带的 DoH / DoT / DoQ，也不能从 ECH 中恢复域名。</span></div>
  {#if warnings.length}<div class="issues warning">{#each warnings as issue}<div>{issue.message}</div>{/each}</div>{/if}
  {#if errors.length}<div class="issues error">{#each errors as issue}<div>{issue.field}：{issue.message}</div>{/each}</div>{/if}
  {#if error}<div class="issues error" role="alert">{error}</div>{/if}
  <div class="actions"><Button onclick={save} disabled={saving || errors.length > 0}><Save />{saving ? '保存并应用中…' : saved ? '已保存并应用' : '保存并应用'}</Button></div>
{/if}

<style>
  .panel-head,.section-head,.server-top,.dispatch-card,.row-section,.actions,.default-row{display:flex;align-items:center}.panel-head,.section-head,.row-section{justify-content:space-between}.panel-head{margin-bottom:16px}.panel-head h2{margin:0;font-size:16px}.panel-head p,.section-head p{margin:3px 0 0;color:var(--muted-foreground);font-size:11.5px}.section{padding:14px 0;border-top:1px solid var(--border)}.section-title{margin-bottom:8px;color:var(--muted-foreground);font-size:11px;font-weight:700;letter-spacing:.07em;text-transform:uppercase}.mode-grid{display:grid;grid-template-columns:repeat(3,1fr);gap:8px}.mode-grid button{display:flex;min-height:72px;flex-direction:column;gap:4px;padding:12px;border:1px solid var(--border);border-radius:9px;background:var(--background);color:var(--foreground);text-align:left}.mode-grid button.active{border-color:var(--primary);background:color-mix(in srgb,var(--primary) 8%,transparent)}.mode-grid span,.row-section span,.system-note{color:var(--muted-foreground);font-size:11px;line-height:1.45}.row-section>div{display:flex;flex-direction:column;gap:2px}.server-list,.dispatch-list{display:flex;flex-direction:column;gap:8px}.server-card,.dispatch-card{padding:10px;border:1px solid var(--border);border-radius:9px;background:color-mix(in srgb,var(--muted) 32%,transparent)}.server-top{gap:8px}.server-top :global(input){font-weight:600}.server-top :global(.input){flex:1}select,textarea{border:1px solid var(--border);border-radius:7px;background:var(--background);color:var(--foreground);font:inherit}select{height:32px;padding:0 28px 0 9px}.field-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:9px;margin-top:10px}.field-grid label{display:flex;min-width:0;flex-direction:column;gap:5px}.field-grid label span,.default-row span{color:var(--muted-foreground);font-size:10.5px}.field-grid .wide{grid-column:1/-1}.field-grid textarea{min-height:76px;padding:8px;resize:vertical}.default-row{justify-content:flex-end;gap:9px;margin-top:10px}.dispatch-card{gap:8px}.dispatch-order{display:flex;align-items:center;gap:2px}.dispatch-order span{width:28px;color:var(--muted-foreground);font-size:11px}.condition{min-height:74px;flex:1;padding:7px;font-family:ui-monospace,monospace;font-size:11px;resize:vertical}.empty,.state{padding:20px;color:var(--muted-foreground);font-size:12px;text-align:center}.boundary,.issues{display:flex;gap:7px;margin-top:12px;padding:9px 10px;border:1px solid var(--border);border-radius:8px;color:var(--muted-foreground);font-size:11.5px}.boundary :global(svg){width:14px;flex:none}.issues{display:block}.issues.warning{border-color:rgba(245,158,11,.3);color:#b7791f}.issues.error{border-color:rgba(239,68,68,.3);color:var(--destructive)}.actions{justify-content:flex-end;margin-top:14px}.spin{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}@media(max-width:900px){.mode-grid{grid-template-columns:1fr}.field-grid{grid-template-columns:1fr}.field-grid .wide{grid-column:auto}.dispatch-card{align-items:stretch;flex-direction:column}}
</style>
