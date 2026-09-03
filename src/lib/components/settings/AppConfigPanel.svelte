<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Textarea } from '$lib/components/ui/textarea';
  import { appendLog, getAppErrorMessage, getAppConfig, updateAppConfig, guiLogPaths, type GuiLogPaths } from '$lib/services/core';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import { store } from '$lib/services/store.svelte';
  import { setTheme, type ThemeMode } from '$lib/services/theme.svelte';
  import { trafficBallPreference } from '$lib/services/traffic-ball-preference.svelte';
  import type { AppConfig } from '$lib/types/app-config';
  import { Switch } from '$lib/components/ui/switch';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { NAV_TABS, TAB_LABELS } from '$lib/constants/navigation';
  import { onDestroy } from 'svelte';
  import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener';
  import { warning } from '$lib/services/toast.svelte';

  type SettingsScope = 'general' | 'network' | 'logs';
  let { scope = 'general' }: { scope?: SettingsScope } = $props();

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
  let proxyBypassDraft = $state('');
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  let configRequestGeneration = 0;

  const DEFAULT_PROXY_BYPASS = [
    '<local>', 'localhost', '127.*', '[::1]', '10.*', '192.168.*',
    ...Array.from({ length: 16 }, (_, index) => `172.${index + 16}.*`),
  ];

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
      proxyBypassDraft = (next.localProxy.bypass ?? DEFAULT_PROXY_BYPASS).join('\n');
    } catch (error) {
      if (generation === configRequestGeneration) {
        configError = getAppErrorMessage(error, '加载应用配置失败');
      }
    } finally {
      if (generation === configRequestGeneration) configLoading = false;
    }
  }

  async function toggleCoreSetting(key: 'autoStart' | 'autoConnect' | 'cleanupProxyOnExit') {
    if (!config) return;
    loading = true;
    updateError = null;
    try {
      const current = config.core[key];
      const updated = await updateAppConfig({ core: { [key]: !current } });
      config = updated;
    } catch (error) {
      updateError = getAppErrorMessage(error, '更新应用设置失败');
    } finally {
      loading = false;
    }
  }

  async function toggleTrafficBall(enabled: boolean) {
    updateError = null;
    try {
      await trafficBallPreference.setEnabled(enabled);
    } catch (error) {
      updateError = getAppErrorMessage(error, '更新流量悬浮球设置失败');
    }
  }

  function parseProxyBypass(value: string): string[] {
    return value.split(/\r?\n/).map((item) => item.trim()).filter(Boolean);
  }

  async function saveProxyBypass() {
    if (!config) return;
    loading = true;
    updateError = null;
    try {
      const updated = await updateAppConfig({
        localProxy: { bypass: parseProxyBypass(proxyBypassDraft) },
      });
      config = updated;
      proxyBypassDraft = updated.localProxy.bypass.join('\n');
    } catch (error) {
      updateError = getAppErrorMessage(error, '更新本地地址绕过配置失败');
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
    if (scope === 'logs') {
      void loadLogPaths();
    } else {
      void refreshConfig();
    }
  });

  onDestroy(() => {
    if (copyTimer) clearTimeout(copyTimer);
  });
</script>

