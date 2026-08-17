<script lang="ts">
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { getGuiStackStatus } from '$lib/services/core';
  import type { GuiFeatureStatus, GuiTunStatus } from '$lib/types/gui-api';
  import { store } from '$lib/services/store.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { Switch } from '$lib/components/ui/switch';

  let stackStatus = $state<GuiFeatureStatus | null>(null);
  let mounted = $state(false);

  const tunRuntime = $derived(guiState.tunStatus as GuiTunStatus | null);

  const tunLabel = $derived(
    !guiState.tunStatus ? '—' :
    guiState.isSwitchingTun ? '切换中' :
    coreEvents.tunState === 'started' ? '活跃' :
    coreEvents.tunState === 'error' ? '异常' :
    guiState.tunStatus.enabled ? '已开启' :
    guiState.tunStatus.supported ? '未开启' : '不支持'
  );

  const tunDotColor = $derived(
    coreEvents.tunState === 'started' ? '#22C55E' :
    coreEvents.tunState === 'error' ? '#EF4444' :
    guiState.isTunEnabled ? '#22C55E' :
    guiState.tunStatus?.supported ? '#F59E0B' : 'var(--muted-foreground)'
  );

  const stackLabel = $derived(
    !stackStatus ? '—' :
    coreEvents.stackState === 'started' ? coreEvents.stackMode ?? '已启动' :
    coreEvents.stackState === 'degraded' ? '降级' :
    stackStatus.supported ? '就绪' : '不支持'
  );

  const stackDotColor = $derived(
    coreEvents.stackState === 'started' ? '#22C55E' :
    coreEvents.stackState === 'degraded' ? '#F59E0B' :
    stackStatus?.supported ? '#F59E0B' : 'var(--muted-foreground)'
  );

  const runtimeSummary = $derived.by(() => {
    if (!tunRuntime?.enabled) return null;
    const address = tunRuntime.addresses?.[0] ?? tunRuntime.addr;
    const egress = tunRuntime.egressInterface
      ?? tunRuntime.egressInterfaceV4
      ?? tunRuntime.egressInterfaceV6;
    return [address, egress].filter(Boolean).join(' · ') || null;
  });

  async function refresh() {
    const [, stackResult] = await Promise.allSettled([
      guiState.refreshTunStatus(),
      getGuiStackStatus(),
    ]);
    if (stackResult.status === 'fulfilled') stackStatus = stackResult.value;
  }

  $effect(() => {
    if (store.isInitialized && !mounted) {
      mounted = true;
      refresh();
    }
  });

  $effect(() => {
    const tick = coreEvents.statusTick;
    if (tick > 0) refresh();
  });
</script>

<div class="feature-card">
  <div class="feature-header">
    <span class="feature-label">高级功能</span>
  </div>

  <div class="feature-grid">
    <div class="feature-row">
      <div class="feature-dot" style="background: {tunDotColor};"></div>
      <div class="feature-copy">
        <div class="feature-main">
          <span class="feature-name">TUN 网卡</span>
          <span class="feature-value">{tunLabel}</span>
        </div>
        {#if runtimeSummary}
          <span class="feature-meta" title={runtimeSummary}>{runtimeSummary}</span>
        {/if}
      </div>
      <Switch
        checked={guiState.isTunEnabled}
        onCheckedChange={() => guiState.toggleTun()}
        disabled={guiState.isTunEnabled ? !guiState.canDisableTun : !guiState.canEnableTun}
        aria-label={guiState.isTunEnabled ? '关闭 TUN' : '开启 TUN'}
      />
    </div>

    <div class="feature-row">
      <div class="feature-dot" style="background: {stackDotColor};"></div>
      <div class="feature-copy">
        <div class="feature-main">
          <span class="feature-name">内核网络栈</span>
          <span class="feature-value">{stackLabel}</span>
        </div>
      </div>
    </div>
  </div>

  {#if tunRuntime?.lastError}
    <div class="feature-error" title={tunRuntime.lastError}>{tunRuntime.lastError}</div>
  {:else if coreEvents.tunState === 'error' && coreEvents.tunStateMessage}
    <div class="feature-error" title={coreEvents.tunStateMessage}>{coreEvents.tunStateMessage}</div>
  {/if}
</div>

<style>
  .feature-card {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-height: 96px;
    padding: 11px 13px;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    overflow: hidden;
    transition: box-shadow 0.15s ease, transform 0.15s ease;
  }

  .feature-card:hover {
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.07);
    transform: translateY(-0.5px);
  }

  :global(.dark) .feature-card { box-shadow: 0 1px 3px rgba(0, 0, 0, 0.22); }
  :global(.dark) .feature-card:hover { box-shadow: 0 2px 8px rgba(0, 0, 0, 0.32); }

  .feature-header { display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; }
  .feature-label { font-size: 12px; font-weight: 500; color: var(--muted-foreground); }
  .feature-grid { display: flex; flex-direction: column; gap: 6px; flex-shrink: 0; }
  .feature-row { display: flex; align-items: center; gap: 7px; min-width: 0; }
  .feature-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .feature-copy { display: flex; min-width: 0; flex: 1; flex-direction: column; gap: 1px; }
  .feature-main { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .feature-name { min-width: 68px; color: var(--muted-foreground); font-size: 11.5px; font-weight: 500; }
  .feature-value { color: var(--foreground); font-size: 11.5px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .feature-meta { overflow: hidden; color: var(--muted-foreground); font-family: var(--font-mono); font-size: 10px; line-height: 1.35; text-overflow: ellipsis; white-space: nowrap; opacity: .78; }
  .feature-error { overflow: hidden; color: var(--destructive); font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
</style>
