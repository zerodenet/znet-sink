<script lang="ts">
  import { onMount } from 'svelte';
  import { untrack } from 'svelte';
  import { Search, RefreshCw, Plus } from '@lucide/svelte';
  import { store } from '$lib/services/store.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { proxyConfigSignal } from '$lib/services/proxy-config-signal.svelte';
  import { createLatestRequestGate } from '$lib/services/latest-request-gate.js';
  import { listConfigurations, type ConfigurationEntry } from '$lib/services/configuration-catalog';
  import { configurationKind, filterConfigurations, pluginLabel } from '$lib/features/configuration/catalog';
  import { getProxyConfig, removeProxyConfig, setActiveProxyConfig, syncSubscription, syncAllSubscriptions,
    removeSubscription, getSubscriptionRemovalPreview } from '$lib/services/config';
  import { getAppErrorMessage } from '$lib/services/core';
  import * as toast from '$lib/services/toast.svelte';
  import type { ProxyConfigProfile, SubscriptionProfile, SubscriptionRemovalPreview } from '$lib/types/domain';
  import ConfigurationEditor from '$lib/components/configuration/ConfigurationEditor.svelte';
  import SubscriptionEditor from '$lib/components/configuration/SubscriptionEditor.svelte';
  import ConfigurationSource from '$lib/components/configuration/ConfigurationSource.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Choice } from '$lib/components/ui/choice';
  import * as Segmented from '$lib/components/AppSegmentedControl';
  import * as Dialog from '$lib/components/ui/dialog';

  let entries = $state<ConfigurationEntry[]>([]);
  let loading = $state(true); let loadError = $state(''); let operationError = $state('');
  let busyId = $state<string | null>(null); let query = $state(''); let kind = $state('all');
  let showCreate = $state(false); let showLocal = $state(false); let showSource = $state(false);
  let editingProfile = $state<ProxyConfigProfile | null>(null); let editingSource = $state<SubscriptionProfile | null>(null);
  let localDelete = $state<ConfigurationEntry | null>(null);
  let sourceDelete = $state<SubscriptionProfile | null>(null);
  let preview = $state<SubscriptionRemovalPreview | null>(null); let removeConfig = $state(false);
  let viewing = $state<ProxyConfigProfile | null>(null);
  const gate = createLatestRequestGate();
  const filtered = $derived(filterConfigurations(entries, kind, query));
  const busy = $derived(busyId !== null);
  const canEdit = $derived(store.isActionOperable('proxyConfig.upsert'));
  const canRemove = $derived(store.isActionOperable('proxyConfig.remove'));
  const current = $derived(entries.find(entry => entry.active)?.name ?? '未选择');

  async function refresh() {
    const request = gate.begin();
    try {
      const result = await listConfigurations();
      if (gate.canApply(request)) { entries = result; loadError = ''; }
    } catch (cause) {
      if (gate.canApply(request)) loadError = getAppErrorMessage(cause, '加载配置失败');
    } finally { if (gate.isLatest(request)) loading = false; }
  }
  $effect(() => { void proxyConfigSignal.revision; untrack(() => { void refresh(); }); });
  onMount(() => () => gate.reset());

  async function run(id: string, action: () => Promise<void>) {
    if (busy) return;
    busyId = id; operationError = '';
    try { await action(); }
    catch (cause) { operationError = getAppErrorMessage(cause, '配置操作失败'); }
    finally { await refresh(); busyId = null; }
  }
  async function saved() {
    await refresh();
    try { await guiState.refreshNodeStateAfterConfigChange(); }
    catch { toast.warning('配置已保存，但节点状态刷新失败，请稍后刷新查看'); }
  }
  function editLocal(entry?: ConfigurationEntry) {
    if (entry?.profileId) void run(entry.id, async () => {
      editingProfile = await getProxyConfig(entry.profileId!); showLocal = true;
    });
    else { editingProfile = null; showLocal = true; }
  }
  function update(source: SubscriptionProfile) {
    void run(source.id, async () => { await syncSubscription(source.id); toast.success('配置来源已更新'); });
  }
  function activate(entry: ConfigurationEntry) {
    if (!entry.profileId || !entry.hasContent || !canEdit) return;
    void run(entry.id, async () => { await setActiveProxyConfig(entry.profileId!); await saved(); toast.success(`已启用 ${entry.name}`); });
  }
  function requestSourceRemoval(source: SubscriptionProfile) {
    void run(source.id, async () => { preview = await getSubscriptionRemovalPreview(source.id); sourceDelete = source; removeConfig = false; });
  }
  function showContent(entry: ConfigurationEntry) {
    if (entry.profileId) void run(entry.id, async () => { viewing = await getProxyConfig(entry.profileId!); });
  }
