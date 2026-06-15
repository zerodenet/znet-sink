<script lang="ts">
  import { open as openFile } from '@tauri-apps/plugin-dialog';
  import { openUrl as openLink } from '@tauri-apps/plugin-opener';
  import { AlertTriangle, Download, FolderOpen, RefreshCcw, Save, X } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Badge } from '$lib/components/ui/badge';
  import { Switch } from '$lib/components/ui/switch';
  import {
    getAppConfig,
    getCoreConfigSnapshot,
    updateAppConfig,
    getGuiCoreHealth,
  } from '$lib/services/core';
  import {
    listKernelVersions,
    installKernelVersion,
    detectKernelVersion,
    onDownloadProgress,
  } from '$lib/services/kernel-version';
  import type { AppConfig } from '$lib/types/app-config';
  import type { CoreKernelInfo } from '$lib/types/core';
  import type {
    ReleaseChannel,
    KernelRelease,
    KernelVersionList,
    KernelDownloadProgress,
    KernelInstallResult,
  } from '$lib/types/kernel-version';
  import DraggableModal from '$lib/components/DraggableModal.svelte';
  import { success, warning } from '$lib/services/toast.svelte';

  const FALLBACK_DOWNLOAD_URL = 'https://github.com/zerodenet/zero/releases/latest';
  const CHANNEL_LABELS: Record<ReleaseChannel, string> = {
    stable: '稳定版',
    beta: '测试版',
    nightly: '开发版',
  };

  let appConfig = $state<AppConfig | null>(null);
  let kernelInfo = $state<CoreKernelInfo | null>(null);
  let executablePathDraft = $state('');
  let loading = $state(false);
  let saving = $state(false);
  let message = $state<string | null>(null);

  // Version management state
  let installedVersion = $state<string | null>(null);
  let runningVersion = $state<string | null>(null);
  let versionManagerOpen = $state(false);
  let versionList = $state<KernelVersionList | null>(null);
  let versionListLoading = $state(false);
  let activeChannel = $state<ReleaseChannel>('stable');
  let downloadProgress = $state<KernelDownloadProgress | null>(null);
  let installBusy = $state(false);
  let installingVersion = $state<string | null>(null);
  let installResult = $state<KernelInstallResult | null>(null);

  const kernelName = $derived(kernelInfo?.kernel ?? appConfig?.core.kernel ?? 'zero');
  const hasExecutable = $derived(Boolean(kernelInfo?.executableExists));
  const recommendedInstallDir = $derived(kernelInfo?.recommendedInstallDir ?? '');
  const pathDirty = $derived(executablePathDraft.trim() !== (kernelInfo?.executablePath ?? ''));
  const currentVersion = $derived(runningVersion ?? installedVersion ?? null);

  const channelFilteredVersions = $derived(
    (versionList?.versions ?? []).filter((v) => v.channel === activeChannel),
  );

  async function refresh() {
    loading = true;
    message = null;
    try {
      const [config, info] = await Promise.all([
        getAppConfig(),
        getCoreConfigSnapshot(),
      ]);
      appConfig = config;
      kernelInfo = info;
      executablePathDraft = info.executablePath ?? '';

      // Detect installed version
      try {
        const detect = await detectKernelVersion();
        installedVersion = detect.version ?? null;
      } catch {
        installedVersion = null;
      }

      // Get running version from health API
      try {
        const health = await getGuiCoreHealth();
        runningVersion = health.engineVersion ? stripV(health.engineVersion) : null;
      } catch {
        runningVersion = null;
      }
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  async function saveExecutablePath() {
    if (!appConfig) return;
    saving = true;
    message = null;
    try {
      const updated = await updateAppConfig({
        core: {
          executablePath: executablePathDraft.trim() || null,
        },
      });
      appConfig = updated;
      message = executablePathDraft.trim() ? '已保存内核路径' : '已清空内核路径';
      await refresh();
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    } finally {
      saving = false;
    }
  }

  async function toggleDownloadProxyAuto(value: boolean) {
    if (!appConfig) return;
    saving = true;
    message = null;
    try {
      const updated = await updateAppConfig({
        core: { downloadProxyAuto: value },
      });
      appConfig = updated;
      message = value
        ? '内核下载已启用跟随系统代理（HTTPS_PROXY / HTTP_PROXY）'
        : '内核下载已切换为直连（绕过所有代理）';
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    } finally {
      saving = false;
    }
  }

  async function chooseExecutablePath() {
    const selected = await openFile({
      title: '选择内核可执行文件',
      defaultPath: executablePathDraft || recommendedInstallDir || undefined,
      multiple: false,
      directory: false,
    });
    if (typeof selected === 'string' && selected.trim()) {
      executablePathDraft = selected.trim();
      await saveExecutablePath();
    }
  }

  async function openVersionManager() {
    versionManagerOpen = true;
    installResult = null;
    downloadProgress = null;
    await loadVersions();
  }

  function closeVersionManager() {
    if (installBusy) return;
    versionManagerOpen = false;
    versionList = null;
    downloadProgress = null;
    installResult = null;
  }

  async function loadVersions() {
    versionListLoading = true;
    try {
      versionList = await listKernelVersions();
    } catch (error) {
      warning(error instanceof Error ? error.message : '获取版本列表失败');
    } finally {
      versionListLoading = false;
    }
  }

  async function handleInstallVersion(release: KernelRelease) {
    if (!release.assetDownloadUrl) {
      warning('该版本没有当前平台的安装包');
      return;
    }

    installBusy = true;
    installingVersion = release.version;
    downloadProgress = null;
    installResult = null;

    const unlisten = await onDownloadProgress((progress) => {
      downloadProgress = progress;
    });

    try {
      const result = await installKernelVersion(
        release.version,
        release.assetDownloadUrl,
        release.checksumSha256 ?? undefined,
        recommendedInstallDir || undefined,
      );
      installResult = result;
      if (result.success) {
        executablePathDraft = result.executablePath;
        await saveExecutablePath();
        success(`内核 ${result.version} 安装成功`);
        await refresh();
      }
    } catch (error) {
      warning(error instanceof Error ? error.message : '安装失败');
    } finally {
      unlisten();
      installBusy = false;
      installingVersion = null;
    }
  }

  function formatBytes(value?: number): string {
    if (!value || value <= 0) return '—';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let size = value;
    let unit = 0;
    while (size >= 1024 && unit < units.length - 1) {
      size /= 1024;
      unit += 1;
    }
    return `${size.toFixed(size >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`;
  }

  function formatDate(value?: number): string {
    if (!value) return '—';
    return new Intl.DateTimeFormat('zh-CN', {
      dateStyle: 'medium',
      timeStyle: 'short',
    }).format(value);
  }

  /** Strip leading 'v' so all version comparisons are prefix-free. */
  function stripV(v: string): string {
    return v.startsWith('v') ? v.slice(1) : v;
  }

  function isCurrentVersion(version: string): boolean {
    if (currentVersion) return stripV(version) === currentVersion;
    return false;
  }

  // 标记每个渠道的第一个（最新）版本
  function isLatestInChannel(release: KernelRelease): boolean {
    const channelVersions = channelFilteredVersions;
    if (channelVersions.length === 0) return false;
    return channelVersions[0].version === release.version;
  }

  $effect(() => {
    refresh();
  });
</script>

<div class="panel">
  <div class="header">
    <div class="heading">
      <div class="title-row">
        <div class="title">内核配置</div>
        {#if currentVersion}
          <Badge variant="secondary">v{currentVersion}</Badge>
        {:else if hasExecutable}
          <Badge variant="outline">已安装</Badge>
        {:else}
          <Badge variant="outline">未安装</Badge>
        {/if}
      </div>
      <div class="desc">
        管理自研内核版本，支持 stable / beta / nightly 渠道。
      </div>
    </div>

    <div class="actions">
      <Button variant={hasExecutable ? 'outline' : 'default'} size="sm" onclick={openVersionManager} disabled={loading || saving}>
        <Download class="h-3.5 w-3.5" />
        <span>版本管理</span>
      </Button>
      <Button variant="outline" size="sm" onclick={chooseExecutablePath} disabled={loading || saving}>
        <FolderOpen class="h-3.5 w-3.5" />
        <span>选择文件</span>
      </Button>
      <Button variant="ghost" size="icon-sm" onclick={refresh} disabled={loading}>
        <RefreshCcw class="h-3.5 w-3.5" />
      </Button>
    </div>
  </div>

  {#if !kernelInfo}
    <div class="loading">加载内核信息中...</div>
  {:else if !hasExecutable}
    <div class="empty-state">
      <div class="empty-icon">
        <Download class="h-5 w-5" />
      </div>
      <div class="empty-title">当前没有可用内核</div>
      <div class="empty-desc">
        通过版本管理安装内核，或手动选择已有的可执行文件。
      </div>
      <div class="empty-actions">
        <Button onclick={openVersionManager} disabled={loading || saving}>
          <Download class="h-3.5 w-3.5" />
          <span>版本管理</span>
        </Button>
        <Button variant="outline" onclick={chooseExecutablePath} disabled={loading || saving}>
          <FolderOpen class="h-3.5 w-3.5" />
          <span>选择文件</span>
        </Button>
      </div>
    </div>
  {:else}
    <div class="summary">
      <div class="summary-grid">
        <div class="field">
          <div class="field-label">内核标识</div>
          <Input value={kernelName} readonly class="mono" />
        </div>

        <div class="field">
          <div class="field-label">当前版本</div>
          <div class="value mono">{currentVersion ?? '—'}</div>
        </div>

        <div class="field">
          <div class="field-label">可执行文件</div>
          <div class="path-row">
            <Input bind:value={executablePathDraft} class="mono" placeholder="请选择内核可执行文件" />
            <Button variant="outline" size="sm" onclick={chooseExecutablePath} disabled={loading || saving}>
              <FolderOpen class="h-3.5 w-3.5" />
              <span>选择</span>
            </Button>
            <Button size="sm" onclick={saveExecutablePath} disabled={loading || saving || !pathDirty}>
              <Save class="h-3.5 w-3.5" />
              <span>保存</span>
            </Button>
          </div>
        </div>

        <div class="field">
          <div class="field-label">文件名</div>
          <div class="value mono">{kernelInfo.fileName ?? '—'}</div>
        </div>

        <div class="field">
          <div class="field-label">文件大小</div>
          <div class="value mono">{formatBytes(kernelInfo.sizeBytes)}</div>
        </div>

        <div class="field">
          <div class="field-label">最后更新</div>
          <div class="value mono">{formatDate(kernelInfo.modifiedAtUnixMs)}</div>
        </div>
      </div>

      {#if kernelInfo.warnings.length}
        <div class="warning">
          <div class="warning-title">
            <AlertTriangle class="h-3.5 w-3.5" />
            <span>提示</span>
          </div>
          {#each kernelInfo.warnings as warn}
            <div class="warning-line">{warn}</div>
          {/each}
        </div>
      {/if}

      <div class="proxy-toggle-row">
        <div class="proxy-toggle-text">
          <span class="proxy-toggle-title">下载跟随系统代理</span>
          <span class="proxy-toggle-desc">
            内核下载与版本列表请求是否使用 HTTPS_PROXY / HTTP_PROXY 环境变量。关闭则直连（绕过所有代理），适用于代理本身不可用或下载源可直连的场景。
          </span>
        </div>
        <Switch
          checked={appConfig?.core.downloadProxyAuto ?? true}
          disabled={loading || saving}
          onCheckedChange={toggleDownloadProxyAuto}
        />
      </div>
    </div>
  {/if}

  {#if message}
    <div class="message">{message}</div>
  {/if}
</div>

<DraggableModal
  title="内核版本管理"
  open={versionManagerOpen}
  onClose={closeVersionManager}
  closeDisabled={installBusy}
  width="min(560px, 90vw)"
>
  {#snippet headerActions()}
    <Button variant="ghost" size="icon-sm" onclick={loadVersions} disabled={versionListLoading || installBusy}>
      <RefreshCcw class="h-3.5 w-3.5" />
    </Button>
  {/snippet}

    <div class="channel-tabs">
      {#each (['stable', 'beta', 'nightly'] as ReleaseChannel[]) as ch}
        <button
          class="channel-tab"
          class:active={activeChannel === ch}
          onclick={() => { activeChannel = ch; installResult = null; downloadProgress = null; }}
          disabled={installBusy}
        >
          {CHANNEL_LABELS[ch]}
        </button>
      {/each}
    </div>

    {#if installResult?.success}
      <div class="install-success">
        <svg width="16" height="16" viewBox="0 0 10 10" fill="none" stroke="#22C55E" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="1.5 5 4 7.5 8.5 2.5"/></svg>
        <div class="install-success-text">
          <span class="font-semibold">安装成功</span>
          <span class="text-xs text-muted-foreground">版本 {installResult.version}</span>
          <span class="text-xs text-muted-foreground">{installResult.executablePath}</span>
          {#if installResult.checksumVerified}
            <Badge variant="secondary" class="text-xs">SHA256 已校验</Badge>
          {/if}
        </div>
      </div>
    {:else if downloadProgress && installBusy}
      <div class="progress-container">
        <div class="progress-label">
          下载中 v{downloadProgress.version}...
          {downloadProgress.percent ? `${downloadProgress.percent.toFixed(1)}%` : ''}
        </div>
        <div class="progress-track">
          <div class="progress-fill" style="width: {downloadProgress.percent ?? 0}%"></div>
        </div>
        <div class="progress-detail">
          {formatBytes(downloadProgress.bytesDownloaded)}
          {downloadProgress.bytesTotal ? `/ ${formatBytes(downloadProgress.bytesTotal)}` : ''}
        </div>
      </div>
    {:else if versionListLoading}
      <div class="loading">获取版本列表中...</div>
    {:else if channelFilteredVersions.length === 0}
      <div class="empty-versions">该渠道暂无可用版本</div>
    {:else}
      <div class="version-list">
        {#each channelFilteredVersions as release (release.version)}
          <div class="version-row" class:current={isCurrentVersion(release.version)}>
            <div class="version-info">
              <span class="version-tag">v{release.version}</span>
              {#if isLatestInChannel(release)}
                <Badge variant="default" class="text-xs">最新</Badge>
              {/if}
              {#if isCurrentVersion(release.version)}
                <Badge variant="secondary" class="text-xs">当前</Badge>
              {/if}
            </div>
            <div class="version-meta">
              <span class="version-date">{formatDate(release.publishedAtUnixMs)}</span>
              {#if release.assetSizeBytes}
                <span class="version-size">{formatBytes(release.assetSizeBytes)}</span>
              {/if}
            </div>
            <div class="version-actions">
              {#if release.releaseNotesUrl}
                <button class="link-btn" onclick={() => openLink(release.releaseNotesUrl!)} title="查看更新说明">
                  <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M8.5 7v2.5h-7v-7h2.5"/><path d="M9.5 1.5h-4v4M10 1L5.5 5.5"/></svg>
                </button>
              {/if}
              <Button
                size="sm"
                onclick={() => handleInstallVersion(release)}
                disabled={installBusy || !release.assetDownloadUrl}
              >
                {#if installingVersion === release.version}
                  <span>安装中...</span>
                {:else if isCurrentVersion(release.version)}
                  <span>重装</span>
                {:else}
                  <Download class="h-3.5 w-3.5" />
                  <span>安装</span>
                {/if}
              </Button>
            </div>
          </div>
        {/each}
      </div>
    {/if}

  {#snippet footer()}
    <Button variant="outline" onclick={closeVersionManager} disabled={installBusy}>
      {installResult?.success ? '关闭' : '取消'}
    </Button>
    <button class="link-btn" onclick={() => openLink(FALLBACK_DOWNLOAD_URL)} disabled={installBusy} title="在浏览器中打开下载页">
      <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M8.5 7v2.5h-7v-7h2.5"/><path d="M9.5 1.5h-4v4M10 1L5.5 5.5"/></svg>
      <span>手动下载</span>
    </button>
  {/snippet}
</DraggableModal>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .heading {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .title {
    font-size: 13px;
    font-weight: 700;
    color: var(--foreground);
  }

  .desc {
    font-size: 11.5px;
    color: var(--muted-foreground);
    line-height: 1.5;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .loading {
    padding: 16px 0;
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .empty-state,
  .summary,
  .message {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--card);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 24px 16px;
    text-align: center;
  }

  .empty-icon {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--muted-foreground);
    background: var(--muted);
  }

  .empty-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--foreground);
  }

  .empty-desc {
    font-size: 12px;
    color: var(--muted-foreground);
    line-height: 1.5;
  }

  .empty-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }

  .summary {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
  }

  .summary-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .field-label {
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .path-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 8px;
    align-items: center;
  }

  .value {
    font-size: 12px;
    color: var(--foreground);
  }

  .mono {
    font-family: var(--font-mono);
  }

  .warning {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid rgba(245, 158, 11, 0.2);
    background: rgba(245, 158, 11, 0.08);
  }

  .warning-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 700;
    color: var(--warning);
  }

  .warning-line {
    font-size: 12px;
    color: var(--warning);
    line-height: 1.4;
  }

  .proxy-toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--muted);
  }

  .proxy-toggle-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .proxy-toggle-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--foreground);
  }

  .proxy-toggle-desc {
    font-size: 11px;
    color: var(--muted-foreground);
    line-height: 1.5;
  }

  .message {
    padding: 10px 12px;
    font-size: 12px;
    color: var(--foreground);
  }

  /* Modal content styles (layout provided by DraggableModal) */

  /* Channel tabs */
  .channel-tabs {
    display: flex;
    gap: 2px;
    background: var(--muted);
    border-radius: 8px;
    padding: 3px;
  }

  .channel-tab {
    flex: 1;
    padding: 6px 0;
    border: none;
    background: transparent;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .channel-tab:hover:not(:disabled) {
    color: var(--foreground);
  }

  .channel-tab.active {
    background: var(--card);
    color: var(--foreground);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
  }

  .channel-tab:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* Version list */
  .version-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 340px;
    overflow-y: auto;
    padding: 2px 0;
  }

  .version-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 6px;
    transition: background 0.12s ease;
  }

  .version-row:hover {
    background: var(--muted);
  }

  .version-row.current {
    background: rgba(34, 197, 94, 0.06);
    border: 1px solid rgba(34, 197, 94, 0.12);
  }

  .version-info {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 100px;
  }

  .version-tag {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 600;
    color: var(--foreground);
  }

  .version-meta {
    flex: 1;
    display: flex;
    gap: 10px;
    min-width: 0;
  }

  .version-date {
    font-size: 11px;
    color: var(--muted-foreground);
  }

  .version-size {
    font-size: 11px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
  }

  .version-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .empty-versions {
    padding: 24px 0;
    text-align: center;
    font-size: 12px;
    color: var(--muted-foreground);
  }

  /* Progress bar */
  .progress-container {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--muted);
  }

  .progress-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--foreground);
  }

  .progress-track {
    height: 6px;
    border-radius: 3px;
    background: var(--border);
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    border-radius: 3px;
    background: var(--primary);
    transition: width 0.2s ease;
  }

  .progress-detail {
    font-size: 11px;
    color: var(--muted-foreground);
  }

  /* Install success */
  .install-success {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px;
    border-radius: 8px;
    background: rgba(34, 197, 94, 0.06);
    border: 1px solid rgba(34, 197, 94, 0.15);
  }

  .install-success-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 13px;
  }

  /* Link button */
  .link-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 12px;
    cursor: pointer;
    padding: 4px 6px;
    border-radius: 4px;
    transition: color 0.12s ease, background 0.12s ease;
  }

  .link-btn:hover:not(:disabled) {
    color: var(--foreground);
    background: var(--muted);
  }

  .link-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  :global(.sr-only) {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  @media (max-width: 720px) {
    .header {
      flex-direction: column;
    }

    .actions {
      width: 100%;
      justify-content: flex-start;
      flex-wrap: wrap;
    }

    .summary-grid {
      grid-template-columns: 1fr;
    }

    .path-row {
      grid-template-columns: 1fr;
    }

    .version-row {
      flex-wrap: wrap;
    }

    .version-meta {
      order: 3;
      width: 100%;
    }
  }
</style>
