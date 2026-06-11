<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { getName, getVersion } from '@tauri-apps/api/app';
  import { store } from '$lib/services/store.svelte';
  import AppLogo from '$lib/components/AppLogo.svelte';

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

  const handleMinimize = () => appWindow?.minimize().catch(() => {});
  const handleMaximize = () => appWindow?.toggleMaximize().catch(() => {});
  const handleClose = () => appWindow?.close().catch(() => {});
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
    <!-- Version -->
    {#if appVersion}
      <span
        class="text-muted-foreground flex-shrink-0"
        style="font-size: 11px; line-height: 1;"
      >
        v{appVersion}
      </span>
    {/if}

    <!-- Divider -->
    <span class="titlebar-divider flex-shrink-0" aria-hidden="true"></span>

    <!-- Mode segmented control — inline after identity -->
    <div class="segment-root flex-shrink-0" style="height: 26px;">
      <button
        onclick={async () => await store.switchUIMode('lite')}
        class="segment-item {store.uiMode === 'lite' ? 'active' : ''}"
        style="min-width: 48px; padding: 4px 10px;"
        aria-label="简约模式"
        title="简约模式"
      >
        简约
      </button>
      <button
        onclick={async () => await store.switchUIMode('pro')}
        class="segment-item {store.uiMode === 'pro' ? 'active' : ''}"
        style="min-width: 48px; padding: 4px 10px;"
        aria-label="专业模式"
        title="专业模式"
      >
        专业
      </button>
    </div>
  </div>

  <!-- Right: Window controls -->
  <div class="flex items-center gap-0.5 pr-2 flex-shrink-0">
    <button
      onclick={handleMinimize}
      class="titlebar-btn"
      aria-label="最小化"
      title="最小化"
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
      aria-label="关闭"
      title="关闭"
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
</style>
