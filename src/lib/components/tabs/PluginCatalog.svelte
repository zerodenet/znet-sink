<script lang="ts">
  import { onMount } from 'svelte';
  import { openExternalUrl as openUrl } from '$lib/services/platform';
  import { ArrowLeft, Download, ExternalLink, LayoutGrid, List, Puzzle, RefreshCw, Search, Upload } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { onPluginDownloadProgress, pluginApi, type PluginComponent, type PluginDownloadProgress, type PluginInstallReview, type PluginListing, type PluginRelease, type PluginSnapshot } from '$lib/services/plugins';
  import { permissionLabel } from '$lib/services/plugin-permissions';
  import { compareAppVersions } from '$lib/services/app-update-policy';
  import { releasesInChannel, releaseChannel, pluginChannelLabel, type PluginChannel } from '$lib/services/plugin-update-policy';
  import { getAppErrorInfo } from '$lib/services/core';

  const VIEW_MODE_KEY = 'znet-plugin-market-view-mode';
  type ViewMode = 'card' | 'list';

  let { oninstalled, onmanage, onlocalinstall, onrefresh, onchannelchange, requestedUpdate = null, channel = 'stable', installed = [], disabled = false }: {
    oninstalled: (snapshot: PluginSnapshot, review: PluginInstallReview) => void;
    onmanage: () => void;
    onlocalinstall: () => void | Promise<void>;
    onrefresh?: (catalog: PluginListing[]) => void | Promise<void>;
    onchannelchange?: (value: string) => void;
    requestedUpdate?: { pluginId: string; tag: string } | null;
    channel?: PluginChannel;
    installed?: PluginComponent[];
    disabled?: boolean;
  } = $props();
  let catalog = $state<PluginListing[]>([]);
  let loading = $state(true);
  let error = $state('');
  let refreshMessage = $state('');
  let query = $state('');
  let viewMode = $state<ViewMode>('list');
  let selected = $state<PluginListing | null>(null);
  let releases = $state<PluginRelease[]>([]);
  const channelReleases = $derived(releasesInChannel(releases, channel));
  let versionsLoading = $state(false);
  let versionError = $state('');
  let tag = $state('');
  let installing = $state(false);
  let progress = $state<PluginDownloadProgress | null>(null);
  let permissionReview = $state<PluginInstallReview | null>(null);
  let permissionReviewKey = $state('');
  let generation = 0;
  let alive = true;
  const installedById = $derived(new Map(installed.map(component => [component.plugin_id, component])));
  const filtered = $derived(catalog.filter(plugin => `${plugin.name} ${plugin.id} ${plugin.product_id ?? ''} ${plugin.description}`.toLowerCase().includes(query.trim().toLowerCase())));
  const release = $derived(releases.find(candidate => candidate.tag_name === tag));
  const installedVersion = $derived(selected ? installedById.get(selected.id)?.version ?? null : null);
  const releaseRelation = $derived.by(() => {
    if (!installedVersion || !release) return 'install' as const;
    const comparison = compareAppVersions(release.tag_name, installedVersion);
    return comparison > 0 ? 'upgrade' as const : comparison < 0 ? 'older' as const : 'current' as const;
  });
  $effect(() => {
    if (selected && !channelReleases.some(candidate => candidate.tag_name === tag)) {
      tag = channelReleases[0]?.tag_name ?? '';
      permissionReview = null;
      permissionReviewKey = '';
    }
  });

  function loadViewMode(): ViewMode {
    try { return localStorage.getItem(VIEW_MODE_KEY) === 'card' ? 'card' : 'list'; }
    catch { return 'list'; }
  }
  function setViewMode(mode: ViewMode) {
    viewMode = mode;
    try { localStorage.setItem(VIEW_MODE_KEY, mode); }
    catch { /* View preference persistence is best effort. */ }
  }
  async function load(notify = false) {
    loading = true; error = ''; refreshMessage = '';
    try {
      const value = await pluginApi.catalog();
      if (alive) {
        catalog = Array.from(new Map(value.map(plugin => [plugin.id, plugin])).values());
        if (notify) await onrefresh?.(catalog);
        if (notify) refreshMessage = `市场已刷新，发现 ${catalog.length} 个已登记插件；已安装插件的更新结果可在管理页查看。`;
      }
    } catch (reason) {
      if (alive) error = getAppErrorInfo(reason, '无法加载插件目录').message;
    } finally {
      if (alive) loading = false;
    }
  }
  onMount(() => {
    alive = true;
    viewMode = loadViewMode();
    void load().then(() => {
      if (requestedUpdate) {
        const plugin = catalog.find(value => value.id === requestedUpdate.pluginId);
        if (plugin) void choose(plugin, requestedUpdate.tag);
      }
    });
    return () => { alive = false; generation++; };
  });
  async function choose(plugin: PluginListing, preferredTag?: string) {
    selected = plugin;
    releases = [];
    tag = '';
    versionError = '';
    permissionReview = null;
    permissionReviewKey = '';
    versionsLoading = true;
    const current = ++generation;
    try {
      const value = await pluginApi.releases(plugin.id);
      if (alive && current === generation) {
        releases = value;
        const matching = releasesInChannel(value, channel);
        tag = matching.find(candidate => candidate.tag_name === preferredTag)?.tag_name ?? matching[0]?.tag_name ?? '';
      }
    } catch (reason) {
      if (alive && current === generation) versionError = getAppErrorInfo(reason, '无法读取发布版本').message;
    } finally {
      if (alive && current === generation) versionsLoading = false;
    }
  }
  function closeDetail() {
    if (installing) return;
    selected = null;
    releases = [];
    tag = '';
    versionError = '';
    permissionReview = null;
    permissionReviewKey = '';
    progress = null;
  }
  async function install() {
    if (!selected || !tag || installing || releaseRelation === 'older' || releaseRelation === 'current') return;
    installing = true; versionError = ''; progress = null;
    let unlisten: (() => void) | null = null;
    try {
      try {
        unlisten = await onPluginDownloadProgress(value => {
          if (value.pluginId === selected?.id && value.tag === tag) progress = value;
        });
      } catch {
        // Progress events are advisory; the installation transaction remains authoritative.
      }
      const key = `${selected.id}@${tag}`;
      const review = permissionReviewKey === key && permissionReview
        ? permissionReview
        : await pluginApi.previewRelease(selected.id, tag);
      if ((review.added_permissions.length || review.removed_permissions.length || review.requires_approval)
        && permissionReviewKey !== key) {
        permissionReview = review;
        permissionReviewKey = key;
        progress = null;
        return;
      }
      const snapshot = await pluginApi.installRelease(
        selected.id,
        tag,
        review.requires_approval ? review.candidate_digest : undefined,
      );
      if (alive) oninstalled(snapshot, review);
    } catch (reason) {
      if (alive) versionError = getAppErrorInfo(reason, '在线安装失败').message;
    } finally {
      unlisten?.();
      if (alive) installing = false;
    }
  }
  function formatBytes(value?: number) {
    if (value == null) return '—';
    if (value < 1024) return `${value} B`;
    return `${(value / 1024).toFixed(value < 10240 ? 1 : 0)} KiB`;
  }
  function channelLabel(value?: PluginRelease) {
    if (!value) return '';
    return pluginChannelLabel[releaseChannel(value)];
  }
