<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import { confirm } from '@tauri-apps/plugin-dialog';
  import { updater } from '$lib/services/updater.svelte';
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
  let installingTag = $state<string | null>(null);

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

  async function installRelease(release: AppRelease) {
    const action = relation(release);
    if (action === 'current' || installingTag || updater.downloading) return;

    if (action === 'rollback') {
      const accepted = await confirm(
        `将从 v${appVersion} 回退到 v${release.version}。请先备份应用数据；如果新版本执行过不兼容的配置迁移，旧版本可能无法直接读取。是否继续？`,
        { title: '确认回退应用版本', kind: 'warning' },
      );
      if (!accepted) return;
    }

    installingTag = release.tagName;
    const selected = await updater.selectRelease(release);
    if (selected) await updater.downloadAndInstall();
    installingTag = null;
  }

  function formatDate(value: string | null): string {
    if (!value) return '发布时间未知';
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? '发布时间未知' : date.toLocaleDateString('zh-CN');
  }
</script>

<div class="versions-root desk-card">
  <header>
    <div>
      <h2>版本管理</h2>
      <p>选择测试版、预发版或历史版本。日常检查更新仍在“关于”页面。</p>
    </div>
    <span class="current-version">当前 v{appVersion}</span>
  </header>

  <div class="separator"></div>

  <section>
    <div class="tools">
      <div class="search-wrap">
        <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" aria-hidden="true">
          <circle cx="6" cy="6" r="4" />
          <path d="M9 9l3 3" />
        </svg>
        <input bind:value={query} type="search" placeholder="检索版本号，例如 0.0.17" aria-label="检索应用版本" />
      </div>
      <button class="refresh" onclick={loadReleases} disabled={loading}>{loading ? '刷新中…' : '刷新'}</button>
    </div>

    <div class="channel-tabs" role="tablist" aria-label="应用发布渠道">
      {#each [
        { id: 'stable', label: '正式版' },
        { id: 'preview', label: '预发版' },
        { id: 'test', label: '测试版' },
      ] as item}
        <button
          class:active={channel === item.id}
          onclick={() => (channel = item.id as AppReleaseChannel)}
          role="tab"
          aria-selected={channel === item.id}
        >{item.label}</button>
      {/each}
    </div>

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
                {#if action === 'current'}<span>当前版本</span>{/if}
              </div>
              <div class="meta">
                <span>{formatDate(release.publishedAt)}</span>
                {#if release.releaseUrl}<a href={release.releaseUrl} target="_blank" rel="noopener noreferrer">发布说明</a>{/if}
              </div>
            </div>
            <button
              class="install"
              class:rollback={action === 'rollback'}
              onclick={() => installRelease(release)}
              disabled={action === 'current' || installingTag !== null || updater.downloading}
            >
              {#if installingTag === release.tagName}
                {updater.downloading ? '安装中…' : '准备中…'}
              {:else if action === 'current'}
                已安装
              {:else if action === 'rollback'}
                回退到此版本
              {:else}
                安装
              {/if}
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>

<style>
  .versions-root { display: flex; flex: 1; min-height: 0; flex-direction: column; overflow: hidden; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 12px; }
  h2 { margin: 0; color: var(--foreground); font-size: 15px; font-weight: 700; }
  header p { margin: 3px 0 0; color: var(--muted-foreground); font-size: 11px; line-height: 1.45; }
  .current-version { flex-shrink: 0; padding: 3px 8px; border-radius: 5px; background: var(--muted); color: var(--muted-foreground); font-family: var(--font-mono); font-size: 11px; font-weight: 600; }
  .separator { height: 1px; margin: 0 12px; background: var(--border); }
  section { display: flex; flex: 1; min-height: 0; flex-direction: column; gap: 10px; padding: 12px; }
  .tools { display: flex; align-items: center; gap: 6px; }
  .search-wrap { display: flex; align-items: center; flex: 1; gap: 6px; min-width: 0; height: 32px; padding: 0 9px; border: 1px solid var(--border); border-radius: 7px; color: var(--muted-foreground); }
  input { flex: 1; min-width: 0; border: none; outline: none; background: transparent; color: var(--foreground); font-size: 11px; }
  input::placeholder { color: var(--muted-foreground); }
  .refresh, .channel-tabs button, .install { border: 1px solid var(--border); border-radius: 6px; background: var(--background); color: var(--foreground); cursor: pointer; font-size: 11px; }
  .refresh { height: 32px; padding: 0 10px; color: var(--muted-foreground); }
  .channel-tabs { display: grid; grid-template-columns: repeat(3, 1fr); gap: 4px; padding: 3px; border-radius: 8px; background: var(--muted); }
  .channel-tabs button { padding: 5px 8px; border-color: transparent; background: transparent; color: var(--muted-foreground); }
  .channel-tabs button.active { border-color: var(--border); background: var(--background); color: var(--foreground); font-weight: 600; }
  .release-list { display: flex; flex: 1; min-height: 0; flex-direction: column; overflow-x: hidden; overflow-y: auto; border: 1px solid var(--border); border-radius: 8px; }
  .release-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 50px; padding: 8px 10px; }
  .release-row + .release-row { border-top: 1px solid var(--border); }
  .release-row.selected { background: color-mix(in srgb, var(--primary) 7%, transparent); }
  .version-line, .meta { display: flex; align-items: center; gap: 7px; }
  .version-line strong { font-family: var(--font-mono); font-size: 11.5px; }
  .version-line > span { padding: 1px 5px; border-radius: 999px; background: var(--muted); color: var(--muted-foreground); font-size: 9px; }
  .meta { margin-top: 3px; color: var(--muted-foreground); font-size: 9.5px; }
  .meta a { color: var(--primary); text-decoration: none; }
  .install { min-width: 94px; padding: 5px 9px; color: var(--primary); font-weight: 600; }
  .install.rollback { color: #b45309; }
  button:disabled { cursor: not-allowed; opacity: 0.45; }
  .message { padding: 18px 10px; border: 1px dashed var(--border); border-radius: 8px; color: var(--muted-foreground); font-size: 11px; text-align: center; }
  .message.error { color: var(--destructive); }
</style>
