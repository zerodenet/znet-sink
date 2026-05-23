<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import CoreStatusCard from '$lib/components/core/CoreStatusCard.svelte';
  import LogPanel from '$lib/components/core/LogPanel.svelte';
  import { Badge } from '$lib/components/ui/badge';

  function formatSpeed(bytesPerSec: number): string {
    if (!bytesPerSec || bytesPerSec < 0) return '0 B/s';
    if (bytesPerSec >= 1024 * 1024) {
      return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
    }
    if (bytesPerSec >= 1024) {
      return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
    }
    return `${bytesPerSec} B/s`;
  }

  function formatUptime(ms?: number): string {
    if (!ms) return '-';
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    if (hours > 0) return `${hours}h ${minutes % 60}m`;
    if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
    return `${seconds}s`;
  }

  let testExpanded = $state(false);

  const PROXY_MODES = [
    { value: 'global', label: '全局' },
    { value: 'rule',   label: '规则' },
    { value: 'direct', label: '直连' },
  ] as const;
</script>

{#if store.uiMode === 'pro'}
  <!-- ============ PRO MODE ============ -->
  <div class="flex-1 w-full flex flex-col gap-3 overflow-y-auto overflow-x-hidden animate-fade-in min-h-0 pr-0.5">

    <!-- Row 1: Status cards -->
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 flex-shrink-0">
      <CoreStatusCard />

      <!-- Connection state -->
      <div class="overview-card flex flex-col gap-2 overflow-hidden" style="min-height: 96px;">
        <div class="flex items-center justify-between flex-shrink-0">
          <span class="card-label">连接状态</span>
          {#if guiState.connection?.state === 'connected'}
            <span class="status-chip active">已连接</span>
          {:else}
            <span class="status-chip">未连接</span>
          {/if}
        </div>

        {#if guiState.connection?.state === 'connected'}
          <div class="flex items-center gap-1 text-muted-foreground mt-auto flex-wrap" style="font-size: 12px;">
            <span>在线时长：</span>
            <span class="font-mono font-semibold text-foreground">{formatUptime(guiState.connection.uptimeMs)}</span>
            <button
              onclick={guiState.disconnect}
              disabled={!guiState.canDisconnect || guiState.isDisconnecting}
              class="pro-disconnect-btn"
            >
              {guiState.isDisconnecting ? '断开中…' : '断开'}
            </button>
          </div>
        {:else}
          <div class="mt-auto flex justify-center">
            <button
              onclick={guiState.connect}
              disabled={!guiState.canConnect || guiState.isConnecting}
              class="connect-btn"
              aria-label="一键连接"
            >
              {guiState.isConnecting ? '连接中…' : '一键连接'}
            </button>
          </div>
        {/if}
      </div>

      <!-- Proxy mode -->
      <div class="overview-card flex flex-col gap-2 overflow-hidden" style="min-height: 96px;">
        <div class="flex items-center justify-between flex-shrink-0">
          <span class="card-label">代理模式</span>
          {#if guiState.proxyMode?.currentMode}
            <span class="mode-indicator">{guiState.proxyMode.currentMode === 'global' ? '全局' : guiState.proxyMode.currentMode === 'rule' ? '规则' : '直连'}</span>
          {/if}
        </div>

        <div class="mt-auto">
          <div class="proxy-segment" role="radiogroup" aria-label="选择代理模式">
            {#each PROXY_MODES as mode}
              <button
                role="radio"
                onclick={() => guiState.setProxyMode(mode.value as any)}
                disabled={guiState.isSwitchingMode}
                class="proxy-seg-btn {guiState.proxyMode?.currentMode === mode.value ? 'active' : ''}"
                aria-checked={guiState.proxyMode?.currentMode === mode.value}
              >
                {mode.label}
              </button>
            {/each}
          </div>
        </div>
      </div>
    </div>

    <!-- Row 2: Self-test -->
    <div class="overview-card flex-shrink-0">
      <button class="flex items-center justify-between w-full cursor-pointer" onclick={() => testExpanded = !testExpanded} style="background: none; border: none; padding: 0; color: inherit;">
        <span class="card-label">系统自测</span>
        <div class="flex items-center gap-2">
          {#if guiState.selfTest}
            {#if guiState.selfTest.ready}
              <span class="inline-flex items-center gap-1 text-success" style="font-size: 12px; font-weight: 600;">
                <svg width="12" height="12" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="1.5 5 4 7.5 8.5 2.5"/></svg>
                就绪
              </span>
            {:else}
              <span class="inline-flex items-center gap-1 text-destructive" style="font-size: 12px; font-weight: 600;">
                <svg width="12" height="12" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><line x1="2" y1="2" x2="8" y2="8"/><line x1="8" y1="2" x2="2" y2="8"/></svg>
                未就绪
              </span>
            {/if}
            {#if guiState.selfTest.warningCount > 0}
              <span class="text-warning" style="font-size: 11px;">{guiState.selfTest.warningCount} 警告</span>
            {/if}
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="expand-chevron" class:expanded={testExpanded}>
              <polyline points="3 5 7 9 11 5"/>
            </svg>
          {:else}
            <span style="font-size: 11px; color: var(--muted-foreground);">检测中…</span>
          {/if}
        </div>
      </button>

      {#if guiState.selfTest?.blockingIssues?.length}
        <div class="mt-2 space-y-0.5">
          {#each guiState.selfTest.blockingIssues as issue}
            <div class="text-destructive" style="font-size: 12px;">• {issue}</div>
          {/each}
        </div>
      {/if}

      {#if testExpanded && guiState.selfTest?.checks?.length}
        <div class="test-checks">
          {#each guiState.selfTest.checks as check}
            <div class="test-check-row">
              {#if check.status === 'pass'}
                <svg width="12" height="12" viewBox="0 0 10 10" fill="none" stroke="#22C55E" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="flex-shrink-0 mt-0.5"><polyline points="1.5 5 4 7.5 8.5 2.5"/></svg>
              {:else if check.status === 'warn'}
                <svg width="12" height="12" viewBox="0 0 10 10" fill="none" stroke="#F59E0B" stroke-width="1.6" stroke-linecap="round" class="flex-shrink-0 mt-0.5"><path d="M5 1.2L9 8.8H1Z"/><line x1="5" y1="4" x2="5" y2="6"/><circle cx="5" cy="7.5" r="0.4" fill="#F59E0B"/></svg>
              {:else}
                <svg width="12" height="12" viewBox="0 0 10 10" fill="none" stroke="#EF4444" stroke-width="1.6" stroke-linecap="round" class="flex-shrink-0 mt-0.5"><line x1="2" y1="2" x2="8" y2="8"/><line x1="8" y1="2" x2="2" y2="8"/></svg>
              {/if}
              <div class="test-check-info">
                <span class="test-check-name">{check.key}</span>
                {#if check.message}
                  <span class="test-check-msg">{check.message}</span>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Row 3: Traffic stats -->
    {#if store.isFeatureVisible('trafficStats')}
      <div class="overview-card flex-shrink-0">
        <div class="flex items-center justify-between mb-2">
          <span class="card-label">实时流量</span>
        </div>
        {#if guiState.trafficStats}
          <div class="grid grid-cols-2 gap-2">
            <div class="traffic-metric down">
              <span class="traffic-value">{formatSpeed(guiState.trafficStats.downloadBytesPerSec)}</span>
              <span class="traffic-label">下行速率</span>
            </div>
            <div class="traffic-metric up">
              <span class="traffic-value">{formatSpeed(guiState.trafficStats.uploadBytesPerSec)}</span>
              <span class="traffic-label">上行速率</span>
            </div>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Row 4: Chart + Current node -->
    {#if store.isFeatureVisible('policySelection')}
      <div class="flex-1 w-full flex flex-col lg:flex-row gap-3 overflow-hidden min-h-0" style="min-height: 180px;">
        <div class="w-full lg:w-2/3 overflow-hidden min-h-[120px]">
          <TrafficChart history={overviewData.speedHistory} />
        </div>
        <div class="w-full lg:w-1/3 min-h-[80px]">
          <div class="overview-card h-full flex flex-col gap-2">
            <div class="flex items-center justify-between flex-shrink-0">
              <span class="card-label">当前节点</span>
              <button
                class="node-link-btn"
                onclick={() => store.activeTab = 'nodes'}
                aria-label="管理节点"
              >
                管理
              </button>
            </div>
            {#if overviewData.proxyNodes.length > 0}
              {@const activeNode = overviewData.proxyNodes.find(n => n.domain === 'selected') ?? overviewData.proxyNodes[0]}
              <div class="flex-1 flex flex-col justify-center min-h-0">
                <span class="active-node-name truncate">{activeNode.name}</span>
                <span class="active-node-meta">{activeNode.protocol} · {activeNode.delay > 0 ? `${activeNode.delay} ms` : '延迟未知'}</span>
              </div>
            {:else}
              <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">
                等待节点数据…
              </div>
            {/if}
          </div>
        </div>
      </div>
    {/if}

    <!-- Row 5: Log panel -->
    {#if store.isNavVisible('logs')}
      <div style="min-height: 120px; max-height: 200px;" class="flex-shrink-0 flex-1 min-h-0">
        <LogPanel />
      </div>
    {/if}

  </div>

{:else}
  <!-- ============ LITE MODE ============ -->
  <div class="flex-1 w-full flex flex-col gap-3 overflow-y-auto overflow-x-hidden animate-fade-in min-h-0 pr-0.5">

    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 flex-shrink-0">
      <CoreStatusCard />

      <!-- Connection -->
      <div class="overview-card flex flex-col gap-2" style="min-height: 96px;">
        <div class="flex items-center justify-between flex-shrink-0">
          <span class="card-label">连接状态</span>
          {#if guiState.connection?.state === 'connected'}
            <span class="status-chip active">已连接</span>
          {:else}
            <span class="status-chip">未连接</span>
          {/if}
        </div>
        <div class="mt-auto flex justify-center">
          {#if guiState.connection?.state === 'connected'}
            <button
              onclick={guiState.disconnect}
              disabled={!guiState.canDisconnect || guiState.isDisconnecting}
              class="disconnect-btn"
            >
              {guiState.isDisconnecting ? '断开中…' : '断开连接'}
            </button>
          {:else}
            <button
              onclick={guiState.connect}
              disabled={!guiState.canConnect || guiState.isConnecting}
              class="connect-btn"
            >
              {guiState.isConnecting ? '连接中…' : '一键连接'}
            </button>
          {/if}
        </div>
      </div>

      <!-- Proxy mode -->
      <div class="overview-card flex flex-col gap-2" style="min-height: 96px;">
        <div class="flex items-center justify-between flex-shrink-0">
          <span class="card-label">代理模式</span>
          {#if guiState.proxyMode?.currentMode}
            <span class="mode-indicator">{guiState.proxyMode.currentMode === 'global' ? '全局' : guiState.proxyMode.currentMode === 'rule' ? '规则' : '直连'}</span>
          {/if}
        </div>
        <div class="mt-auto">
          <div class="proxy-segment" role="radiogroup" aria-label="选择代理模式">
            {#each PROXY_MODES as mode}
              <button
                role="radio"
                onclick={() => guiState.setProxyMode(mode.value as any)}
                disabled={guiState.isSwitchingMode}
                class="proxy-seg-btn {guiState.proxyMode?.currentMode === mode.value ? 'active' : ''}"
                aria-checked={guiState.proxyMode?.currentMode === mode.value}
              >
                {mode.label}
              </button>
            {/each}
          </div>
        </div>
      </div>
    </div>

    <!-- Self-test -->
    <div class="overview-card flex-shrink-0">
      <button class="flex items-center justify-between w-full cursor-pointer" onclick={() => testExpanded = !testExpanded} style="background: none; border: none; padding: 0; color: inherit;">
        <span class="card-label">系统自测</span>
        <div class="flex items-center gap-2">
          {#if guiState.selfTest}
            {#if guiState.selfTest.ready}
              <span class="inline-flex items-center gap-1 text-success" style="font-size: 12px; font-weight: 600;">
                <svg width="12" height="12" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="1.5 5 4 7.5 8.5 2.5"/></svg>
                就绪
              </span>
            {:else}
              <span class="inline-flex items-center gap-1 text-destructive" style="font-size: 12px; font-weight: 600;">
                <svg width="12" height="12" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><line x1="2" y1="2" x2="8" y2="8"/><line x1="8" y1="2" x2="2" y2="8"/></svg>
                未就绪
              </span>
            {/if}
            {#if guiState.selfTest.warningCount > 0}
              <span class="text-warning" style="font-size: 11px;">{guiState.selfTest.warningCount} 警告</span>
            {/if}
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="expand-chevron" class:expanded={testExpanded}>
              <polyline points="3 5 7 9 11 5"/>
            </svg>
          {:else}
            <span style="font-size: 11px; color: var(--muted-foreground);">检测中…</span>
          {/if}
        </div>
      </button>
      {#if guiState.selfTest?.blockingIssues?.length}
        <div class="mt-2 space-y-0.5">
          {#each guiState.selfTest.blockingIssues as issue}
            <div class="text-destructive" style="font-size: 12px;">• {issue}</div>
          {/each}
        </div>
      {/if}
      {#if testExpanded && guiState.selfTest?.checks?.length}
        <div class="test-checks">
          {#each guiState.selfTest.checks as check}
            <div class="test-check-row">
              {#if check.status === 'pass'}
                <svg width="12" height="12" viewBox="0 0 10 10" fill="none" stroke="#22C55E" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="flex-shrink-0 mt-0.5"><polyline points="1.5 5 4 7.5 8.5 2.5"/></svg>
              {:else if check.status === 'warn'}
                <svg width="12" height="12" viewBox="0 0 10 10" fill="none" stroke="#F59E0B" stroke-width="1.6" stroke-linecap="round" class="flex-shrink-0 mt-0.5"><path d="M5 1.2L9 8.8H1Z"/><line x1="5" y1="4" x2="5" y2="6"/><circle cx="5" cy="7.5" r="0.4" fill="#F59E0B"/></svg>
              {:else}
                <svg width="12" height="12" viewBox="0 0 10 10" fill="none" stroke="#EF4444" stroke-width="1.6" stroke-linecap="round" class="flex-shrink-0 mt-0.5"><line x1="2" y1="2" x2="8" y2="8"/><line x1="8" y1="2" x2="2" y2="8"/></svg>
              {/if}
              <div class="test-check-info">
                <span class="test-check-name">{check.key}</span>
                {#if check.message}
                  <span class="test-check-msg">{check.message}</span>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="overview-card flex-shrink-0">
      <div class="flex items-center justify-between">
        <span class="card-label">当前节点</span>
        <button
          class="node-link-btn"
          onclick={() => store.activeTab = 'nodes'}
          aria-label="管理节点"
        >
          管理
        </button>
      </div>
      {#if overviewData.proxyNodes.length > 0}
        {@const activeNode = overviewData.proxyNodes.find(n => n.domain === 'selected') ?? overviewData.proxyNodes[0]}
        <div class="flex items-center gap-2 mt-2">
          <span class="active-node-name truncate">{activeNode.name}</span>
          <span class="active-node-meta">{activeNode.protocol} · {activeNode.delay > 0 ? `${activeNode.delay} ms` : '—'}</span>
        </div>
      {:else}
        <div class="mt-2 text-xs text-muted-foreground">等待节点数据…</div>
      {/if}
    </div>

  </div>
{/if}

<style>
  /* ---- Card base ---- */
  .overview-card {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px 14px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    transition: box-shadow 0.15s ease, transform 0.15s ease;
  }

  .overview-card:hover {
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.07);
    transform: translateY(-0.5px);
  }

  :global(.dark) .overview-card {
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.22);
  }

  :global(.dark) .overview-card:hover {
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.32);
  }

  .card-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--muted-foreground);
    letter-spacing: 0.01em;
  }

  /* ---- Status chip ---- */
  .status-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 600;
    padding: 3px 7px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--muted-foreground);
  }

  .status-chip.active {
    background: rgba(34, 197, 94, 0.1);
    color: #16A34A;
  }

  :global(.dark) .status-chip.active {
    background: rgba(74, 222, 128, 0.12);
    color: #4ADE80;
  }

  /* ---- Mode indicator ---- */
  .mode-indicator {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted-foreground);
    font-variant-numeric: tabular-nums;
  }

  /* ---- Connect button — compact, precise ---- */
  .connect-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 32px;
    min-width: 108px;
    max-width: 180px;
    padding: 0 18px;
    border-radius: 8px;
    border: none;
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    letter-spacing: -0.01em;
    transition: opacity 0.13s ease, transform 0.13s ease, box-shadow 0.13s ease;
    white-space: nowrap;
  }

  .connect-btn:hover:not(:disabled) {
    opacity: 0.88;
    transform: translateY(-0.5px);
  }

  .connect-btn:active:not(:disabled) {
    opacity: 0.78;
    transform: translateY(0);
  }

  .connect-btn:disabled {
    opacity: 0.38;
    cursor: not-allowed;
  }

  /* ---- Disconnect button (Lite) ---- */
  .disconnect-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 32px;
    min-width: 108px;
    max-width: 180px;
    padding: 0 18px;
    border-radius: 8px;
    border: 1px solid var(--destructive, rgba(239, 68, 68, 0.4));
    background: rgba(239, 68, 68, 0.08);
    color: var(--destructive, #EF4444);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    letter-spacing: -0.01em;
    transition: opacity 0.13s ease, transform 0.13s ease;
    white-space: nowrap;
  }

  .disconnect-btn:hover:not(:disabled) {
    opacity: 0.85;
    transform: translateY(-0.5px);
  }

  .disconnect-btn:disabled {
    opacity: 0.38;
    cursor: not-allowed;
  }

  /* ---- Pro inline disconnect button ---- */
  .pro-disconnect-btn {
    margin-left: 10px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 24px;
    padding: 0 10px;
    border-radius: 5px;
    border: 1px solid rgba(239, 68, 68, 0.3);
    background: rgba(239, 68, 68, 0.06);
    color: var(--destructive, #EF4444);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.13s ease;
    white-space: nowrap;
  }

  .pro-disconnect-btn:hover:not(:disabled) {
    opacity: 0.8;
  }

  .pro-disconnect-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  :global(.dark) .connect-btn {
    box-shadow: 0 0 0 0.5px rgba(255, 255, 255, 0.1), 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  /* ---- Proxy mode segmented control ---- */
  .proxy-segment {
    display: flex;
    align-items: center;
    gap: 1px;
    background: var(--segment-bg, rgba(0, 0, 0, 0.055));
    padding: 2px;
    border-radius: 7px;
    width: 100%;
  }

  .proxy-seg-btn {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 24px;
    border-radius: 5px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 11.5px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.13s ease;
    white-space: nowrap;
  }

  .proxy-seg-btn:hover:not(:disabled) {
    color: var(--foreground);
  }

  .proxy-seg-btn.active {
    background: var(--segment-active-bg, #ffffff);
    box-shadow: var(--segment-active-shadow, 0 1px 3px rgba(0,0,0,0.12));
    color: var(--foreground);
    font-weight: 600;
  }

  .proxy-seg-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* ---- Traffic metrics ---- */
  .traffic-metric {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 7px;
  }

  .traffic-metric.down {
    background: rgba(59, 130, 246, 0.06);
    border: 1px solid rgba(59, 130, 246, 0.1);
  }

  .traffic-metric.up {
    background: rgba(34, 197, 94, 0.06);
    border: 1px solid rgba(34, 197, 94, 0.1);
  }

  .traffic-value {
    font-size: 14px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    font-family: var(--font-mono, monospace);
  }

  .traffic-metric.down .traffic-value { color: #3B82F6; }
  .traffic-metric.up  .traffic-value { color: #22C55E; }

  :global(.dark) .traffic-metric.down .traffic-value { color: #60A5FA; }
  :global(.dark) .traffic-metric.up  .traffic-value { color: #4ADE80; }

  /* ---- Expand chevron ---- */
  .expand-chevron {
    transition: transform 0.2s ease;
    opacity: 0.5;
    flex-shrink: 0;
  }

  .expand-chevron.expanded {
    transform: rotate(180deg);
  }

  /* ---- Test checks ---- */
  .test-checks {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .test-check-row {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    font-size: 11.5px;
  }

  .test-check-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .test-check-name {
    font-weight: 600;
    color: var(--foreground);
  }

  .test-check-msg {
    color: var(--muted-foreground);
    font-size: 11px;
    line-height: 1.4;
    word-break: break-all;
  }

  /* ---- Current node indicator ---- */
  .node-link-btn {
    display: inline-flex;
    align-items: center;
    height: 22px;
    padding: 0 9px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted-foreground);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }

  .node-link-btn:hover {
    background: var(--muted);
    color: var(--foreground);
  }

  .active-node-name {
    font-size: 14px;
    font-weight: 700;
    color: var(--foreground);
  }

  .active-node-meta {
    font-size: 11.5px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
  }

  .traffic-label {
    font-size: 11px;
    color: var(--muted-foreground);
    margin-left: auto;
  }
</style>
