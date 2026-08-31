<script lang="ts">
  import { RefreshCw } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { getAppErrorMessage, getGuiCapabilitiesSnapshot, getGuiZeroCapabilities } from '$lib/services/core';
  import type { GuiCapabilitySnapshot } from '$lib/types/capability';
  import type { GuiCapabilityState, GuiZeroCapabilities } from '$lib/types/gui-api';

  let snapshot = $state<GuiCapabilitySnapshot | null>(null);
  let kernelCaps = $state<GuiZeroCapabilities | null>(null);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let partialError = $state<string | null>(null);
  let refreshGeneration = 0;
  const contractRows = $derived(kernelCaps?.contracts
    ? [
        { label: '能力', range: kernelCaps.contracts.capabilities },
        { label: '控制 API', range: kernelCaps.contracts.controlApi },
        { label: '配置', range: kernelCaps.contracts.configSchema },
        { label: '错误码', range: kernelCaps.contracts.errorCodes },
      ]
    : []);

  async function refresh() {
    const generation = ++refreshGeneration;
    loading = true;
    const [capSnap, zeroCaps] = await Promise.allSettled([
      getGuiCapabilitiesSnapshot(),
      getGuiZeroCapabilities(),
    ]);
    if (generation !== refreshGeneration) return;

    if (capSnap.status === 'fulfilled') snapshot = capSnap.value;
    if (zeroCaps.status === 'fulfilled') kernelCaps = zeroCaps.value;
    const errors = [capSnap, zeroCaps]
      .filter((result): result is PromiseRejectedResult => result.status === 'rejected')
      .map((result) => getAppErrorMessage(result.reason, '能力查询失败'));
    loadError = errors.length === 2 ? errors.join('；') : null;
    partialError = errors.length === 1 ? `部分能力未能加载：${errors[0]}` : null;
    loading = false;
  }

  function statusColor(status: string): string {
    switch (status) {
      case 'supported': return 'bg-green-500';
      case 'partial': return 'bg-yellow-500';
      case 'experimental': return 'bg-orange-500';
      case 'unsupported': return 'bg-red-500';
      default: return 'bg-muted';
    }
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'supported': return '支持';
      case 'partial': return '部分';
      case 'experimental': return '实验';
      case 'unsupported': return '不支持';
      default: return status;
    }
  }

  function capabilityStateTitle(state: GuiCapabilityState): string {
    const detail = [statusLabel(state.level), ...state.notes].filter(Boolean);
    return detail.join(' · ');
  }

  $effect(() => {
    refresh();
  });
</script>