{#if scope === 'general'}
  <div class="config-section">
    <div class="config-section-title">界面与窗口</div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">主题</span>
        <span class="label-desc">选择界面配色方案。</span>
      </div>
      <SegmentedControl.Root
        value={store.selectedTheme}
        onValueChange={(value) => handleThemeChange(value as ThemeMode)}
        class="config-segment"
        aria-label="主题"
      >
        {#each THEMES as theme}
          <SegmentedControl.Item value={theme.value}>
            {theme.label}
          </SegmentedControl.Item>
        {/each}
      </SegmentedControl.Root>
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">界面模式</span>
        <span class="label-desc">简约模式保留常用入口，专业模式展示完整控制面。</span>
      </div>
      <SegmentedControl.Root
        value={store.uiMode}
        onValueChange={(value) => {
          if (value === 'lite' || value === 'pro') void store.switchUIMode(value);
        }}
        disabled={store.isSwitchingUiMode}
        class="config-segment"
        aria-label="界面模式"
      >
        <SegmentedControl.Item value="lite">简约</SegmentedControl.Item>
        <SegmentedControl.Item value="pro">专业</SegmentedControl.Item>
      </SegmentedControl.Root>
    </div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">流量悬浮球</span>
        <span class="label-desc">最小化或关闭主窗口时，以悬浮球继续展示实时上下行速率。</span>
      </div>
      <Switch
        checked={trafficBallPreference.enabled}
        onCheckedChange={(checked) => void toggleTrafficBall(checked)}
        disabled={trafficBallPreference.loading || trafficBallPreference.saving}
        aria-label="流量悬浮球"
      />
    </div>
  </div>

  {#if configError || updateError || trafficBallPreference.error}
    <div class="settings-error" role="alert">
      <span>{configError ?? updateError ?? trafficBallPreference.error}</span>
      {#if configError}
        <Button variant="outline" size="sm"  onclick={refreshConfig} disabled={configLoading}>重试</Button>
      {/if}
    </div>
  {/if}

  {#if store.uiMode === 'pro'}
    <div class="config-separator"></div>

    <div class="config-section">
      <div class="config-section-title">界面导航</div>

      <div class="menu-panel">
        <div class="menu-panel-head">
          <div class="label-text">专业模式菜单</div>
          <div class="label-desc">选择需要显示在主导航中的功能入口；设置入口始终保留。</div>
        </div>

        {#if configLoading}
          <div class="config-loading">加载配置中...</div>
        {:else if !config}
          <div class="config-loading error-copy">应用配置不可用，请先重试加载。</div>
        {:else}
          <div class="menu-button-row">
            {#each menuTabs as tab}
              <Button variant="outline" size="sm"
                type="button"

                onclick={() => toggleMenuVisibility(tab.id)}
                disabled={updatingMenuKey !== null || loading}
                aria-pressed={isMenuVisible(tab.id)}
              >
                <span>{TAB_LABELS[tab.id] ?? tab.label}{tab.comingSoon ? '（敬请期待）' : ''}</span>
              </Button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <div class="config-separator"></div>

  <div class="config-section">
    <div class="config-section-title">启动与连接</div>

    {#if configLoading}
      <div class="config-loading">加载配置中...</div>
    {:else if !config}
      <div class="config-loading error-copy">应用配置不可用，请先重试加载。</div>
    {:else}
      <div class="config-row">
        <div class="config-row-label">
          <span class="label-text">启动应用时启动内核</span>
          <span class="label-desc">打开 ZNet Sink 后自动启动当前配置的内核。</span>
        </div>
        <Switch
          checked={config.core.autoStart}
          onCheckedChange={() => toggleCoreSetting('autoStart')}
          disabled={loading}
          aria-label="启动应用时启动内核"
        />
      </div>

      <div class="config-row">
        <div class="config-row-label">
          <span class="label-text">内核就绪后自动连接</span>
          <span class="label-desc">内核可用后，自动启用由 ZNet Sink 管理的系统代理。</span>
        </div>
        <Switch
          checked={config.core.autoConnect}
          onCheckedChange={() => toggleCoreSetting('autoConnect')}
          disabled={loading}
          aria-label="内核就绪后自动连接"
        />
      </div>
    {/if}
  </div>

  <div class="config-separator"></div>

  <div class="config-section">
    <div class="config-section-title">引导</div>

    <div class="config-row">
      <div class="config-row-label">
        <span class="label-text">重新显示新手引导</span>
        <span class="label-desc">下次进入应用时重新显示当前版本的引导，不会重置其他设置。</span>
      </div>
      <Button variant="outline" size="sm"  onclick={() => store.resetOnboarding()}>
        重新显示
      </Button>
    </div>
  </div>
{:else if scope === 'network'}
  {#if configError || updateError}
    <div class="settings-error" role="alert">
      <span>{configError ?? updateError}</span>
      {#if configError}
        <Button variant="outline" size="sm"  onclick={refreshConfig} disabled={configLoading}>重试</Button>
      {/if}
    </div>
  {/if}

  <div class="config-section">
    <div class="config-section-title">系统代理</div>

    {#if configLoading}
      <div class="config-loading">加载配置中...</div>
    {:else if !config}
      <div class="config-loading error-copy">应用配置不可用，请先重试加载。</div>
    {:else}
      <div class="config-row">
        <div class="config-row-label">
          <span class="label-text">退出时恢复系统代理</span>
          <span class="label-desc">退出 ZNet Sink 时恢复应用接管前的系统代理设置。</span>
        </div>
        <Switch
          checked={config.core.cleanupProxyOnExit}
          onCheckedChange={() => toggleCoreSetting('cleanupProxyOnExit')}
          disabled={loading}
          aria-label="退出时恢复系统代理"
        />
      </div>

      <div class="proxy-bypass-editor">
        <div class="config-row-label">
          <span class="label-text">系统代理绕过</span>
          <span class="label-desc">每行一项，仅作用于系统代理，不等于 TUN 排除。支持 localhost、域名和 16.* 这类主机模式；整字节 IPv4 CIDR（如 16.0.0.0/8）保存时会自动转换。</span>
        </div>
        <Textarea
          class="font-mono"
          bind:value={proxyBypassDraft}
          disabled={loading}
          rows={6}
          spellcheck="false"
          aria-label="本地地址绕过列表"
        ></Textarea>
        <div class="bypass-actions">
          <Button variant="outline" size="sm"

            onclick={() => (proxyBypassDraft = DEFAULT_PROXY_BYPASS.join('\n'))}
            disabled={loading}
          >恢复默认</Button>
          <Button variant="default" size="sm"  onclick={saveProxyBypass} disabled={loading}>保存</Button>
        </div>
      </div>
    {/if}
  </div>
{:else}
  <div class="config-section">
    <div class="config-section-title">日志与数据</div>

    {#if logPathsError}
      <div class="config-row">
        <div class="config-row-label">
          <span class="label-text" style="color: var(--destructive);">{logPathsError}</span>
        </div>
        <Button variant="outline" size="sm"  onclick={loadLogPaths}>重试</Button>
      </div>
    {:else if logPaths}
      <div class="config-row">
        <div class="config-row-label">
          <span class="label-text">应用日志</span>
          <span class="label-desc log-path">{logPaths.logFile}</span>
        </div>
        <div class="log-actions">
          <Button variant="outline" size="sm"  onclick={() => copyToClipboard(logPaths!.logFile, 'logFile')}>
            {copiedField === 'logFile' ? '已复制' : '复制'}
          </Button>
          <Button variant="default" size="sm"  onclick={() => revealLogFile(logPaths!.logFile)}>打开目录</Button>
        </div>
      </div>

      <div class="config-row">
        <div class="config-row-label">
          <span class="label-text">内核日志</span>
          <span class="label-desc log-path">{logPaths.coreLogFile}</span>
        </div>
        <div class="log-actions">
          <Button variant="outline" size="sm"  onclick={() => copyToClipboard(logPaths!.coreLogFile, 'coreLogFile')}>
            {copiedField === 'coreLogFile' ? '已复制' : '复制'}
          </Button>
          <Button variant="default" size="sm"  onclick={() => revealLogFile(logPaths!.coreLogFile)}>打开目录</Button>
        </div>
      </div>

      <div class="config-row">
        <div class="config-row-label">
          <span class="label-text">日志目录</span>
          <span class="label-desc log-path">{logPaths.logsDir}</span>
        </div>
        <div class="log-actions">
          <Button variant="outline" size="sm"  onclick={() => copyToClipboard(logPaths!.logsDir, 'logsDir')}>
            {copiedField === 'logsDir' ? '已复制' : '复制'}
          </Button>
          <Button variant="default" size="sm"  onclick={() => openDirectory(logPaths!.logsDir)}>打开</Button>
        </div>
      </div>

      <div class="config-row">
        <div class="config-row-label">
          <span class="label-text">数据目录</span>
          <span class="label-desc log-path">{logPaths.dataDir}</span>
        </div>
        <div class="log-actions">
          <Button variant="outline" size="sm"  onclick={() => copyToClipboard(logPaths!.dataDir, 'dataDir')}>
            {copiedField === 'dataDir' ? '已复制' : '复制'}
          </Button>
          <Button variant="default" size="sm"  onclick={() => openDirectory(logPaths!.dataDir)}>打开</Button>
        </div>
      </div>
    {:else}
      <div class="config-loading">加载路径中...</div>
    {/if}

    {#if pathActionError}
      <div class="settings-error" role="alert">{pathActionError}</div>
    {/if}
  </div>
{/if}

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
    line-height: 1.5;
  }

  :global(.config-segment) {
    flex-shrink: 0;
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

  .config-loading {
    font-size: 12px;
    color: var(--muted-foreground);
    padding: 14px 0;
    text-align: center;
    opacity: 0.6;
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

  .proxy-bypass-editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 0;
  }

  .bypass-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  @media (max-width: 760px) {
    .config-row {
      flex-direction: column;
      align-items: stretch;
    }

    :global(.config-segment) {
      width: fit-content;
    }
  }
</style>
