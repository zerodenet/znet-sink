<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import NodeTileGrid from '$lib/components/NodeTileGrid.svelte';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import NodeSelector from '$lib/components/NodeSelector.svelte';
  import CoreStatusCard from '$lib/components/core/CoreStatusCard.svelte';
  import LogPanel from '$lib/components/core/LogPanel.svelte';
  import { Button } from '$lib/components/ui/button';
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
</script>

{#if store.uiMode === 'pro'}
  <div class="flex-1 w-full flex flex-col gap-3 overflow-y-auto overflow-x-hidden animate-fade-in min-h-0">
    <!-- 顶部状态卡片 -->
    <div class="grid grid-cols-3 gap-3 flex-shrink-0">
      <CoreStatusCard />

      <!-- 系统代理/连接状态 -->
      <div class="bg-card border border-card-border rounded-xl p-3 h-24 flex flex-col justify-between overflow-hidden shadow-sm transition-all duration-200 hover:shadow hover:-translate-y-0.5">
        <span class="text-sm font-medium text-muted-foreground truncate">
          {guiState.connection?.state === 'connected' ? '已连接' : '未连接'}
        </span>
        {#if guiState.connection?.state === 'connected'}
          <div class="text-xs text-foreground">
            运行时间：{formatUptime(guiState.connection.uptimeMs)}
          </div>
        {:else}
          <Button
            size="sm"
            onclick={guiState.connect}
            disabled={!guiState.canConnect || guiState.isConnecting}
            class="w-full"
          >
            {guiState.isConnecting ? '连接中...' : '一键连接'}
          </Button>
        {/if}
      </div>

      <!-- 代理模式 -->
      <div class="bg-card border border-card-border rounded-xl p-3 h-24 flex flex-col justify-between overflow-hidden shadow-sm transition-all duration-200 hover:shadow hover:-translate-y-0.5">
        <span class="text-sm font-medium text-muted-foreground truncate mb-1">
          代理模式：{guiState.proxyMode?.currentMode ?? '-'}
        </span>
        <div class="flex gap-1" role="radiogroup" aria-label="选择代理模式">
          {#each ['global', 'rule', 'direct'] as mode}
            <Button
              variant={guiState.proxyMode?.currentMode === mode ? 'default' : 'secondary'}
              size="sm"
              onclick={() => guiState.setProxyMode(mode as any)}
              disabled={guiState.isSwitchingMode}
              class="flex-1"
              aria-checked={guiState.proxyMode?.currentMode === mode}
            >
              {mode === 'global' ? '全局' : mode === 'rule' ? '规则' : '直连'}
            </Button>
          {/each}
        </div>
      </div>
    </div>

    <!-- 自测状态 -->
    <div class="bg-card border border-card-border rounded-xl p-3 shadow-sm transition-all duration-200 hover:shadow hover:-translate-y-0.5">
      <div class="text-sm font-medium text-muted-foreground mb-2">系统自测</div>
      {#if guiState.selfTest}
        <div class="text-xs">
          {#if guiState.selfTest.ready}
            <Badge variant="default" class="bg-emerald-500 text-white">✓ 就绪</Badge>
          {:else}
            <Badge variant="destructive">✗ 未就绪</Badge>
          {/if}
          {#if guiState.selfTest.warningCount > 0}
            <span class="ml-2 text-amber-400">({guiState.selfTest.warningCount} 个警告)</span>
          {/if}
          {#if guiState.selfTest.blockingIssues.length > 0}
            <div class="mt-1 text-red-400">
              {#each guiState.selfTest.blockingIssues as issue}
                <div>• {issue}</div>
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <div class="text-xs text-muted-foreground">加载中...</div>
      {/if}
    </div>

    <!-- 中间区域：流量看板 + 节点选择 -->
    {#if store.isFeatureVisible('trafficStats')}
      <div class="bg-card border border-card-border rounded-xl p-3 shadow-sm transition-all duration-200 hover:shadow hover:-translate-y-0.5">
        <div class="text-sm font-medium text-muted-foreground mb-3">流量统计</div>
        {#if guiState.trafficStats}
          <div class="grid grid-cols-2 gap-4">
            <div class="flex flex-col items-center p-3 rounded-xl bg-emerald-500/5 border border-emerald-500/10">
              <span class="text-sm font-bold text-emerald-400">
                {formatSpeed(guiState.trafficStats.downloadBytesPerSec)}
              </span>
              <span class="text-xs text-muted-foreground">下行速率</span>
            </div>
            <div class="flex flex-col items-center p-3 rounded-xl bg-blue-500/5 border border-blue-500/10">
              <span class="text-sm font-bold text-blue-400">
                {formatSpeed(guiState.trafficStats.uploadBytesPerSec)}
              </span>
              <span class="text-xs text-muted-foreground">上行速率</span>
            </div>
          </div>
        {/if}
      </div>
    {/if}

    {#if store.isFeatureVisible('policySelection')}
      <div class="flex-1 w-full flex gap-3 overflow-hidden min-h-[200px]">
        <div class="w-2/3 overflow-hidden">
          <TrafficChart history={overviewData.speedHistory} />
        </div>
        <NodeSelector nodes={overviewData.proxyNodes} />
      </div>
    {/if}

    <!-- 底部日志 -->
    {#if store.isNavVisible('logs')}
      <div class="h-40 flex-shrink-0">
        <LogPanel />
      </div>
    {/if}
  </div>
{:else}
  <div class="flex-1 w-full flex flex-col gap-3 overflow-y-auto overflow-x-hidden animate-fade-in min-h-0">
    <div class="grid grid-cols-3 gap-3 flex-shrink-0">
      <CoreStatusCard />

      <!-- 系统代理/连接状态 -->
      <div class="bg-card border border-card-border rounded-xl p-3 h-24 flex flex-col justify-between overflow-hidden shadow-sm transition-all duration-200 hover:shadow hover:-translate-y-0.5">
        <span class="text-sm font-medium text-muted-foreground truncate">
          {guiState.connection?.state === 'connected' ? '已连接' : '未连接'}
        </span>
        <Button
          size="sm"
          onclick={guiState.connect}
          disabled={!guiState.canConnect || guiState.isConnecting}
          class="w-full"
        >
          {guiState.isConnecting ? '连接中...' : '一键连接'}
        </Button>
      </div>

      <!-- 代理模式 -->
      <div class="bg-card border border-card-border rounded-xl p-3 h-24 flex flex-col justify-between overflow-hidden shadow-sm transition-all duration-200 hover:shadow hover:-translate-y-0.5">
        <span class="text-sm font-medium text-muted-foreground truncate mb-1">
          代理模式：{guiState.proxyMode?.currentMode ?? '-'}
        </span>
        <div class="flex gap-1" role="radiogroup" aria-label="选择代理模式">
          {#each ['global', 'rule', 'direct'] as mode}
            <Button
              variant={guiState.proxyMode?.currentMode === mode ? 'default' : 'secondary'}
              size="sm"
              onclick={() => guiState.setProxyMode(mode as any)}
              disabled={guiState.isSwitchingMode}
              class="flex-1"
              aria-checked={guiState.proxyMode?.currentMode === mode}
            >
              {mode === 'global' ? '全局' : mode === 'rule' ? '规则' : '直连'}
            </Button>
          {/each}
        </div>
      </div>
    </div>

    <!-- 自测状态 -->
    <div class="bg-card border border-card-border rounded-xl p-3 shadow-sm transition-all duration-200 hover:shadow hover:-translate-y-0.5">
      <div class="text-sm font-medium text-muted-foreground mb-2">系统自测</div>
      {#if guiState.selfTest}
        <div class="text-xs">
          {#if guiState.selfTest.ready}
            <Badge variant="default" class="bg-emerald-500 text-white">✓ 就绪</Badge>
          {:else}
            <Badge variant="destructive">✗ 未就绪</Badge>
          {/if}
          {#if guiState.selfTest.warningCount > 0}
            <span class="ml-2 text-amber-400">({guiState.selfTest.warningCount} 个警告)</span>
          {/if}
          {#if guiState.selfTest.blockingIssues.length > 0}
            <div class="mt-1 text-red-400">
              {#each guiState.selfTest.blockingIssues as issue}
                <div>• {issue}</div>
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <div class="text-xs text-muted-foreground">加载中...</div>
      {/if}
    </div>

    <div class="flex-1 overflow-hidden min-h-[200px]">
      <NodeTileGrid nodes={overviewData.proxyNodes} showCheck={true} />
    </div>
  </div>
{/if}
