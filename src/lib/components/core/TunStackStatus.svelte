<script lang="ts">
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { getGuiStackStatus } from '$lib/services/core';
  import type { GuiFeatureStatus } from '$lib/types/gui-api';
  import type { GuiManagedTunStatus } from '$lib/types/tun';
  import { store } from '$lib/services/store.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { Switch } from '$lib/components/ui/switch';

  let stackStatus = $state<GuiFeatureStatus | null>(null);
  let mounted = $state(false);

  const tunRuntime = $derived(guiState.tunStatus as GuiManagedTunStatus | null);

  // The queried status belongs to the current Core instance and is
  // authoritative. Event state is deliberately not allowed to outrank it,
  // because an old event generation may still say "started" during restart.
  const tunLabel = $derived(
    guiState.tunStatusError ? '状态未知' :
    !guiState.tunStatus ? '—' :
    guiState.isSwitchingTun ? '切换中' :
    guiState.tunStatus.enabled ? '活跃' :
    guiState.tunStatus.lastError ? '异常' :
    guiState.tunStatus.supported ? '未开启' : '不支持'
  );

  const tunDotColor = $derived(
    guiState.tunStatusError ? '#F59E0B' :
    guiState.tunStatus?.enabled ? '#22C55E' :
    guiState.tunStatus?.lastError ? '#EF4444' :
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
    if (!tunRuntime) return null;
    const source = tunRuntime.configSource === 'profile'
      ? `配置：${tunRuntime.configSourceName ?? '当前配置'}`
      : tunRuntime.configSource === 'app'
        ? '来源：ZNet-Sink 缺省'
        : tunRuntime.configSource === 'runtime'
          ? '来源：临时运行态'
          : null;
    const address = tunRuntime.enabled ? (tunRuntime.addresses?.[0] ?? tunRuntime.addr) : null;
    const egress = tunRuntime.enabled
      ? (tunRuntime.egressInterface ?? tunRuntime.egressInterfaceV4 ?? tunRuntime.egressInterfaceV6)
      : null;
    return [source, address, egress].filter(Boolean).join(' · ') || null;
  });

  function egressLabel(family: 'IPv4' | 'IPv6', egress: GuiManagedTunStatus['ipv4Egress']): string {
    const state = egress.availability === 'available'
      ? '可用'
      : egress.availability === 'unavailable'
        ? '不可用'
        : '检测中';
    const detail = egress.interface ?? egressReason(egress.reason);
    return `${family} ${state}${detail ? `（${detail}）` : ''}`;
  }

  function egressReason(reason?: string): string | null {
    if (!reason) return null;
    const labels: Record<string, string> = {
      no_default_route: '无默认路由',
      no_usable_address: '无可用地址',
      interface_down: '接口未连接',
      route_lookup_failed: '路由探测失败',
    };
    return labels[reason] ?? reason;
  }

  const tunDiagnostics = $derived.by(() => {
    if (!tunRuntime?.enabled) return [];
    const values = [
      egressLabel('IPv4', tunRuntime.ipv4Egress),
      egressLabel('IPv6', tunRuntime.ipv6Egress),
      tunRuntime.addressFamilyPolicy ? `策略 ${tunRuntime.addressFamilyPolicy}` : null,
      tunRuntime.dnsHijack
        ? `DNS 劫持 ${tunRuntime.dnsHijackedQueries} 次${tunRuntime.fakeIpEnabled ? ' · Fake-IP' : ' · Real-IP'}`
        : 'DNS 劫持关闭',
      `Gen ${tunRuntime.networkGeneration}`,
      tunRuntime.ipv6ToIpv4Fallbacks > 0
        ? `IPv6→IPv4 回退 ${tunRuntime.ipv6ToIpv4Fallbacks} 次`
        : null,
    ];
    return values.filter((value): value is string => Boolean(value));
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
        checked={guiState.isTunSwitchOn}
        onCheckedChange={() => guiState.toggleTun()}
        disabled={guiState.isTunSwitchOn ? !guiState.canDisableTun : !guiState.canEnableTun}
        aria-label={guiState.isTunSwitchOn ? '关闭 TUN 并取消自动恢复' : '开启 TUN'}
      />
    </div>

    {#if guiState.tunStatusError || (guiState.isTunDesiredEnabled && !guiState.isTunEnabled)}
      <p class="feature-meta" role="status">
        {guiState.tunStatusError ? '暂时无法确认 TUN 状态。' : 'TUN 尚未运行。'}
        {guiState.isTunDesiredEnabled ? '已保存开启设置，重启后会尝试恢复；关闭开关可取消。' : '请刷新确认运行状态。'}
      </p>
    {/if}
    {#if tunDiagnostics.length > 0}
      <div class="tun-diagnostics" aria-label="TUN 出口诊断">
        {#each tunDiagnostics as diagnostic}
          <span class="diagnostic-chip" title={diagnostic}>{diagnostic}</span>
        {/each}
      </div>
    {/if}

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
  .tun-diagnostics { display: flex; flex-wrap: wrap; gap: 4px; padding-left: 14px; }
  .diagnostic-chip { max-width: 100%; overflow: hidden; padding: 2px 5px; color: var(--muted-foreground); background: color-mix(in srgb, var(--muted) 70%, transparent); border-radius: 4px; font-size: 9.5px; line-height: 1.25; text-overflow: ellipsis; white-space: nowrap; }
  .feature-error { overflow: hidden; color: var(--destructive); font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
</style>
