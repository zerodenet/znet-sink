<script lang="ts">
  import type { Component } from 'svelte';
  import { recordTelemetry } from '$lib/services/telemetry';
  import { store } from '$lib/services/store.svelte';
  import RuntimePerformance from '$lib/components/RuntimePerformance.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Spinner } from '$lib/components/ui/Spinner';

  let { tab }: { tab: string } = $props();

  type ComponentModule = { default: Component };
  type Loader = () => Promise<ComponentModule>;

  const loaders: Record<string, Loader> = {
    overview: () => import('./tabs/OverviewTab.svelte'),
    nodes: () => import('./tabs/NodesTab.svelte'),
    profiles: () => import('./tabs/ProfilesTab.svelte'),
    subscriptions: () => import('./tabs/SubscriptionsTab.svelte'),
    rules: () => import('./tabs/RulesTab.svelte'),
    connections: () => import('./tabs/ConnectionsTab.svelte'),
    logs: () => import('./tabs/LogsTab.svelte'),
    settings: () => import('./SettingsPanel.svelte'),
    capabilities: () => import('./tabs/CapabilitiesTab.svelte'),
    debug: () => import('./tabs/DebugTab.svelte'),
  };

  let ActiveComponent = $state<Component | null>(null);
  let activeProps = $state<Record<string, unknown>>({});
  let loadError = $state<string | null>(null);
  let requestId = 0;

  function reloadApplication() {
    window.location.reload();
  }

  $effect(() => {
    const currentRequest = ++requestId;
    ActiveComponent = null;
    activeProps = {};
    loadError = null;
    const startedAt = performance.now();
    const loader = loaders[tab] ?? (() => import('./tabs/PlaceholderTab.svelte'));

    void loader()
      .then((module) => {
        if (currentRequest !== requestId) return;
        ActiveComponent = module.default;
        activeProps = loaders[tab] ? {} : { label: tab };
        void recordTelemetry({
          level: 'debug',
          area: 'ui',
          operation: 'tab.load',
          message: `tab ${tab} loaded`,
          durationMs: Math.round(performance.now() - startedAt),
          context: { tab },
        });
      })
      .catch((error) => {
        if (currentRequest !== requestId) return;
        loadError = error instanceof Error ? error.message : String(error);
        void recordTelemetry({
          level: 'error',
          area: 'ui',
          operation: 'tab.load',
          message: loadError,
          durationMs: Math.round(performance.now() - startedAt),
          context: { tab },
        });
      });
  });
</script>

{#if loadError}
  <div class="tab-load-state error">
    <strong>页面加载失败</strong>
    <span>{loadError}</span>
    <Button variant="outline" size="sm" onclick={reloadApplication}>重新加载</Button>
  </div>
{:else if ActiveComponent}
  {#if tab === 'overview' && store.uiMode === 'lite'}
    <div class="overview-runtime-shell">
      <RuntimePerformance mode="lite" />
      <div class="overview-runtime-content">
        <ActiveComponent {...activeProps} />
      </div>
    </div>
  {:else}
    <ActiveComponent {...activeProps} />
  {/if}
{:else}
  <div class="tab-load-state">
    <Spinner size="sm" color="default" />
    <span>正在加载页面…</span>
  </div>
{/if}

<style>
  .tab-load-state {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--muted-foreground);
    font-size: 12px;
  }

  .tab-load-state.error {
    flex-direction: column;
    color: var(--destructive);
    text-align: center;
    overflow-wrap: anywhere;
  }

  .overview-runtime-shell {
    flex: 1;
    min-width: 0;
    min-height: 0;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .overview-runtime-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }
</style>
