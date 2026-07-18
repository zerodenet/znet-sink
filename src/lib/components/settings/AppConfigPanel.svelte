<script lang="ts">
  import { appendLog, getAppErrorMessage, getAppConfig, updateAppConfig, guiLogPaths, type GuiLogPaths } from '$lib/services/core';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import { store } from '$lib/services/store.svelte';
  import { setTheme, type ThemeMode } from '$lib/services/theme.svelte';
  import type { AppConfig } from '$lib/types/app-config';
  import { Switch } from '$lib/components/ui/switch';
  import { NAV_TABS, TAB_LABELS } from '$lib/constants/navigation';
  import { onDestroy } from 'svelte';
  import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener';
  import { warning } from '$lib/services/toast.svelte';

  let config = $state<AppConfig | null>(null);
  let configLoading = $state(true);
  let configError = $state<string | null>(null);
  let updateError = $state<string | null>(null);
  let loading = $state(false);
  let updatingMenuKey = $state<string | null>(null);
  let logPaths = $state<GuiLogPaths | null>(null);
  let logPathsError = $state<string | null>(null);
  let copiedField = $state<string | null>(null);
  let pathActionError = $state<string | null>(null);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  let configRequestGeneration = 0;

  const menuTabs = NAV_TABS.filter((tab) => tab.id !== 'settings');

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function logPathAction(level: 'info' | 'warn' | 'error', message: string, fields: Record<string, unknown>) {
    void appendLog({ source: 'app', level, message, fields }).catch((error) => {
      console.error('Failed to append path action log:', error);
    });
  }

  async function refreshConfig() {
    const generation = ++configRequestGeneration;
    configLoading = true;
    configError = null;
    updateError = null;
    try {
      const next = await getAppConfig();
      if (generation !== configRequestGeneration) return;
      config = next;
    } catch (error) {
      if (generation === configRequestGeneration) {
        configError = getAppErrorMessage(error, '加载应用配置失败');
      }
    } finally {
      if (generation === configRequestGeneration) configLoading = false;
    }
  }

  async function toggleCoreSetting(key: 'autoStart' | 'autoConnect') {
    if (!config) return;
    loading = true;
    updateError = null;
    try {
      const current = config.core[key];
      const updated = await updateAppConfig({ core: { [key]: !current } });
      config = updated;
    } catch (error) {
      updateError = getAppErrorMessage(error, '更新内核行为失败');
    } finally {
      loading = false;
    }
  }

  function handleThemeChange(theme: ThemeMode) {
    setTheme(theme);
  }

  async function toggleMenuVisibility(key: string) {
    if (!config || key === 'settings' || store.uiMode !== 'pro') return;

    updatingMenuKey = key;
    updateError = null;
    try {
      const hidden = new Set((config.ui.hiddenMenuKeys ?? []).map((item) => item.toLowerCase()));
      if (hidden.has(key)) {
        hidden.delete(key);
      } else {
        hidden.add(key);
      }

      const nextHiddenKeys = Array.from(hidden);
      const updated = await updateAppConfig({
        ui: { hiddenMenuKeys: nextHiddenKeys },
      });

      config = {
        ...updated,
        ui: {
          ...updated.ui,
          hiddenMenuKeys: nextHiddenKeys,
        },
      };

      await store.refreshInteractionSurface();

      if (!store.isNavVisible(store.activeTab)) {
        store.activeTab = 'overview';
      }
    } catch (error) {
      updateError = getAppErrorMessage(error, '更新菜单设置失败');
    } finally {
      updatingMenuKey = null;
    }
  }

  function isMenuVisible(key: string): boolean {
    if (!config) return true;
    return !(config.ui.hiddenMenuKeys ?? []).some((item) => item.toLowerCase() === key);
  }

  async function loadLogPaths() {
    try {
      logPaths = await guiLogPaths();
      logPathsError = null;
    } catch (e) {
      logPathsError = getAppErrorMessage(e, '获取日志路径失败');
    }
  }

  async function copyToClipboard(text: string, field: string) {
    pathActionError = null;
    try {
      await copyTextToClipboard(text);
      logPathAction('info', '已复制应用路径', { action: 'copy_path', field, path: text });
      copiedField = field;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copiedField = null;
        copyTimer = null;
      }, 2000);
    } catch (clipboardError) {
      const message = getAppErrorMessage(clipboardError, '浏览器未能复制路径');
      pathActionError = message;
      logPathAction('error', message, {
        action: 'copy_path_failed', field, path: text, clipboardError: errorMessage(clipboardError),
      });
    }
  }

  async function openDirectory(path: string) {
    pathActionError = null;
    try {
      await openPath(path);
      logPathAction('info', '已打开应用目录', { action: 'open_directory', path });
    } catch (openError) {
      logPathAction('warn', '直接打开目录失败，尝试在资源管理器中定位', {
        action: 'open_directory_fallback', path, error: errorMessage(openError),
      });
      try {
        await revealItemInDir(path);
        logPathAction('info', '已在资源管理器中定位应用目录', {
          action: 'reveal_directory', path,
        });
      } catch (revealError) {
        const message = `无法打开目录: ${errorMessage(revealError)}`;
        pathActionError = message;
        logPathAction('error', message, {
          action: 'open_directory_failed', path,
          openError: errorMessage(openError),
          revealError: errorMessage(revealError),
        });
        warning(message);
      }
    }
  }

  async function revealLogFile(path: string) {
    pathActionError = null;
    try {
      await revealItemInDir(path);
      logPathAction('info', '已在资源管理器中定位日志文件', {
        action: 'reveal_log_file', path,
      });
    } catch (error) {
      const message = `无法定位日志文件: ${errorMessage(error)}`;
      pathActionError = message;
      logPathAction('error', message, {
        action: 'reveal_log_file_failed', path, error: errorMessage(error),
      });
      warning(message);
    }
  }

  const THEMES: Array<{ value: ThemeMode; label: string }> = [
    { value: 'light', label: '明亮' },
    { value: 'dark', label: '暗色' },
    { value: 'system', label: '跟随系统' },
  ];

  $effect(() => {
    refreshConfig();
    loadLogPaths();
  });

  onDestroy(() => {
    if (copyTimer) clearTimeout(copyTimer);
  });
