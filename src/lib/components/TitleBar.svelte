<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { getName, getVersion } from '@tauri-apps/api/app';
  import { store } from '$lib/services/store.svelte';
  import AppLogo from '$lib/components/AppLogo.svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import KernelStatusPill from '$lib/components/core/KernelStatusPill.svelte';
  import { updater } from '$lib/services/updater.svelte';
  import { showTrafficBall } from '$lib/services/traffic-ball';
  import { trafficBallPreference } from '$lib/services/traffic-ball-preference.svelte';

  let appWindow: ReturnType<typeof getCurrentWindow> | null = null;
  let appName = $state('ZNet Sink');
  let appVersion = $state('');

  $effect(() => {
    let mounted = true;
    async function init() {
      try {
        appWindow = getCurrentWindow();
        if (mounted) {
          appName = await getName();
          appVersion = await getVersion();
        }
      } catch (e) {
        // 非 Tauri 环境下忽略
      }
    }
    init();
    return () => { mounted = false; };
  });

  onMount(() => {
    let mounted = true;
    let unlistenClose: (() => void) | null = null;
    try {
      const current = getCurrentWindow();
      void current.onCloseRequested((event) => {
        if (!trafficBallPreference.enabled) return;
        event.preventDefault();
        void showTrafficBall(current);
      }).then((unlisten) => {
        if (mounted) unlistenClose = unlisten;
        else unlisten();
      }).catch(() => {});
    } catch {
      // Browser preview has no native close event.
    }

    return () => {
      mounted = false;
      unlistenClose?.();
    };
  });

  const handleMinimize = () => {
    if (!appWindow) return;
    if (trafficBallPreference.enabled) {
      void showTrafficBall(appWindow);
    } else {
      void appWindow.minimize().catch(() => {});
    }
  };

  const handleMaximize = () => appWindow?.toggleMaximize().catch(() => {});

  const handleClose = () => {
    if (!appWindow) return;
    if (trafficBallPreference.enabled) {
      void showTrafficBall(appWindow);
    } else {
      void appWindow.close().catch(() => {});
    }
  };
</script>

<!--
  TitleBar: h-11 (44px), compact desktop toolbar
  Layout: [Logo + Name + Version + ModeSwitch .............. WindowControls]
-->
<div
  data-tauri-drag-region
  class="h-11 w-full flex-shrink-0 flex items-center justify-between select-none"
  style="
    background: var(--titlebar);
    border-bottom: 1px solid var(--titlebar-border);
    backdrop-filter: blur(12px) saturate(1.5);
    -webkit-backdrop-filter: blur(12px) saturate(1.5);
  "
>
  <!-- Left: App identity + mode switch inline -->
  <div class="flex items-center gap-2 pl-3.5 min-w-0 overflow-hidden">
    <!-- Logo: theme-aware icon -->
    <div class="titlebar-logo" style="--logo-radius: 3px; opacity: 0.9; transition: opacity 0.15s ease">
      <AppLogo size={18} />
    </div>
    <!-- App name -->
    <span
      class="font-semibold text-foreground/90 tracking-tight overflow-hidden text-ellipsis whitespace-nowrap"
      style="font-size: 13px; letter-spacing: -0.01em; max-width: 120px;"
    >
      {appName}
    </span>
    <!-- Version — opens About; yellow pulse dot when an update is available -->
    {#if appVersion}
      <button
        onclick={() => store.openSettings('about')}
        class="titlebar-version"
        title={updater.prominentUpdateAvailable
          ? `新版本 v${updater.latestVersion} 可用 — 点击查看`
          : '关于'}
        aria-label="关于"
      >
        {#if updater.prominentUpdateAvailable}
          <span class="titlebar-update-dot" aria-hidden="true"></span>
        {/if}
        <span class="text-muted-foreground" style="font-size: 11px; line-height: 1;">
          v{appVersion}
        </span>
      </button>
    {/if}

    <!-- Divider -->
    <span class="titlebar-divider flex-shrink-0" aria-hidden="true"></span>

    <!-- Mode segmented control — inline after identity -->
    <SegmentedControl.Root
      value={store.uiMode}
      onValueChange={(value) => {
        if (value === 'lite' || value === 'pro') void store.switchUIMode(value);
      }}
      disabled={store.isSwitchingUiMode}
      class="flex-shrink-0"
      aria-label="界面模式"
    >
      <SegmentedControl.Item
        value="lite"
        style="min-width: 48px;"
        aria-label="简约模式"
        title="简约模式"
      >
        简约
      </SegmentedControl.Item>
      <SegmentedControl.Item
        value="pro"
        style="min-width: 48px;"
        aria-label="专业模式"
        title="专业模式"
      >
        专业
      </SegmentedControl.Item>
    </SegmentedControl.Root>
  </div>

  <!-- Right: Kernel status + Window controls -->
  <div class="flex items-center gap-1 pr-2 flex-shrink-0">
    <KernelStatusPill />

    <span class="titlebar-divider flex-shrink-0" aria-hidden="true"></span>

    <button
      onclick={handleMinimize}
      class="titlebar-btn"
      aria-label={trafficBallPreference.enabled ? '最小化为流量悬浮球' : '最小化'}
      title={trafficBallPreference.enabled ? '最小化为流量悬浮球' : '最小化'}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
        <rect x="0" y="5" width="10" height="1" rx="0.5"/>
      </svg>
    </button>
    <button
      onclick={handleMaximize}
      class="titlebar-btn"
      aria-label="最大化"
      title="最大化"
    >
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
        <rect x="0.5" y="0.5" width="9" height="9" rx="1"/>
      </svg>
    </button>
    <button
      onclick={handleClose}
      class="titlebar-btn titlebar-btn-close"
      aria-label={trafficBallPreference.enabled ? '关闭为流量悬浮球' : '关闭'}
      title={trafficBallPreference.enabled ? '关闭为流量悬浮球' : '关闭'}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
        <line x1="2" y1="2" x2="8" y2="8"/>
        <line x1="8" y1="2" x2="2" y2="8"/>
      </svg>
    </button>
  </div>
</div>

<style>
  /* Logo container in titlebar */
  .titlebar-logo {
    --logo-radius: 3px;
    opacity: 0.9;
    transition: opacity 0.15s ease;
  }

  /* 分隔线 */
  .titlebar-divider {
    display: block;
    width: 1px;
    height: 16px;
    background: var(--titlebar-border);
    border-radius: 1px;
    margin: 0 2px;
  }

  .titlebar-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 5px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }

  .titlebar-btn:hover {
    background: var(--muted);
    color: var(--foreground);
  }

  .titlebar-btn:active {
    opacity: 0.7;
  }

  .titlebar-btn-close:hover {
    background: rgba(239, 68, 68, 0.12);
    color: #EF4444;
  }

  :global(.dark) .titlebar-btn-close:hover {
    background: rgba(248, 113, 113, 0.14);
    color: #F87171;
  }

  /* Version button — opens About; shows a pulsing dot when an update is pending */
  .titlebar-version {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: none;
    background: transparent;
    padding: 3px 6px;
    border-radius: 5px;
    cursor: pointer;
    transition: background 0.12s ease;
    flex-shrink: 0;
  }

  .titlebar-version:hover {
    background: var(--muted);
  }

  .titlebar-update-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #F59E0B;
    flex-shrink: 0;
    animation: titlebar-update-pulse 1.4s ease-in-out infinite;
  }

  @keyframes titlebar-update-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.35; }
  }
</style>