</script>

<div class="profiles-root">
  <header class="toolbar">
    <div class="toolbar-top">
      <div class="title-block"><h2>配置管理</h2><p>当前配置：{current}</p></div>
      <div class="current-block">
        <Button size="sm" variant="outline" disabled={busy || !entries.some(entry => entry.sources.some(source => source.enabled && !source.managedSource))} onclick={() => run('all', async () => {
          const result = await syncAllSubscriptions();
          if (result.failed) toast.warning(`更新完成：成功 ${result.succeeded}/${result.total}，失败 ${result.failed}`);
          else toast.success(`已更新 ${result.succeeded} 个来源`);
        })}>更新订阅</Button>
        <Button size="icon" variant="ghost" disabled={busy} onclick={() => refresh()} aria-label="刷新配置列表"><RefreshCw class="size-4" /></Button>
        {#if canEdit}<Button size="sm" disabled={busy} onclick={() => showCreate = true}><Plus class="size-4" />添加配置</Button>{/if}
      </div>
    </div>
    <div class="filters">
      <Segmented.Root value={kind} onValueChange={(value) => kind = value} aria-label="配置来源类型">
        <Segmented.Item value="all">全部</Segmented.Item><Segmented.Item value="local">本地</Segmented.Item><Segmented.Item value="subscription">订阅</Segmented.Item><Segmented.Item value="managed">插件托管</Segmented.Item>
      </Segmented.Root>
      <div class="toolbar-search"><span class="search-icon"><Search class="size-4" /></span><Input bind:value={query} class="pl-9" aria-label="搜索配置" placeholder="搜索配置或来源名称" /></div>
    </div>
  </header>
  <div class="list-shell">
    {#if loadError}<div role="alert" class="error">{loadError}<Button variant="outline" size="sm" onclick={() => refresh()}>重试</Button></div>{/if}
    {#if operationError}<p role="alert" class="error">{operationError}</p>{/if}
    {#if loading}<p class="empty">正在加载配置…</p>
    {:else if !filtered.length}<p class="empty">{entries.length ? '没有匹配的配置' : '还没有配置，可添加本地文件、JSON 或订阅。'}</p>
    {:else}<div class="config-list">
      {#each filtered as entry (entry.id)}
        <article class="config-card" class:active={entry.active} aria-label={`配置 ${entry.name}`}>
          <div class="card-heading"><h3>{entry.name}</h3><span class="badge">{entry.active ? '当前配置' : entry.hasContent ? '已保存' : '待更新'}</span></div>
          <p class="metadata">{configurationKind(entry) === 'local' ? entry.path ? '本地文件' : '本地 JSON' : configurationKind(entry) === 'managed' ? '插件托管配置' : '订阅配置'} · {entry.format}</p>
          {#if entry.path}<p class="metadata path">{entry.path}</p>{/if}
          {#if entry.missingProfile}<p class="error">原关联配置已缺失；来源和策略选择仍保留，可重新更新。</p>{/if}
          {#if entry.sources.length > 1}<p class="metadata">保留了 {entry.sources.length} 个历史更新来源；它们更新同一份配置。</p>{/if}
          <div class="card-actions">
            {#if entry.profileId && entry.hasContent}
              <Button size="sm" variant={entry.active ? 'secondary' : 'default'} disabled={busy || entry.active || !canEdit} onclick={() => activate(entry)}>{entry.active ? '当前使用' : '启用配置'}</Button>
              <Button size="sm" variant="outline" disabled={busy} onclick={() => showContent(entry)}>查看内容</Button>
            {/if}
            {#if configurationKind(entry) === 'local'}
              {#if canEdit}<Button size="sm" variant="ghost" disabled={busy} onclick={() => editLocal(entry)}>编辑配置</Button>{/if}
              {#if canRemove}<Button size="sm" variant="ghost" disabled={busy} onclick={() => localDelete = entry}>删除配置</Button>{/if}
            {:else if !entry.sources.length && entry.managedSource}
              <span class="metadata">由 {pluginLabel(entry.managedSource.pluginId)} 托管</span><Button size="sm" variant="outline" onclick={() => store.openTab('plugins')}>前往来源插件</Button>
            {/if}
          </div>
          {#each entry.sources as source (source.id)}
            <ConfigurationSource {source} {canRemove} busy={busy || !canEdit} onupdate={() => update(source)} onedit={() => { editingSource = source; showSource = true; }} onremove={() => requestSourceRemoval(source)} onplugin={() => store.openTab('plugins')} />
          {/each}
        </article>
      {/each}
    </div>{/if}
    <footer>共 {filtered.length} 份配置</footer>
  </div>
</div>

{#if showLocal}<ConfigurationEditor profile={editingProfile} onsaved={saved} onclose={() => showLocal = false} />{/if}
{#if showSource}<SubscriptionEditor source={editingSource} onsaved={saved} onclose={() => showSource = false} />{/if}
<Dialog.Root bind:open={showCreate}><Dialog.Content>
  <Dialog.Header><Dialog.Title>添加配置</Dialog.Title><Dialog.Description>选择配置来源，所有配置在同一个列表中管理。</Dialog.Description></Dialog.Header>
  <Dialog.Body class="grid gap-3">
    <Button variant="outline" onclick={() => { showCreate = false; editLocal(); }}>本地文件或 JSON</Button>
    <Button variant="outline" onclick={() => { showCreate = false; editingSource = null; showSource = true; }}>URL 订阅</Button>
    <Button variant="outline" onclick={() => { showCreate = false; store.openTab('plugins'); }}>插件托管</Button>
  </Dialog.Body>
</Dialog.Content></Dialog.Root>
<Dialog.Root open={!!localDelete} onOpenChange={(value) => { if (!value && !busy) localDelete = null; }}><Dialog.Content>
  <Dialog.Header><Dialog.Title>删除配置</Dialog.Title><Dialog.Description>删除「{localDelete?.name}」{localDelete?.active ? '，当前配置将由现有切换流程处理' : ''}。此操作不修改客户端全局设置。</Dialog.Description></Dialog.Header>
  <Dialog.Footer><Button variant="outline" disabled={busy} onclick={() => localDelete = null}>取消</Button><Button variant="destructive" disabled={busy} onclick={() => {
    if (localDelete?.profileId) void run(localDelete.id, async () => { await removeProxyConfig(localDelete!.profileId!); localDelete = null; await saved(); });
  }}>确认删除</Button></Dialog.Footer>
</Dialog.Content></Dialog.Root>
<Dialog.Root open={!!sourceDelete} onOpenChange={(value) => { if (!value && !busy) sourceDelete = null; }}><Dialog.Content>
  <Dialog.Header><Dialog.Title>移除订阅来源</Dialog.Title><Dialog.Description>默认保留已生成的配置、策略内容和客户端全局设置，仅停止此来源的后续更新。</Dialog.Description></Dialog.Header>
  <Dialog.Body>
    {#if preview?.targetProxyConfig}<label class="flex gap-2"><Choice bind:checked={removeConfig} disabled={busy || preview.targetProxyConfig.sharedBySubscriptionCount > 0} />同时删除配置及此来源的托管规则集</label>
      {#if preview.targetProxyConfig.sharedBySubscriptionCount > 0}<p>另有 {preview.targetProxyConfig.sharedBySubscriptionCount} 个来源使用此配置，配置将保留。</p>{/if}
    {/if}
    {#if operationError}<p role="alert" class="error">{operationError}</p>{/if}
  </Dialog.Body>
  <Dialog.Footer><Button variant="outline" disabled={busy} onclick={() => sourceDelete = null}>取消</Button><Button variant="destructive" disabled={busy} onclick={() => {
    if (sourceDelete) void run(sourceDelete.id, async () => { await removeSubscription(sourceDelete!.id, removeConfig); sourceDelete = null; await saved(); });
  }}>确认移除</Button></Dialog.Footer>
</Dialog.Content></Dialog.Root>
<Dialog.Root open={!!viewing} onOpenChange={(value) => { if (!value) viewing = null; }}><Dialog.Content class="sm:max-w-2xl">
  <Dialog.Header><Dialog.Title>{viewing?.name} · 来源内容</Dialog.Title><Dialog.Description>此处展示保存的配置内容。客户端全局覆盖后的最终配置可在设置的高级配置中查看。</Dialog.Description></Dialog.Header>
  <Dialog.Body><pre class="raw-config">{JSON.stringify(viewing?.content, null, 2)}</pre></Dialog.Body>
  <Dialog.Footer><Button variant="outline" onclick={() => { viewing = null; store.openSettings('config'); }}>查看最终生效配置</Button><Button onclick={() => viewing = null}>关闭</Button></Dialog.Footer>
</Dialog.Content></Dialog.Root>
<style>
  .profiles-root { flex:1; min-width:0; min-height:0; display:flex; flex-direction:column; border:1px solid var(--border); border-radius:14px; background:var(--card); overflow:hidden; }
  .toolbar { padding:20px; display:grid; gap:16px; border-bottom:1px solid var(--border); }
  .toolbar-top,.filters { display:flex; gap:12px; align-items:center; justify-content:space-between; flex-wrap:wrap; }
  .title-block h2 { font-size:16px; font-weight:600; } .title-block p { color:var(--muted-foreground); font-size:12px; margin-top:5px; overflow-wrap:anywhere; }
  .current-block { display:flex; gap:6px; align-items:center; flex-wrap:wrap; }
  .toolbar-search { position:relative; width:260px; max-width:100%; flex:0 1 260px; } .search-icon { position:absolute; left:12px; top:50%; transform:translateY(-50%); color:var(--muted-foreground); pointer-events:none; }
  .list-shell { min-height:0; flex:1; overflow:auto; padding:18px 20px; scrollbar-gutter:stable; }
  .config-list { display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,330px),1fr)); gap:16px; align-items:start; }
  .config-card { min-width:0; display:grid; gap:12px; padding:16px; border:1px solid var(--border); border-radius:12px; }
  .config-card.active { border-color:var(--primary); }
  .card-heading { display:flex; align-items:baseline; flex-wrap:wrap; gap:8px; } .card-heading h3 { font-weight:600; overflow-wrap:anywhere; }
  .badge { font-size:11px; padding:3px 7px; border-radius:5px; background:var(--secondary); color:var(--secondary-foreground); }
  .metadata { font-size:12px; color:var(--muted-foreground); overflow-wrap:anywhere; } .path { font-family:monospace; }
  .card-actions { display:flex; flex-wrap:wrap; gap:5px; }
  .error { color:var(--destructive); font-size:12px; overflow-wrap:anywhere; margin-bottom:8px; }
  .empty { padding:40px 10px; color:var(--muted-foreground); text-align:center; }
  footer { padding-top:16px; color:var(--muted-foreground); font-size:12px; }
  .raw-config { font-size:12px; white-space:pre-wrap; overflow-wrap:anywhere; }
  @media(max-width:500px) { .toolbar,.list-shell { padding:14px; } .toolbar-search { width:100%; flex-basis:100%; } }
</style>
