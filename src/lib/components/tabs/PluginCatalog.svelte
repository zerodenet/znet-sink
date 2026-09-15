<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { Search, Download, RefreshCw, Puzzle, ExternalLink } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import { onPluginDownloadProgress, pluginApi, type PluginDownloadProgress, type PluginListing, type PluginRelease, type PluginSnapshot } from '$lib/services/plugins';
  import { getAppErrorInfo } from '$lib/services/core';

  let { oninstalled, disabled = false, navigation }: { oninstalled: (snapshot: PluginSnapshot) => void; disabled?: boolean; navigation: Snippet } = $props();
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
  let progress = $state<PluginDownloadProgress | null>(null);
  let open = $state(false);
  let generation = 0;
  let alive = true;
  const filtered = $derived(catalog.filter(p => `${p.name} ${p.id} ${p.product_id ?? ''} ${p.description}`.toLowerCase().includes(query.trim().toLowerCase())));
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
        tag = value.find(r => r.channel ? r.channel === 'stable' : !r.prerelease)?.tag_name ?? value[0]?.tag_name ?? '';
      }
    } catch (reason) { if (alive && current === generation) versionError = getAppErrorInfo(reason, '无法读取发布版本').message; }
    finally { if (alive && current === generation) versionsLoading = false; }
  }
  async function install() {
    if (!selected || !tag || installing) return;
    installing = true; versionError = ''; progress = null;
    let unlisten: (() => void) | null = null;
    try {
      try {
        unlisten = await onPluginDownloadProgress(value => {
          if (value.pluginId === selected?.id && value.tag === tag) progress = value;
        });
      } catch {
        // Event delivery is advisory; the installation transaction remains authoritative.
      }
      const snapshot = await pluginApi.installRelease(selected.id, tag);
      if (alive) { open = false; oninstalled(snapshot); }
    } catch (reason) { if (alive) versionError = getAppErrorInfo(reason, '在线安装失败').message; }
    finally { unlisten?.(); if (alive) installing = false; }
  }
  function formatBytes(value?: number) {
    if (value == null) return '—';
    if (value < 1024) return `${value} B`;
    return `${(value / 1024).toFixed(value < 10240 ? 1 : 0)} KiB`;
  }
</script>

<div class="plugins-toolbar">
  {@render navigation()}
  <div class="plugins-toolbar-actions">
    <div class="plugins-search"><Search size={14} aria-hidden="true" /><Input class="pl-8" aria-label="搜索插件" placeholder="搜索插件名称或简介…" bind:value={query} /></div>
    <Button variant="outline" size="sm" disabled={loading || installing} onclick={load}><RefreshCw size={14} class={loading ? 'animate-spin' : ''} />刷新目录</Button>
  </div>