</script>

<div class="config-section">
  <div class="config-section-title">外观</div>

  <div class="config-row">
    <div class="config-row-label">
      <span class="label-text">主题</span>
      <span class="label-desc">选择界面配色方案</span>
    </div>
    <div class="theme-segment">
      {#each THEMES as theme}
        <button
          onclick={() => handleThemeChange(theme.value)}
          class="theme-seg-btn {store.selectedTheme === theme.value ? 'active' : ''}"
          aria-pressed={store.selectedTheme === theme.value}
        >
          {theme.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="config-row">
    <div class="config-row-label">
      <span class="label-text">界面模式</span>
      <span class="label-desc">简约模式会收起高阶入口，专业模式展示完整控制面。</span>
    </div>
    <div class="theme-segment">
      <button
        onclick={async () => await store.switchUIMode('lite')}
        class="theme-seg-btn {store.uiMode === 'lite' ? 'active' : ''}"
      >
        简约
      </button>
      <button
        onclick={async () => await store.switchUIMode('pro')}
        class="theme-seg-btn {store.uiMode === 'pro' ? 'active' : ''}"
      >
        专业
      </button>
    </div>
  </div>
</div>

{#if configError || updateError}
  <div class="settings-error" role="alert">
    <span>{configError ?? updateError}</span>
    {#if configError}
      <button class="log-action-btn" onclick={refreshConfig} disabled={configLoading}>重试</button>
    {/if}
  </div>
{/if}

{#if store.uiMode === 'pro'}
  <div class="config-separator"></div>

  <div class="config-section">
    <div class="config-section-title">菜单</div>

    <div class="menu-panel">
      <div class="menu-panel-head">
        <div class="label-text">专业模式菜单显隐</div>
        <div class="label-desc">设置固定展示，其他菜单点击选中表示显示，再点一次取消显示。</div>
      </div>

      {#if configLoading}
        <div class="config-loading">加载配置中...</div>
      {:else if !config}
        <div class="config-loading error-copy">应用配置不可用，请先重试加载。</div>
      {:else}
        <div class="menu-button-row">
          {#each menuTabs as tab}
            <button
              type="button"
              class="menu-chip {isMenuVisible(tab.id) ? 'active' : ''}"
              onclick={() => toggleMenuVisibility(tab.id)}
              disabled={updatingMenuKey !== null || loading}
              aria-pressed={isMenuVisible(tab.id)}
            >
              <span>{TAB_LABELS[tab.id] ?? tab.label}{tab.comingSoon ? '（敬请期待）' : ''}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<div class="config-separator"></div>

<div class="config-section">
  <div class="config-section-title">内核行为</div>

  {#if configLoading}
    <div class="config-loading">加载配置中...</div>
  {:else if !config}
    <div class="config-loading error-copy">应用配置不可用，请先重试加载。</div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">开机自动启动内核</span>
        <span class="label-desc">系统启动时自动运行内核进程。</span>
      </div>
      <Switch
        checked={config.core.autoStart}
        onCheckedChange={() => toggleCoreSetting('autoStart')}
        disabled={loading}
        aria-label="开机自动启动内核"
      />
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">启动后自动连接</span>
        <span class="label-desc">内核启动完成后自动建立连接。</span>
      </div>
      <Switch
        checked={config.core.autoConnect}
        onCheckedChange={() => toggleCoreSetting('autoConnect')}
        disabled={loading}
        aria-label="启动后自动连接"
      />
    </div>
  {/if}
</div>

<div class="config-separator"></div>

<div class="config-section">
  <div class="config-section-title">日志</div>

  {#if logPathsError}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text" style="color: var(--destructive);">{logPathsError}</span>
      </div>
      <button class="log-action-btn" onclick={loadLogPaths}>重试</button>
    </div>
  {:else if logPaths}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">运行日志文件</span>
        <span class="label-desc log-path">{logPaths.logFile}</span>
      </div>
      <div class="log-actions">
        <button
          class="log-action-btn"
          onclick={() => copyToClipboard(logPaths!.logFile, 'logFile')}
          title="复制路径"
        >
          {copiedField === 'logFile' ? '已复制' : '复制'}
        </button>
        <button
          class="log-action-btn primary"
          onclick={() => revealLogFile(logPaths!.logFile)}
          title="在文件夹中显示"
        >
          打开目录
        </button>
      </div>
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">日志目录</span>
        <span class="label-text">内核日志文件</span>
        <span class="label-desc log-path">{logPaths.coreLogFile}</span>
      </div>
      <div class="log-actions">
        <button
          class="log-action-btn core-log-btn"
          onclick={() => copyToClipboard(logPaths!.coreLogFile, 'coreLogFile')}
          title="复制路径"
        >
          {copiedField === 'coreLogFile' ? '已复制' : '复制'}
        </button>
        <button
          class="log-action-btn primary"
          onclick={() => revealLogFile(logPaths!.coreLogFile)}
          title="在文件夹中显示"
        >
          打开目录
        </button>
      </div>
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">日志目录</span>
        <span class="label-desc log-path">{logPaths.logsDir}</span>
      </div>
      <div class="log-actions">
        <button
          class="log-action-btn"
          onclick={() => copyToClipboard(logPaths!.logsDir, 'logsDir')}
          title="复制路径"
        >
          {copiedField === 'logsDir' ? '已复制' : '复制'}
        </button>
        <button
          class="log-action-btn primary"
          onclick={() => openDirectory(logPaths!.logsDir)}
          title="打开文件夹"
        >
          打开
        </button>
      </div>
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">数据目录</span>
        <span class="label-desc log-path">{logPaths.dataDir}</span>
      </div>
      <div class="log-actions">
        <button
          class="log-action-btn"
          onclick={() => copyToClipboard(logPaths!.dataDir, 'dataDir')}
          title="复制路径"
        >
          {copiedField === 'dataDir' ? '已复制' : '复制'}
        </button>
        <button
          class="log-action-btn primary"
          onclick={() => openDirectory(logPaths!.dataDir)}
          title="打开文件夹"
        >
          打开
        </button>
      </div>
    </div>
  {:else}
    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">加载中...</span>
      </div>
    </div>
  {/if}

  {#if pathActionError}
    <div class="settings-error" role="alert">{pathActionError}</div>
  {/if}
</div>

<div class="config-separator"></div>

<div class="config-section">
  <div class="config-section-title">其他</div>

  <div class="config-row">
    <div class="config-row-label">
      <span class="label-text">重置引导</span>
      <span class="label-desc">清除本地状态，重启后重新显示欢迎引导。</span>
    </div>
    <button class="reset-btn" onclick={() => { store.resetApp(); location.reload(); }}>
      重置
    </button>
  </div>
</div>

<style>
  .config-section {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .config-section-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    padding: 0 0 8px;
    opacity: 0.7;
  }

  .config-separator {
    height: 1px;
    background: var(--border);
    margin: 16px 0;
  }

  .settings-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin: 10px 0;
    padding: 8px 10px;
    border: 1px solid rgba(239, 68, 68, 0.22);
    border-radius: 8px;
    background: rgba(239, 68, 68, 0.07);
    color: var(--destructive);
    font-size: 11.5px;
  }

  .error-copy {
    color: var(--destructive);
  }

  .config-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }

  .config-row:last-child {
    border-bottom: none;
  }

  .config-row-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .label-text {
    font-size: 13px;
    font-weight: 500;
    color: var(--foreground);
  }

  .label-desc {
    font-size: 11.5px;
    color: var(--muted-foreground);
    opacity: 0.8;
  }

  .theme-segment {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    background: var(--segment-bg, rgba(0, 0, 0, 0.055));
    padding: 2px;
    border-radius: 7px;
    flex-shrink: 0;
  }

  .theme-seg-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 26px;
    padding: 0 11px;
    border-radius: 5px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.13s ease;
    white-space: nowrap;
  }

  .theme-seg-btn:hover {
    color: var(--foreground);
  }

  .theme-seg-btn.active {
    background: var(--segment-active-bg, #ffffff);
    box-shadow: var(--segment-active-shadow, 0 1px 3px rgba(0, 0, 0, 0.12));
    color: var(--foreground);
    font-weight: 600;
  }

  .menu-panel {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--card);
    overflow: hidden;
  }

  .menu-panel-head {
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 3px;
    background: var(--muted);
  }

  .menu-button-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 14px;
  }

  .menu-chip {
    height: 30px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.13s ease, color 0.13s ease, border-color 0.13s ease;
  }

  .menu-chip:hover:not(:disabled) {
    color: var(--foreground);
    background: var(--muted);
  }

  .menu-chip.active {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--primary-foreground);
    font-weight: 600;
  }

  .menu-chip:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .config-loading {
    font-size: 12px;
    color: var(--muted-foreground);
    padding: 14px 0;
    text-align: center;
    opacity: 0.6;
  }

  .reset-btn {
    height: 28px;
    padding: 0 16px;
    border-radius: 7px;
    border: 1px solid rgba(239, 68, 68, 0.3);
    background: rgba(239, 68, 68, 0.06);
    color: var(--destructive, #EF4444);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.13s ease;
    white-space: nowrap;
  }

  .reset-btn:hover {
    background: rgba(239, 68, 68, 0.12);
  }

  .log-path {
    font-family: var(--font-mono);
    font-size: 11px;
    word-break: break-all;
    line-height: 1.4;
  }

  .log-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .log-action-btn {
    height: 26px;
    padding: 0 10px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted-foreground);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.13s ease;
    white-space: nowrap;
  }

  .log-action-btn:hover {
    background: var(--muted);
    color: var(--foreground);
  }

  .log-action-btn.primary {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--primary-foreground);
  }

  .log-action-btn.primary:hover {
    opacity: 0.9;
  }

  .config-row:has(.core-log-btn) .config-row-label > .label-text:first-child {
    display: none;
  }

  @media (max-width: 760px) {
    .config-row {
      flex-direction: column;
      align-items: stretch;
    }

    .theme-segment {
      width: fit-content;
    }
  }
</style>