<div class="flex-1 w-full bg-card border border-card-border rounded-xl p-4 flex flex-col gap-4 animate-fade-in overflow-hidden">
  <div class="flex items-center justify-between flex-shrink-0">
    <h3 class="text-sm font-bold text-foreground">能力快照</h3>
    <Button
      onclick={refresh}
      disabled={loading}
      size="sm"
    >
      <RefreshCw class={loading ? 'animate-spin' : undefined} />
      {loading ? '刷新中...' : '刷新'}
    </Button>
  </div>

  {#if partialError}
    <div class="capability-warning" role="status">{partialError}</div>
  {/if}
  {#if loadError && (snapshot || kernelCaps)}
    <div class="capability-warning error" role="alert">刷新失败，当前仍显示上一批能力：{loadError}</div>
  {/if}

  {#if loading && !snapshot && !kernelCaps}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">加载中...</div>
  {:else if loadError && !snapshot && !kernelCaps}
    <div class="flex-1 flex flex-col gap-2 items-center justify-center text-xs text-destructive" role="alert">
      <span>能力快照加载失败：{loadError}</span>
      <Button variant="outline" size="sm" onclick={refresh}>重试</Button>
    </div>
  {:else}
    <div class="flex-1 overflow-y-auto min-h-0 space-y-4">
      <!-- 内核协议能力矩阵 -->
      {#if kernelCaps && kernelCaps.protocols.length > 0}
        <div>
          <h4 class="text-xs font-medium text-foreground mb-2">协议能力矩阵</h4>
          <div class="overflow-x-auto">
            <table class="w-full text-[10px]">
              <thead>
                <tr class="text-muted-foreground border-b border-card-border">
                  <th class="text-left py-1 pr-2">协议</th>
                  <th class="text-center py-1 px-1">状态</th>
                  <th class="text-center py-1 px-1">入站TCP</th>
                  <th class="text-center py-1 px-1">入站UDP</th>
                  <th class="text-center py-1 px-1">出站TCP</th>
                  <th class="text-center py-1 px-1">出站UDP</th>
                  <th class="text-center py-1 px-1">MUX</th>
                  <th class="text-left py-1 pl-2">限制</th>
                </tr>
              </thead>
              <tbody>
                {#each kernelCaps.protocols as proto (proto.name)}
                  <tr class="border-b border-card-border/50">
                    <td class="py-1 pr-2 font-medium text-foreground">{proto.name}</td>
                    <td class="py-1 px-1 text-center">
                      <span class="inline-flex items-center gap-1">
                        <span class="w-1.5 h-1.5 rounded-full {statusColor(proto.status)}"></span>
                        {statusLabel(proto.status)}
                      </span>
                    </td>
                    <td class="py-1 px-1 text-center" title={capabilityStateTitle(proto.inboundTcpState)}>{proto.inboundTcp ? '✓' : '—'}</td>
                    <td class="py-1 px-1 text-center" title={capabilityStateTitle(proto.inboundUdpState)}>{proto.inboundUdp ? '✓' : '—'}</td>
                    <td class="py-1 px-1 text-center" title={capabilityStateTitle(proto.outboundTcpState)}>{proto.outboundTcp ? '✓' : '—'}</td>
                    <td class="py-1 px-1 text-center" title={capabilityStateTitle(proto.outboundUdpState)}>{proto.outboundUdp ? '✓' : '—'}</td>
                    <td class="py-1 px-1 text-center" title={capabilityStateTitle(proto.muxState)}>{proto.mux ? '✓' : '—'}</td>
                    <td class="py-1 pl-2 text-muted-foreground">
                      {#if proto.limitations.length > 0}
                        {proto.limitations.join(', ')}
                      {:else}
                        —
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </div>
      {/if}

      {#if kernelCaps?.contracts}
        <div>
          <h4 class="text-xs font-medium text-foreground mb-2">稳定契约</h4>
          <div class="grid grid-cols-2 sm:grid-cols-4 gap-2">
            {#each contractRows as contract (contract.label)}
              <div class="bg-muted/30 border border-card-border rounded-lg p-2">
                <div class="text-[10px] text-muted-foreground">{contract.label}</div>
                <div class="text-xs font-mono text-foreground">v{contract.range.current} <span class="text-[9px] text-muted-foreground">最低 v{contract.range.minimumSupported}</span></div>
              </div>
            {/each}
          </div>
        </div>
      {:else if kernelCaps?.available}
        <div class="capability-warning" role="status">当前内核未发布稳定契约版本，客户端会使用保守的旧版兼容路径。</div>
      {/if}

      {#if kernelCaps && kernelCaps.globalLimitations.length > 0}
        <div>
          <h4 class="text-xs font-medium text-foreground mb-2">全局限制</h4>
          <div class="flex flex-wrap gap-1.5">
            {#each kernelCaps.globalLimitations as limitation (limitation)}
              <span class="px-2 py-0.5 rounded-md bg-yellow-500/10 text-[10px] text-yellow-700 dark:text-yellow-300 font-mono">{limitation}</span>
            {/each}
          </div>
        </div>
      {/if}

      {#if kernelCaps && kernelCaps.errorCodes.length > 0}
        <details class="bg-muted/20 border border-card-border rounded-lg p-2">
          <summary class="text-xs font-medium text-foreground cursor-pointer">稳定错误码目录（{kernelCaps.errorCodes.length}）</summary>
          <div class="flex flex-wrap gap-1.5 mt-2">
            {#each kernelCaps.errorCodes as code (code)}
              <span class="px-2 py-0.5 rounded-md bg-muted/50 text-[10px] text-foreground font-mono">{code}</span>
            {/each}
          </div>
        </details>
      {/if}

      <!-- 内核构建特性 -->
      {#if kernelCaps && kernelCaps.buildFeatures.length > 0}
        <div>
          <h4 class="text-xs font-medium text-foreground mb-2">内核构建特性</h4>
          <div class="flex flex-wrap gap-1.5">
            {#each kernelCaps.buildFeatures as feat (feat)}
              <span class="px-2 py-0.5 rounded-md bg-muted/50 text-[10px] text-foreground font-mono">{feat}</span>
            {/each}
          </div>
        </div>
      {/if}

      {#if snapshot}
        <!-- 管理能力 -->
        <div>
          <h4 class="text-xs font-medium text-foreground mb-2">管理能力</h4>
          <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2">
            {#each snapshot.management as item (item.key)}
              <div class="bg-muted/30 border border-card-border rounded-lg p-2 flex flex-col gap-1">
                <div class="flex items-center justify-between">
                  <span class="text-[10px] font-medium text-foreground">{item.key}</span>
                  <div class="w-1.5 h-1.5 rounded-full {item.enabled ? 'bg-green-500' : 'bg-muted'}"></div>
                </div>
              </div>
            {/each}
          </div>
        </div>

        <!-- 代理特性 -->
        <div>
          <h4 class="text-xs font-medium text-foreground mb-2">代理特性</h4>
          <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2">
            {#each snapshot.proxyFeatures as item (item.key)}
              <div class="bg-muted/30 border border-card-border rounded-lg p-2 flex flex-col gap-1">
                <div class="flex items-center justify-between">
                  <span class="text-[10px] font-medium text-foreground">{item.key}</span>
                  <div class="w-1.5 h-1.5 rounded-full {item.enabled ? 'bg-green-500' : 'bg-yellow-500'}"></div>
                </div>
                {#if item.reason}
                  <span class="text-[9px] text-muted-foreground">{item.reason}</span>
                {/if}
              </div>
            {/each}
          </div>
        </div>

        {#if snapshot.activeProxyConfigId}
          <div class="text-[10px] text-muted-foreground">
            活跃配置: <span class="font-mono">{snapshot.activeProxyConfigId}</span>
          </div>
        {/if}
      {/if}

      {#if !snapshot && !kernelCaps}
        <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">暂无可用能力</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .capability-warning {
    padding: 7px 9px;
    border: 1px solid color-mix(in srgb, var(--warning) 22%, var(--border));
    border-radius: 7px;
    background: color-mix(in srgb, var(--warning) 7%, transparent);
    color: var(--warning);
    font-size: 10.5px;
  }

  .capability-warning.error {
    border-color: color-mix(in srgb, var(--destructive) 22%, var(--border));
    background: color-mix(in srgb, var(--destructive) 7%, transparent);
    color: var(--destructive);
  }
</style>
