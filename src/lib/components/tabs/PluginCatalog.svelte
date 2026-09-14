<script lang="ts">
  import { onMount } from 'svelte';
  import { Search, Download, RefreshCw } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as Dialog from '$lib/components/ui/dialog';
  import { pluginApi, type PluginListing, type PluginRelease, type PluginSnapshot } from '$lib/services/plugins';
  import { getAppErrorInfo } from '$lib/services/core';

  let { oninstalled, disabled = false }: { oninstalled: (snapshot: PluginSnapshot) => void; disabled?: boolean } = $props();
  let catalog = $state<PluginListing[]>([]);
  let loading = $state(true);
  let error = $state('');
  let query = $state('');
  let selected = $state<PluginListing | null>(null);
  let releases = $state<PluginRelease[]>([]);
  let versionsLoading = $state(false);
  let versionError = $state('');
  let tag = $state('');
  let installing = $state(false);
  let open = $state(false);
  let generation = 0;
  let alive = true;
  const filtered = $derived(catalog.filter(p => `${p.name} ${p.id} ${p.description}`.toLowerCase().includes(query.trim().toLowerCase())));
  const release = $derived(releases.find(r => r.tag_name === tag));

  async function load() {
    loading = true; error = '';
    try { const value = await pluginApi.catalog(); if (alive) catalog = value; }
    catch (reason) { if (alive) error = getAppErrorInfo(reason, '无法加载插件目录').message; }
    finally { if (alive) loading = false; }
  }
  onMount(() => { alive = true; void load(); return () => { alive = false; generation++; }; });
  async function choose(plugin: PluginListing) {
    selected = plugin; releases = []; tag = ''; versionError = ''; versionsLoading = true; open = true;
    const current = ++generation;
    try {
      const value = await pluginApi.releases(plugin.id);
      if (alive && current === generation) {
        releases = value;
        tag = value.find(r => !r.prerelease)?.tag_name ?? value[0]?.tag_name ?? '';
      }
    } catch (reason) { if (alive && current === generation) versionError = getAppErrorInfo(reason, '无法读取发布版本').message; }
    finally { if (alive && current === generation) versionsLoading = false; }
  }
  async function install() {
    if (!selected || !tag || installing) return;
    installing = true; versionError = '';
    try {
      const snapshot = await pluginApi.installRelease(selected.id, tag);
      if (alive) { open = false; oninstalled(snapshot); }
    } catch (reason) { if (alive) versionError = getAppErrorInfo(reason, '在线安装失败').message; }
    finally { if (alive) installing = false; }
  }
</script>

<div class="catalog-toolbar">
  <div class="search"><Search size={14} aria-hidden="true" /><Input class="pl-8" aria-label="搜索插件" placeholder="搜索插件名称或简介" bind:value={query} /></div>
  <Button variant="outline" size="sm" disabled={loading || installing} onclick={load}><RefreshCw size={14} />刷新目录</Button>
</div>
{#if loading}<div class="empty" role="status">正在加载在线插件目录…</div>
{:else if error}<div class="empty" role="alert">{error}<Button variant="outline" size="sm" onclick={load}>重试</Button></div>
{:else if !catalog.length}<div class="empty">暂时没有已上架的插件。发布者完成登记并发布后，可在这里选择版本并在线安装。</div>
{:else if !filtered.length}<div class="empty">没有找到匹配的插件。</div>
{:else}
  <div class="catalog-grid">
    {#each filtered as plugin (plugin.id)}
      <article>
        <strong>{plugin.name}</strong><p>{plugin.description}</p>
        <small>发布者 {plugin.publisher.id}</small>
        <Button size="sm" variant="outline" disabled={disabled || installing} onclick={() => choose(plugin)}><Download size={14} />选择版本</Button>
      </article>
    {/each}
  </div>
{/if}
<Dialog.Root bind:open onOpenChange={(value) => { if (installing) open = true; else open = value; }}>
  <Dialog.Content class="sm:max-w-[480px]" showCloseButton={!installing}>
    <Dialog.Header><Dialog.Title>在线安装 · {selected?.name}</Dialog.Title><Dialog.Description>从发布者的登记仓库下载。安装后需单独授权才能运行。</Dialog.Description></Dialog.Header>
    <Dialog.Body>
      {#if versionsLoading}<p role="status">正在读取发布版本…</p>
      {:else if releases.length}
        <label class="version-label" for="plugin-version">发布版本</label>
        <select id="plugin-version" class="znet-field w-full" bind:value={tag} disabled={installing}>
          {#each releases as version}<option value={version.tag_name}>{version.tag_name}{version.prerelease ? '（预发布）' : ''}</option>{/each}
        </select>
        {#if release?.prerelease}<p>这是预发布版本，可能尚不稳定。</p>{/if}
        {#if release?.body}<pre>{release.body}</pre>{/if}
      {:else if !versionError}<p>发布者尚未发布可安装版本。</p>{/if}
      {#if versionError}<p class="error" role="alert">{versionError}</p>{/if}
      {#if versionError && !releases.length}<Button variant="outline" disabled={installing} onclick={() => { if (selected) void choose(selected); }}>重试</Button>{/if}
      {#if installing}<p role="status">正在下载并校验插件，请稍候…</p>{/if}
    </Dialog.Body>
    <Dialog.Footer><Button variant="outline" disabled={installing} onclick={() => { open = false; }}>取消</Button><Button disabled={!tag || versionsLoading || installing || disabled} onclick={install}>{installing ? '正在安装…' : '下载并安装'}</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<style>
  .catalog-toolbar { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .search { position: relative; width: 280px; max-width: 100%; }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--muted-foreground); pointer-events: none; }
  .catalog-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(260px, 100%), 1fr)); gap: 12px; }
  article { display: flex; flex-direction: column; align-items: flex-start; gap: 12px; padding: 18px; border: 1px solid var(--border); border-radius: 10px; overflow-wrap: anywhere; min-width: 0; }
  p, small { color: var(--muted-foreground); overflow-wrap: anywhere; }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 40px 20px; background: var(--muted); border-radius: 10px; color: var(--muted-foreground); text-align: center; }
  .error { color: var(--destructive); }
  .version-label { display: block; margin-bottom: 8px; }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; font: inherit; max-height: 180px; overflow: auto; margin-top: 12px; }
</style>