</div>
<div class="plugins-scroll">
  {#if loading}<div class="plugins-empty" role="status"><RefreshCw size={24} class="animate-spin" />正在加载插件目录…</div>
  {:else if error}<div class="plugins-empty" role="alert"><Puzzle size={28} /><p class="plugins-error">{error}</p><Button variant="outline" size="sm" onclick={load}>重试</Button></div>
  {:else if !catalog.length}<div class="plugins-empty"><Puzzle size={28} /><strong>暂时没有已登记的插件</strong><p>插件中心登记后，可在这里查看发行版本并在线安装。</p></div>
  {:else if !filtered.length}<div class="plugins-empty"><Search size={28} /><p>没有找到匹配的插件。</p><Button variant="ghost" size="sm" onclick={() => { query = ''; }}>清除搜索</Button></div>
  {:else}
    <div class="plugins-grid">
      {#each filtered as plugin (plugin.id)}
        <article class="plugin-card">
          <div class="plugin-card-heading"><div class="plugin-icon"><Puzzle size={18} /></div><div class="plugin-identity"><strong>{plugin.name}</strong><span class="plugin-meta">发布者 {plugin.publisher.id}</span></div></div>
          <p class="plugin-description">{plugin.description}</p>
          <div class="plugin-card-actions">
            <Button size="sm" variant="ghost" class="mr-auto" onclick={async () => {
              try { await openUrl(plugin.repository); }
              catch (reason) { error = getAppErrorInfo(reason, '无法打开插件仓库').message; }
            }}><ExternalLink size={13} />仓库</Button>
            <Button size="sm" variant="outline" disabled={disabled || installing} onclick={() => choose(plugin)}><Download size={14} />选择版本</Button>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>
<Dialog.Root bind:open onOpenChange={(value) => { if (installing) open = true; else open = value; }}>
  <Dialog.Content class="sm:max-w-[480px]" showCloseButton={!installing}>
    <Dialog.Header><Dialog.Title>在线安装 · {selected?.name}</Dialog.Title><Dialog.Description>版本与校验信息来自插件中心，安装包直接从作者 GitHub Release 下载。安装后需单独授权才能运行。</Dialog.Description></Dialog.Header>
    <Dialog.Body class="space-y-4">
      <div class="release-source"><span>发布仓库</span><strong>{selected?.repository.replace('https://github.com/', '')}</strong></div>
      {#if versionsLoading}<p role="status">正在读取插件中心发行信息…</p>
      {:else if releases.length}
        <label class="version-label" for="plugin-version">发布版本</label>
        <FieldSelect
          id="plugin-version"
          aria-label="发布版本"
          bind:value={tag}
          disabled={installing}
          options={releases.map(version => ({ value: version.tag_name, label: `${version.tag_name}${version.channel ? `（${version.channel}）` : version.prerelease ? '（预发布）' : ''}` }))}
        />
        {#if release?.prerelease}<p>这是预发布版本，可能尚不稳定。</p>{/if}
        {#if release?.body}<pre class="release-notes">{release.body}</pre>{/if}
        {#if release?.html_url?.startsWith('https://')}
          <Button variant="link" size="sm" onclick={async () => {
            try { await openUrl(release!.html_url!); }
            catch (reason) { versionError = getAppErrorInfo(reason, '无法打开发行说明').message; }
          }}>查看发行说明</Button>
        {/if}
      {:else if !versionError}<p>发布者尚未发布可安装版本。</p>{/if}
      {#if versionError}<p class="error" role="alert">{versionError}</p>{/if}
      {#if versionError && !releases.length}<Button variant="outline" disabled={installing} onclick={() => { if (selected) void choose(selected); }}>重试</Button>{/if}
      {#if installing}
        <div class="download-progress" role="status" aria-live="polite">
          <div><strong>{progress?.state === 'verifying' ? '正在校验插件' : progress?.state === 'retrying' ? `连接中断，正在第 ${(progress?.attempt ?? 1) + 1} 次重试` : '正在从 GitHub Release 下载'}</strong><span>{progress?.percent != null ? `${progress.percent.toFixed(0)}%` : formatBytes(progress?.bytesDownloaded)}</span></div>
          <progress max="100" value={progress?.percent ?? 0}></progress>
          <p>支持断点续传；中断后重新安装同一版本会继续已保存的进度。</p>
        </div>
      {/if}
    </Dialog.Body>
    <Dialog.Footer><Button variant="outline" disabled={installing} onclick={() => { open = false; }}>取消</Button><Button disabled={!tag || versionsLoading || installing || disabled} onclick={install}>{installing ? '正在安装…' : '下载并安装'}</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
<style>
  p { font-size: 12px; color: var(--muted-foreground); overflow-wrap: anywhere; }
  .error { color: var(--destructive); }
  .version-label { display: block; margin-bottom: 8px; font-size: 12px; font-weight: 500; }
  .release-source { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; padding: 10px 12px; border-radius: 8px; background: var(--muted); font-size: 11px; color: var(--muted-foreground); }
  .release-source strong { font-weight: 500; color: var(--foreground); overflow-wrap: anywhere; min-width: 0; }
  .release-notes { white-space: pre-wrap; overflow-wrap: anywhere; font: inherit; font-size: 12px; max-height: 180px; overflow: auto; padding: 12px; border-radius: 8px; border: 1px solid var(--border); }
  .download-progress { display: grid; gap: 8px; padding: 10px 12px; border: 1px solid color-mix(in srgb, var(--primary) 24%, var(--border)); border-radius: 8px; background: color-mix(in srgb, var(--primary) 6%, var(--card)); }
  .download-progress > div { display: flex; justify-content: space-between; gap: 12px; font-size: 12px; }
  .download-progress progress { width: 100%; accent-color: var(--primary); }
  .download-progress p { margin: 0; }
</style>