</script>

<div class="plugin-catalog">
  {#if selected}
    <div class="plugin-release-workspace">
      <div class="plugin-release-nav">
        <Button variant="ghost" size="sm" disabled={installing} onclick={closeDetail}><ArrowLeft size={14} />返回插件市场</Button>
        <span>{installedVersion ? '版本管理' : '安装插件'}</span>
      </div>
      <div class="plugin-release-scroll">
        <section class="plugin-release-hero">
          <div class="plugin-icon"><Puzzle size={18} /></div>
          <div><strong>{selected.name}</strong><span>发布者 {selected.publisher.id}</span></div>
        </section>
        <p>{selected.description}</p>
        <p>安装包安装与运行权限授权分为两个步骤；权限有变化时，安装后可能需要再次确认才能启用。</p>
        <div class="plugin-release-channel"><label for="plugin-release-channel">发行通道</label><FieldSelect id="plugin-release-channel" aria-label="插件发行通道" value={channel} onValueChange={onchannelchange} disabled={installing} options={[
          { value: 'stable', label: '正式版' }, { value: 'rc', label: '候选版' }, { value: 'dev', label: '开发版' },
        ]} /></div>
        <div class="release-source"><span>发布仓库</span><strong>{selected.repository.replace('https://github.com/', '')}</strong></div>

        {#if versionsLoading}
          <div class="plugins-empty compact" role="status"><RefreshCw size={20} class="animate-spin" />正在读取最新兼容版本…</div>
        {:else if channelReleases.length}
          {#if installedVersion}
            <label class="version-label" for="plugin-version">目标版本</label>
            <FieldSelect id="plugin-version" aria-label="目标版本" bind:value={tag} disabled={installing} options={channelReleases.map(version => ({ value: version.tag_name, label: `${version.tag_name}（${channelLabel(version)}）` }))} />
          {:else}
            <div class="release-version-summary"><span>安装版本</span><strong>{release?.tag_name}（{channelLabel(release)}）</strong><em>最新兼容版本</em></div>
          {/if}

          {#if release?.prerelease}<p>这是预发布版本，可能尚不稳定。</p>{/if}
          {#if releaseRelation === 'older'}
            <p class="version-notice" role="status">当前已安装 {installedVersion}，所选远端版本 {release?.tag_name.replace(/^v/, '')} 更旧。如需回退，请先卸载当前插件；卸载会同时清除其配置。</p>
          {:else if releaseRelation === 'current'}
            <p class="version-notice" role="status">当前已经安装这个版本，无需重复下载。</p>
          {/if}
          {#if permissionReview && permissionReviewKey === `${selected.id}@${tag}` && (permissionReview.added_permissions.length || permissionReview.removed_permissions.length || permissionReview.requires_approval)}
            <div class="permission-change-review" role="alert">
              <strong>{permissionReview.added_permissions.length ? installedVersion ? '此更新申请新增或扩大权限' : '安装包申请以下宿主权限' : '此更新移除了权限声明'}</strong>
              <p>{permissionReview.added_permissions.length ? '确认安装包后，还需在插件权限页批准运行权限；确认更新并不会直接授权。' : '移除的权限会被撤销；若其余权限不变，原有启用状态会尽量保留。'}</p>
              <ul>
                {#each permissionReview.added_permissions as change}
                  <li><span>{change.component_id}</span><b>{permissionLabel(change.request.capability)}</b><code title={change.request.scope}>{change.request.scope}</code>{change.required ? '（必需）' : '（可选）'}</li>
                {/each}
                {#each permissionReview.removed_permissions as change}
                  <li><span>{change.component_id}</span><b>移除 {permissionLabel(change.request.capability)}</b><code title={change.request.scope}>{change.request.scope}</code></li>
                {/each}
              </ul>
            </div>
          {/if}
          {#if release?.body}<pre class="release-notes">{release.body}</pre>{/if}
          {#if release?.html_url?.startsWith('https://')}
            <Button variant="link" size="sm" onclick={async () => {
              try { await openUrl(release!.html_url!); }
              catch (reason) { versionError = getAppErrorInfo(reason, '无法打开发行说明').message; }
            }}>查看发行说明</Button>
          {/if}
        {:else if !versionError}
          <p>市场尚无适用于当前客户端与平台的{pluginChannelLabel[channel]}发行包。</p>
        {/if}
        {#if versionError}<p class="error" role="alert">{versionError}</p>{/if}
        {#if versionError && !releases.length}<Button variant="outline" disabled={installing} onclick={() => { if (selected) void choose(selected); }}>重试</Button>{/if}
        {#if installing}
          <div class="download-progress" role="status" aria-live="polite">
            <div><strong>{progress?.state === 'verifying' ? '正在校验插件' : progress?.state === 'retrying' ? `连接中断，正在第 ${(progress?.attempt ?? 1) + 1} 次重试` : '正在从 GitHub Release 下载'}</strong><span>{progress?.percent != null ? `${progress.percent.toFixed(0)}%` : formatBytes(progress?.bytesDownloaded)}</span></div>
            <progress max="100" value={progress?.percent ?? 0}></progress>
            <p>支持断点续传；中断后重新安装同一版本会继续已保存的进度。</p>
          </div>
        {/if}
      </div>
      <div class="plugin-release-footer">
        <Button variant="outline" disabled={installing} onclick={closeDetail}>返回</Button>
        <Button disabled={!tag || versionsLoading || installing || disabled || releaseRelation === 'older' || releaseRelation === 'current'} onclick={install}>
          {installing ? '正在校验并安装…' : releaseRelation === 'older' ? '不能安装旧版本' : releaseRelation === 'current' ? '当前版本已安装' : permissionReview && permissionReviewKey === `${selected.id}@${tag}` && (permissionReview.added_permissions.length || permissionReview.removed_permissions.length || permissionReview.requires_approval) ? installedVersion ? '确认变更并更新安装包' : '确认安装包' : installedVersion ? '下载并更新' : `安装 ${release?.tag_name ?? ''}`}
        </Button>
      </div>
    </div>
  {:else}
    <div class="plugin-catalog-toolbar">
      <div><strong>线上插件市场</strong><span>安装包直接从作者 GitHub Release 下载</span></div>
      <div class="plugins-toolbar-actions">
        {#if catalog.length}
          <SegmentedControl.Root value={viewMode} onValueChange={(value) => setViewMode(value as ViewMode)} aria-label="插件市场显示方式">
            <SegmentedControl.Item value="card" size="icon" title="卡片视图" aria-label="卡片视图"><LayoutGrid class="h-3.5 w-3.5" /></SegmentedControl.Item>
            <SegmentedControl.Item value="list" size="icon" title="列表视图" aria-label="列表视图"><List class="h-3.5 w-3.5" /></SegmentedControl.Item>
          </SegmentedControl.Root>
        {/if}
        <div class="plugins-search"><Search size={14} aria-hidden="true" /><Input class="pl-8" aria-label="搜索插件" placeholder="搜索插件名称或简介…" bind:value={query} /></div>
        <Button variant="outline" size="sm" disabled={loading || installing} onclick={() => load(true)}><RefreshCw size={14} class={loading ? 'animate-spin' : ''} />刷新市场</Button>
        <Button variant="outline" size="sm" disabled={disabled || loading || installing} onclick={() => { void onlocalinstall(); }}><Upload size={14} />从本地安装</Button>
      </div>
    </div>
    <div class="plugin-catalog-scroll">
      {#if refreshMessage}<p class="version-notice" role="status">{refreshMessage}</p>{/if}
      {#if loading}<div class="plugins-empty" role="status"><RefreshCw size={24} class="animate-spin" />正在加载插件目录…</div>
      {:else if error}<div class="plugins-empty" role="alert"><Puzzle size={28} /><p class="plugins-error">{error}</p><Button variant="outline" size="sm" onclick={() => load()}>重试</Button></div>
      {:else if !catalog.length}<div class="plugins-empty"><Puzzle size={28} /><strong>暂时没有已登记的插件</strong><p>插件中心登记后，可在这里安装最新兼容版本。</p></div>
      {:else if !filtered.length}<div class="plugins-empty"><Search size={28} /><p>没有找到匹配的插件。</p><Button variant="ghost" size="sm" onclick={() => { query = ''; }}>清除搜索</Button></div>
      {:else}
        <div class="plugins-collection" class:card-view={viewMode === 'card'} class:list-view={viewMode === 'list'}>
          {#each filtered as plugin (plugin.id)}
            <article class="plugin-card">
              <div class="plugin-card-heading"><div class="plugin-icon"><Puzzle size={18} /></div><div class="plugin-identity"><strong title={plugin.name}>{plugin.name}</strong><span class="plugin-meta" title={`发布者 ${plugin.publisher.id}`}>发布者 {plugin.publisher.id}</span></div>{#if installedById.has(plugin.id)}<span class="plugin-status authorized" title={`已安装 ${installedById.get(plugin.id)?.version}`}>已安装 {installedById.get(plugin.id)?.version}</span>{/if}</div>
              <p class="plugin-description" title={plugin.description}>{plugin.description}</p>
              {#if installedById.has(plugin.id) && !(plugin.surfaces?.length)}<p class="plugin-foundation">当前市场版本未声明管理页面。</p>{/if}
              <div class="plugin-card-actions">
                <Button size="sm" variant="ghost" class="mr-auto" onclick={async () => {
                  try { await openUrl(plugin.repository); }
                  catch (reason) { error = getAppErrorInfo(reason, '无法打开插件仓库').message; }
                }}><ExternalLink size={13} />仓库</Button>
                {#if installedById.has(plugin.id)}<Button size="sm" variant="outline" disabled={disabled || installing} onclick={onmanage}>管理插件</Button>{/if}
                <Button size="sm" variant={installedById.has(plugin.id) ? 'outline' : 'default'} disabled={disabled || installing} onclick={() => choose(plugin)}><Download size={14} />{installedById.has(plugin.id) ? '版本管理' : '安装最新版'}</Button>
              </div>
            </article>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  p { margin: 0; font-size: 12px; color: var(--muted-foreground); overflow-wrap: anywhere; }
  .error { color: var(--destructive); }
  .version-label { display: block; margin-bottom: 8px; font-size: 12px; font-weight: 500; }
  .release-source, .release-version-summary { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; padding: 10px 12px; border-radius: 8px; background: var(--muted); font-size: 11px; color: var(--muted-foreground); }
  .release-source strong { font-weight: 500; color: var(--foreground); overflow-wrap: anywhere; min-width: 0; }
  .release-version-summary strong { margin-left: auto; color: var(--foreground); }
  .release-version-summary em { padding: 2px 6px; border-radius: 4px; background: color-mix(in srgb, var(--success) 10%, transparent); color: var(--success); font-style: normal; }
  .release-notes { white-space: pre-wrap; overflow-wrap: anywhere; font: inherit; font-size: 12px; max-height: 180px; overflow: auto; padding: 12px; border-radius: 8px; border: 1px solid var(--border); }
  .plugin-foundation { padding: 8px 10px; border-radius: 8px; background: var(--muted); }
  .version-notice { padding: 10px 12px; border-radius: 8px; background: var(--muted); color: var(--foreground); line-height: 1.6; }
  .download-progress { display: grid; gap: 8px; padding: 10px 12px; border: 1px solid color-mix(in srgb, var(--primary) 24%, var(--border)); border-radius: 8px; background: color-mix(in srgb, var(--primary) 6%, var(--card)); }
  .download-progress > div { display: flex; justify-content: space-between; gap: 12px; font-size: 12px; }
  .download-progress progress { width: 100%; accent-color: var(--primary); }
  .permission-change-review { display: grid; gap: 8px; padding: 12px; border: 1px solid color-mix(in srgb, var(--destructive) 34%, var(--border)); border-radius: 8px; background: color-mix(in srgb, var(--destructive) 6%, var(--card)); }
  .permission-change-review ul { display: grid; gap: 6px; margin: 0; padding-left: 18px; font-size: 11px; }
  .permission-change-review li span, .permission-change-review li b, .permission-change-review li code { margin-right: 6px; }
  .permission-change-review code { overflow-wrap: anywhere; }
  .plugin-catalog { display: flex; flex: 1; flex-direction: column; min-height: 0; height: 100%; overflow: hidden; }
  .plugin-catalog-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-shrink: 0; padding: 0 0 12px; border-bottom: 1px solid var(--border); }
  .plugin-catalog-toolbar > div:first-child { display: grid; gap: 2px; min-width: 0; }
  .plugin-catalog-toolbar strong { font-size: 12px; }
  .plugin-catalog-toolbar span { color: var(--muted-foreground); font-size: 10.5px; }
  .plugin-catalog-scroll { flex: 1; min-height: 0; overflow: auto; overscroll-behavior: contain; padding: 12px 2px 2px; }
  .plugin-release-workspace { display: grid; grid-template-rows: auto minmax(0, 1fr) auto; flex: 1; min-height: 0; overflow: hidden; }
  .plugin-release-nav { display: flex; align-items: center; gap: 8px; padding-bottom: 10px; border-bottom: 1px solid var(--border); }
  .plugin-release-nav > span { color: var(--muted-foreground); font-size: 11px; }
  .plugin-release-scroll { display: flex; flex-direction: column; gap: 12px; min-height: 0; overflow: auto; overscroll-behavior: contain; padding: 14px 4px; }
  .plugin-release-hero { display: flex; align-items: center; gap: 10px; }
  .plugin-release-hero > div:last-child { display: grid; gap: 2px; min-width: 0; }
  .plugin-release-hero strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 14px; }
  .plugin-release-hero span { color: var(--muted-foreground); font-size: 11px; }
  .plugin-release-footer { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding-top: 10px; border-top: 1px solid var(--border); }
  .plugins-empty.compact { flex: 0; min-height: 120px; }
  @media (max-width: 760px) {
    .plugin-catalog-toolbar { align-items: stretch; flex-direction: column; }
    .plugin-catalog-toolbar :global(.plugins-toolbar-actions) { width: 100%; flex-wrap: wrap; }
    .plugin-catalog-toolbar :global(.plugins-search) { flex: 1; width: auto; min-width: 150px; }
  }
</style>
