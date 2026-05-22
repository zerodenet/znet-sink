<script lang="ts">
  import { open as openFile } from '@tauri-apps/plugin-dialog';
  import { openUrl as openLink } from '@tauri-apps/plugin-opener';
  import { AlertTriangle, Download, FolderOpen, RefreshCcw, Save, X } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Badge } from '$lib/components/ui/badge';
  import { getAppConfig, getCoreConfigSnapshot, updateAppConfig } from '$lib/services/core';
  import type { AppConfig } from '$lib/types/app-config';
  import type { CoreKernelInfo } from '$lib/types/core';

  const FALLBACK_DOWNLOAD_URL = 'https://github.com/zerdenet/zero/releases/latest';

  let appConfig = $state<AppConfig | null>(null);
  let kernelInfo = $state<CoreKernelInfo | null>(null);
  let executablePathDraft = $state('');
  let downloadUrlDraft = $state('');
  let loading = $state(false);
  let saving = $state(false);
  let installOpen = $state(false);
  let installBusy = $state(false);
  let message = $state<string | null>(null);

  const kernelName = $derived(kernelInfo?.kernel ?? appConfig?.core.kernel ?? 'zero');
  const hasExecutable = $derived(Boolean(kernelInfo?.executableExists));
  const recommendedInstallDir = $derived(kernelInfo?.recommendedInstallDir ?? '');
  const installDownloadUrl = $derived(downloadUrlDraft.trim() || FALLBACK_DOWNLOAD_URL);
  const pathDirty = $derived(executablePathDraft.trim() !== (kernelInfo?.executablePath ?? ''));

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
      downloadUrlDraft = config.core.downloadUrl ?? info.downloadUrl ?? FALLBACK_DOWNLOAD_URL;
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

  function openInstallDialog() {
    installOpen = true;
    downloadUrlDraft = appConfig?.core.downloadUrl ?? kernelInfo?.downloadUrl ?? FALLBACK_DOWNLOAD_URL;
  }

  function closeInstallDialog() {
    if (installBusy) return;
    installOpen = false;
  }

  async function openDownloadLink() {
    installBusy = true;
    message = null;
    try {
      const currentUrl = installDownloadUrl;
      if (appConfig && currentUrl !== (appConfig.core.downloadUrl ?? '')) {
        appConfig = await updateAppConfig({
          core: { downloadUrl: currentUrl },
        });
      }
      await openLink(currentUrl);
      installOpen = false;
      message = recommendedInstallDir
        ? `已打开下载链接，建议安装到 ${recommendedInstallDir}`
        : '已打开下载链接';
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    } finally {
      installBusy = false;
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

  $effect(() => {
    refresh();
  });
</script>

<div class="panel">
  <div class="header">
    <div class="heading">
      <div class="title-row">
        <div class="title">内核配置</div>
        <Badge variant={hasExecutable ? 'secondary' : 'outline'}>
          {hasExecutable ? '已安装' : '未安装'}
        </Badge>
      </div>
      <div class="desc">
        仅支持自研内核。这里不暴露配置文件、工作目录或控制 socket。
      </div>
    </div>

    <div class="actions">
      <Button variant={hasExecutable ? 'outline' : 'default'} size="sm" onclick={openInstallDialog} disabled={loading || saving}>
        <Download class="h-3.5 w-3.5" />
        <span>安装</span>
      </Button>
      <Button variant="outline" size="sm" onclick={chooseExecutablePath} disabled={loading || saving}>
        <FolderOpen class="h-3.5 w-3.5" />
        <span>选择文件所在位置</span>
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
        先安装到应用目录，或者直接选择已有的可执行文件。
      </div>
      <div class="empty-actions">
        <Button onclick={openInstallDialog} disabled={loading || saving}>
          <Download class="h-3.5 w-3.5" />
          <span>安装</span>
        </Button>
        <Button variant="outline" onclick={chooseExecutablePath} disabled={loading || saving}>
          <FolderOpen class="h-3.5 w-3.5" />
          <span>选择文件所在位置</span>
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
          <div class="field-label">安装状态</div>
          <div class="value">
            <Badge variant="secondary">已安装</Badge>
          </div>
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
          {#each kernelInfo.warnings as warning}
            <div class="warning-line">{warning}</div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if message}
    <div class="message">{message}</div>
  {/if}
</div>

{#if installOpen}
  <div class="modal-layer">
    <button type="button" class="modal-backdrop" aria-label="关闭安装对话框" onclick={closeInstallDialog}></button>
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="install-core-title">
      <div class="modal-header">
        <div class="modal-title" id="install-core-title">安装内核</div>
        <Button variant="ghost" size="icon-sm" onclick={closeInstallDialog} disabled={installBusy}>
          <X class="h-3.5 w-3.5" />
          <span class="sr-only">关闭</span>
        </Button>
      </div>

      <div class="modal-body">
        <div class="field">
          <div class="field-label">下载地址</div>
          <Input bind:value={downloadUrlDraft} class="mono" placeholder={FALLBACK_DOWNLOAD_URL} />
          <div class="field-hint">默认安装目录：{recommendedInstallDir || '当前工作目录'}</div>
        </div>
      </div>

      <div class="modal-actions">
        <Button variant="outline" onclick={closeInstallDialog} disabled={installBusy}>取消</Button>
        <Button onclick={openDownloadLink} disabled={installBusy}>
          <Download class="h-3.5 w-3.5" />
          <span>{installBusy ? '处理中...' : '打开下载链接'}</span>
        </Button>
      </div>
    </div>
  </div>
{/if}

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

  .field-hint {
    font-size: 11px;
    color: var(--muted-foreground);
    line-height: 1.4;
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

  .message {
    padding: 10px 12px;
    font-size: 12px;
    color: var(--foreground);
  }

  .modal-layer {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }

  .modal-backdrop {
    position: absolute;
    inset: 0;
    border: 0;
    background: rgba(15, 23, 42, 0.42);
    padding: 0;
  }

  .modal {
    position: relative;
    z-index: 1;
    width: min(520px, 100%);
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--card);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.22);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .modal-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--foreground);
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
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

    .modal-actions {
      flex-direction: column-reverse;
    }

    .modal-actions :global(button) {
      width: 100%;
    }
  }
</style>
