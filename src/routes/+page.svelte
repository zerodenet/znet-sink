<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { store } from '$lib/services/store.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { initTheme, applyTheme } from '$lib/services/theme.svelte';
  import { updater } from '$lib/services/updater.svelte';
  import { fade } from 'svelte/transition';
  import TitleBar from '$lib/components/TitleBar.svelte';
  import AppHeader from '$lib/components/AppHeader.svelte';
  import AppLogo from '$lib/components/AppLogo.svelte';
  import UpdateBanner from '$lib/components/UpdateBanner.svelte';
  import { Spinner } from '$lib/components/ui/Spinner';
  import TabContent from '$lib/components/TabContent.svelte';
  import { WelcomeGuide } from '$lib/components/WelcomeGuide';
  import { installGlobalErrorTelemetry, recordTelemetry } from '$lib/services/telemetry';

  onMount(() => {
    let unlistenNavigate: UnlistenFn | null = null;
    const uninstallGlobalErrorTelemetry = installGlobalErrorTelemetry();
    initTheme();
    requestAnimationFrame(() => {
      void getCurrentWindow().show().catch((error) => {
        console.error('Failed to show main window after first render:', error);
        void recordTelemetry({
          level: 'error',
          area: 'startup',
          operation: 'window.show',
          message: error instanceof Error ? error.message : String(error),
        });
      });
    });
    void store.loadFromBackend();
    void listen<{ tab?: string; section?: string }>('app:navigate', (event) => {
      const { tab, section } = event.payload;
      if (tab === 'settings') {
        store.openSettings(
          section === 'core' || section === 'config' || section === 'about' ? section : 'general',
        );
      } else if (tab) {
        store.isInitialized = true;
        store.activeTab = tab;
      }
    }).then((unlisten) => {
      unlistenNavigate = unlisten;
    });
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const onSystemThemeChange = () => {
      if (store.selectedTheme === 'system') applyTheme('system');
    };
    mediaQuery.addEventListener('change', onSystemThemeChange);
    return () => {
      mediaQuery.removeEventListener('change', onSystemThemeChange);
      unlistenNavigate?.();
      uninstallGlobalErrorTelemetry();
    };
  });

  $effect(() => {
    // This is the only reactive dependency of the app lifecycle. The methods
    // below synchronously read and write their own rune state before their
    // first await; without untrack those fields become accidental effect
    // dependencies and every loading/status update restarts the whole app.
    const shouldInitialize = store.isInitialized;
    untrack(() => {
      if (shouldInitialize) {
        void guiState.initialize();
        void coreEvents.start();
        updater.startPeriodicChecks();
      } else {
        guiState.destroy();
        void coreEvents.stop();
        updater.stopPeriodicChecks();
      }
    });
    return () => {
      untrack(() => {
        guiState.destroy();
        void coreEvents.stop();
        updater.stopPeriodicChecks();
      });
    };
  });

  // Refresh runtime state when the core event stream signals a status change.
  // guiState.refreshOnTick dedups internally, so no local tick mirror is needed.
  $effect(() => {
    const tick = coreEvents.statusTick;
    if (tick > 0) {
      guiState.refreshOnTick(tick);
    }
  });
</script>

<main
  class="h-screen w-screen flex flex-col select-none overflow-hidden transition-colors duration-200"
  style="background: var(--background); color: var(--foreground); font-family: var(--font-sans, system-ui);"
>
  <!-- Title bar: 44px, drag region -->
  <TitleBar />

  <!-- Nav header: 38px -->
  <div class="flex-shrink-0 px-5 pt-2.5">
    <AppHeader />
  </div>

  <!-- Separator -->
  <div
    class="flex-shrink-0 mx-5"
    style="height: 1px; background: var(--border); opacity: 0.5;"
  ></div>

  <!-- Main content area -->
  <div class="flex-1 min-h-0 px-3 sm:px-5 py-2 sm:py-3.5 flex flex-col overflow-hidden">
    {#if store.appLoading}
      <!-- Loading screen -->
      <div
        class="flex-1 flex flex-col items-center justify-center gap-5"
        transition:fade={{ duration: 200 }}
      >
        <div class="loading-logo-ring">
          <div class="loading-logo-inner">
            <AppLogo size={36} class="loading-logo" />
          </div>
        </div>
        <div class="flex flex-col items-center gap-2">
          <span class="loading-title">ZNet Sink</span>
          <div class="flex items-center gap-2">
            <Spinner size="sm" color="default" />
            <span class="loading-hint">{'正在加载配置...'}</span>
          </div>
        </div>
      </div>
    {:else if store.loadError}
      <div
        class="flex-1 flex flex-col items-center justify-center gap-3"
        transition:fade={{ duration: 200 }}
      >
        <span style="font-size: 14px; color: var(--destructive); font-weight: 600;"
          >{'启动失败'}</span
        >
        <span
          style="font-size: 12px; color: var(--muted-foreground); max-width: 360px; text-align: center;"
          >{store.loadError}</span
        >
        <button
          class="retry-btn"
          onclick={() => {
            store.loadError = null;
            store.appLoading = true;
            store.loadFromBackend();
          }}>{'重试'}</button
        >
      </div>
    {:else if !store.isInitialized}
      <WelcomeGuide />
    {:else}
      {#key store.activeTab}
        <div class="flex-1 min-h-0 flex flex-col" transition:fade={{ duration: 160 }}>
          <TabContent />
        </div>
      {/key}
    {/if}
  </div>

  {#if (updater.updateAvailable || updater.downloading) && store.isInitialized}
    <div class="global-update-shell">
      <UpdateBanner />
    </div>
  {/if}

</main>

<style>
  /* Loading screen */
  .loading-logo-ring {
    width: 72px;
    height: 72px;
    border-radius: 50%;
    background: conic-gradient(
      from 0deg,
      var(--primary) 0deg,
      var(--accent) 120deg,
      var(--muted) 240deg,
      var(--primary) 360deg
    );
    animation: loading-ring-spin 1.8s linear infinite;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2.5px;
  }

  .loading-logo-inner {
    width: 100%;
    height: 100%;
    border-radius: 50%;
    background: var(--background);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .loading-logo-ring {
    --logo-radius: 8px;
  }

  .loading-title {
    font-size: 15px;
    font-weight: 700;
    color: var(--foreground);
    letter-spacing: 0.01em;
  }

  .loading-hint {
    font-size: 12.5px;
    color: var(--muted-foreground);
    font-weight: 450;
  }

  @keyframes loading-ring-spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  /* Retry button */
  .retry-btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--card);
    color: var(--foreground);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.13s ease, box-shadow 0.13s ease;
  }

  .retry-btn:hover {
    background: var(--muted);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
  }

  .global-update-shell {
    flex-shrink: 0;
    padding: 0 20px 14px;
  }

  @media (max-width: 640px) {
    .global-update-shell {
      padding: 0 12px 12px;
    }
  }
</style>
