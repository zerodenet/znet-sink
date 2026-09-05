<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import { confirm } from '@tauri-apps/plugin-dialog';
  import { CircleCheck, Download, LoaderCircle, Power, RefreshCcw, RotateCcw, Search } from '@lucide/svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import * as Tabs from '$lib/components/AppTabs';
  import { formatBytes, updater } from '$lib/services/updater.svelte';
  import {
    compareAppVersions,
    fetchAppReleases,
    type AppRelease,
    type AppReleaseChannel,
  } from '$lib/services/app-update-policy';

  let appVersion = $state('0.0.1');
  let releases = $state<AppRelease[]>([]);
  let channel = $state<AppReleaseChannel>('stable');
  let query = $state('');
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let installError = $state<string | null>(null);
  let workingTag = $state<string | null>(null);

  const visibleReleases = $derived.by(() => {
    const needle = query.trim().toLowerCase().replace(/^v/, '');
    return releases.filter((release) => (
      release.channel === channel
      && (!needle || release.version.toLowerCase().includes(needle))
    ));
  });

  $effect(() => {
    void loadCurrentVersion();
    void loadReleases();
  });

  async function loadCurrentVersion() {
    try {
      appVersion = await getVersion();
    } catch {
      appVersion = '0.0.1';
    }
  }

  async function loadReleases() {
    loading = true;
    loadError = null;
    try {
      releases = await fetchAppReleases();
    } catch (error) {
      loadError = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  function relation(release: AppRelease): 'upgrade' | 'current' | 'rollback' {
    const comparison = compareAppVersions(release.version, appVersion);
    if (comparison > 0) return 'upgrade';
    if (comparison < 0) return 'rollback';
    return 'current';
  }

  function isDownloaded(release: AppRelease): boolean {
    return updater.selectedTag === release.tagName && updater.readyToInstall;
  }

  async function handleReleaseAction(release: AppRelease) {
    const action = relation(release);
    if (action === 'current' || workingTag || updater.busy || updater.restartRequired) return;

    installError = null;

    if (isDownloaded(release)) {
      const accepted = await confirm(
        `v${release.version} 已下载完成。安装后应用将立即重启，是否现在安装？`,
        { title: '确认安装应用版本', kind: 'info' },
      );
      if (!accepted) return;

      workingTag = release.tagName;
      try {
        const installed = await updater.installUpdate();
        if (!installed) installError = updater.lastError ?? `无法安装 v${release.version}`;
      } finally {
        workingTag = null;
      }
      return;
    }

    const accepted = await confirm(
      action === 'rollback'
        ? `将下载 v${release.version}，供后续回退安装。请先备份应用数据；下载完成后不会自动安装或重启。是否继续？`
        : `将下载 v${release.version} 的升级包。下载完成后不会自动安装或重启，可由你确认安装时间。是否继续？`,
      { title: action === 'rollback' ? '确认下载回退版本' : '确认下载升级版本', kind: action === 'rollback' ? 'warning' : 'info' },
    );
    if (!accepted) return;

    workingTag = release.tagName;
    try {
      const selected = await updater.selectRelease(release);
      if (!selected) {
        installError = updater.lastError ?? `无法准备 v${release.version} 的安装包`;
        return;
      }
      const downloaded = await updater.downloadUpdate();
      if (!downloaded) installError = updater.lastError ?? `无法下载 v${release.version}`;
    } finally {
      workingTag = null;
    }
  }

  function formatDate(value: string | null): string {
    if (!value) return '发布时间未知';
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? '发布时间未知' : date.toLocaleDateString('zh-CN');
  }
</script>

<Tabs.Root bind:value={channel} class="versions-root">
  <header>
    <div>
      <h2>版本管理</h2>
      <p>下载指定版本；安装前会再次确认。</p>
    </div>
    <Badge variant="secondary" class="current-version">当前 v{appVersion}</Badge>
  </header>

  <div class="separator"></div>

  <section>
    <div class="tools">
      <div class="search-wrap">
        <Search class="search-icon" aria-hidden="true" />
        <Input bind:value={query} type="search" placeholder="检索版本号，例如 0.0.17" aria-label="检索应用版本" class="search-input" />
      </div>
      <Button variant="outline" size="sm" onclick={loadReleases} disabled={loading}>
        <RefreshCcw class={loading ? 'animate-spin' : undefined} />
        {loading ? '刷新中…' : '刷新'}
      </Button>
    </div>

    <Tabs.List class="channel-tabs" aria-label="应用发布渠道">
      {#each [
        { id: 'stable', label: '正式版' },
        { id: 'preview', label: '预发版' },
        { id: 'test', label: '测试版' },
      ] as item}
        <Tabs.Trigger class="channel-button" value={item.id}>{item.label}</Tabs.Trigger>
      {/each}
    </Tabs.List>

    {#if updater.restartRequired}
      <div class="message" role="status">
        更新已安装，请重启应用。{updater.lastError ?? ''}
        <Button size="sm" onclick={() => updater.restartApp()} disabled={updater.busy}>重启应用</Button>
      </div>
    {/if}

    {#if installError}
      <div class="message error" role="alert">{installError}</div>
    {/if}

    {#if updater.downloading && updater.selectedTag}
      <div class="download-progress" role="status" aria-live="polite">
        <div class="progress-copy">
          <span>{updater.downloadLabel}</span>
          <span>{updater.progressPct != null ? `${updater.progressPct}%` : '计算大小中'} · {formatBytes(updater.downloaded)}{updater.total != null ? ` / ${formatBytes(updater.total)}` : ''}</span>
        </div>
        <div
          class="progress-track"
          role="progressbar"
          aria-label="版本下载进度"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={updater.progressPct ?? undefined}
        >
          <div class:indeterminate={updater.progressPct == null} class="progress-value" style={updater.progressPct != null ? `width: ${updater.progressPct}%` : ''}></div>
        </div>
      </div>
    {/if}

    {#if loadError}
      <div class="message error">{loadError}</div>
    {:else if loading && releases.length === 0}
      <div class="message">正在读取可用版本…</div>
    {:else if visibleReleases.length === 0}
      <div class="message">{query.trim() ? '没有匹配的版本' : '该渠道暂无带签名更新清单的版本'}</div>
    {:else}
      <div class="release-list">
        {#each visibleReleases as release (release.tagName)}
          {@const action = relation(release)}
          <div class="release-row" class:selected={updater.selectedTag === release.tagName}>
            <div>
              <div class="version-line">
                <strong>v{release.version}</strong>
                {#if action === 'current'}<Badge variant="secondary" class="release-badge">当前版本</Badge>{/if}
                {#if isDownloaded(release)}<Badge variant="outline" class="release-badge"><CircleCheck />已下载</Badge>{/if}
              </div>
              <div class="meta">
                <span>{formatDate(release.publishedAt)}</span>
                {#if release.releaseUrl}<a href={release.releaseUrl} target="_blank" rel="noopener noreferrer">发布说明</a>{/if}
              </div>
            </div>
            <Button
              variant={action === 'current' ? 'outline' : 'default'}
              size="sm"
              class="install"
              onclick={() => handleReleaseAction(release)}
              disabled={action === 'current' || workingTag !== null || updater.busy || updater.restartRequired}
            >
              {#if workingTag === release.tagName}
                <LoaderCircle class="animate-spin" />
                {updater.downloading ? '下载中…' : isDownloaded(release) ? '安装中…' : '准备中…'}
              {:else if action === 'current'}
                已安装
              {:else if isDownloaded(release)}
                <Power />
                安装重启
              {:else if action === 'rollback'}
                <RotateCcw />
                下载回退
              {:else}
                <Download />
                下载升级
              {/if}
            </Button>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</Tabs.Root>

<style>
  :global(.versions-root) { display: flex; flex: 1; min-height: 0; flex-direction: column; gap: 0; overflow: hidden; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 2px 0 10px; }
  h2 { margin: 0; color: var(--foreground); font-size: 13px; font-weight: 500; }
  header p { margin: 2px 0 0; color: var(--muted-foreground); font-size: 11.5px; line-height: 1.45; opacity: 0.8; }
  :global(.current-version) { flex-shrink: 0; font-family: var(--font-mono); font-size: 11px; }
  .separator { height: 1px; background: var(--border); }
  section { display: flex; flex: 1; min-height: 0; flex-direction: column; gap: 10px; padding: 10px 0 0; }
  .tools { display: flex; align-items: center; gap: 8px; }
  .search-wrap { position: relative; flex: 1; min-width: 0; }
  :global(.search-icon) { position: absolute; top: 50%; left: 9px; z-index: 1; width: 14px; height: 14px; transform: translateY(-50%); color: var(--muted-foreground); pointer-events: none; }
  :global(.search-input) { height: var(--control-height); padding-left: 30px; font-size: 11px; }
  :global(.channel-tabs) { display: grid; grid-template-columns: repeat(3, 1fr); width: 100%; }
  :global(.channel-button) { width: 100%; font-size: 12px; }
  .release-list { display: flex; flex: 1; min-height: 0; flex-direction: column; overflow-x: hidden; overflow-y: auto; border: 1px solid var(--border); border-radius: 8px; }
  .release-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 50px; padding: 8px 10px; }
  .release-row + .release-row { border-top: 1px solid var(--border); }
  .release-row.selected { background: color-mix(in srgb, var(--primary) 7%, transparent); }
  .version-line, .meta { display: flex; align-items: center; gap: 7px; }
  .version-line strong { font-family: var(--font-mono); font-size: 11.5px; }
  :global(.release-badge) { height: 17px; padding-inline: 6px; font-size: 9px; }
  .meta { margin-top: 3px; color: var(--muted-foreground); font-size: 9.5px; }
  .meta a { color: var(--primary); text-decoration: none; }
  :global(.install) { min-width: 108px; font-size: 11px; }
  .download-progress { padding: 9px 10px; border: 1px solid color-mix(in srgb, var(--primary) 24%, var(--border)); border-radius: 8px; background: color-mix(in srgb, var(--primary) 6%, var(--card)); }
  .progress-copy { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 7px; color: var(--muted-foreground); font-size: 10px; }
  .progress-copy span:first-child { color: var(--foreground); font-weight: 600; }
  .progress-track { height: 5px; overflow: hidden; border-radius: 999px; background: var(--muted); }
  .progress-value { height: 100%; border-radius: inherit; background: var(--primary); transition: width 160ms ease; }
  .progress-value.indeterminate { width: 35%; animation: progress-slide 1.1s ease-in-out infinite; }
  .message { padding: 18px 10px; border: 1px dashed var(--border); border-radius: 8px; color: var(--muted-foreground); font-size: 11px; text-align: center; }
  .message.error { color: var(--destructive); }
  @keyframes progress-slide { from { transform: translateX(-110%); } to { transform: translateX(310%); } }
</style>
