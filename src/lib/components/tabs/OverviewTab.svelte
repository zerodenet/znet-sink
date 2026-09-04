<script lang="ts">
  import { onMount } from 'svelte';
  import { store } from '$lib/services/store.svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { getAppErrorMessage } from '$lib/services/core';
  import {
    listProxyConfigs,
    listSubscriptions,
    setActiveProxyConfig,
    syncSubscription,
    type ProxyConfigProfile,
    type SubscriptionProfile,
  } from '$lib/services/config';
  import { parseNodeName } from '$lib/services/node-utils';
  import { resolveEffectiveNodeSelection } from '$lib/components/tabs/nodes-view-model';
  import * as toast from '$lib/services/toast.svelte';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import CoreStatusCard from '$lib/components/core/CoreStatusCard.svelte';
  import KernelVersionCard from '$lib/components/core/KernelVersionCard.svelte';
  import TunStackStatus from '$lib/components/core/TunStackStatus.svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import * as Select from '$lib/components/ui/select';

  function formatUptime(ms?: number): string {
    if (!ms) return '—';
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    if (hours > 0) return `${hours}h ${minutes % 60}m`;
    if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
    return `${seconds}s`;
  }

  function formatSpeed(speed: number): string {
    if (speed >= 1) return `${speed.toFixed(2)} MB/s`;
    if (speed * 1000 >= 1) return `${(speed * 1000).toFixed(0)} KB/s`;
    return '0 KB/s';
  }

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
    if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
    if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
    if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(0)} KB`;
    return `${Math.round(bytes)} B`;
  }

  function trafficShare(part: number, total: number): number {
    // An empty session still needs a complete, stable ring. Start from an
    // intentional 50/50 composition and let real cumulative bytes move the
    // boundary once traffic exists.
    if (!Number.isFinite(part) || !Number.isFinite(total) || total <= 0) return 50;
    return Math.max(0, Math.min(100, (part / total) * 100));
  }

  function formatRelativeTime(timestamp?: number): string {
    if (!timestamp) return '未同步';
    const diffMs = Math.max(0, Date.now() - timestamp);
    const minutes = Math.floor(diffMs / 60_000);
    if (minutes < 1) return '刚刚同步';
    if (minutes < 60) return `${minutes} 分钟前同步`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours} 小时前同步`;
    const days = Math.floor(hours / 24);
    return `${days} 天前同步`;
  }

  function formatSubscriptionFormat(format: string): string {
    if (!format || format === 'auto') return '自动检测';
    if (format === 'zero') return 'Zero';
    if (format === 'clash') return 'Clash';
    return format;
  }

  function routeFinalOutbound(content?: unknown): string | null {
    if (!content || typeof content !== 'object' || Array.isArray(content)) return null;
    const route = (content as Record<string, unknown>).route;
    if (!route || typeof route !== 'object' || Array.isArray(route)) return null;
    const finalRoute = (route as Record<string, unknown>).final;
    if (!finalRoute || typeof finalRoute !== 'object' || Array.isArray(finalRoute)) return null;
    const outbound = (finalRoute as Record<string, unknown>).outbound;
    return typeof outbound === 'string' && outbound.trim() ? outbound : null;
  }

  let testExpanded = $state(false);
  let uptimeNowMs = $state(Date.now());
  let subscriptions = $state<SubscriptionProfile[]>([]);
  let proxyConfigs = $state<ProxyConfigProfile[]>([]);
  let sourceLoading = $state(true);
  let activatingSourceId = $state<string | null>(null);

  const PROXY_MODES = [
    { value: 'global', label: '全局' },
    { value: 'rule',   label: '规则' },
    { value: 'direct', label: '直连' },
  ] as const;

  const networkProbeResult = $derived(guiState.networkProbe);
  const networkProbeLoading = $derived(guiState.networkProbeLoading);
  const networkProbeError = $derived(guiState.networkProbeError);

  const COUNTRY_CODES: Record<string, string> = {
    '中国': 'CN', '美国': 'US', '日本': 'JP', '韩国': 'KR', '新加坡': 'SG',
    '香港': 'HK', '台湾': 'TW', '澳门': 'MO', '英国': 'GB', '德国': 'DE',
    '法国': 'FR', '加拿大': 'CA', '澳大利亚': 'AU', '俄罗斯': 'RU', '印度': 'IN',
    '巴西': 'BR', '荷兰': 'NL', '瑞典': 'SE', '瑞士': 'CH', '芬兰': 'FI',
    '挪威': 'NO', '丹麦': 'DK', '波兰': 'PL', '捷克': 'CZ', '奥地利': 'AT',
    '比利时': 'BE', '意大利': 'IT', '西班牙': 'ES', '葡萄牙': 'PT', '爱尔兰': 'IE',
    '新西兰': 'NZ', '墨西哥': 'MX', '阿根廷': 'AR', '智利': 'CL', '南非': 'ZA',
    '泰国': 'TH', '越南': 'VN', '马来西亚': 'MY', '印度尼西亚': 'ID', '菲律宾': 'PH',
    '阿联酋': 'AE', '沙特阿拉伯': 'SA', '以色列': 'IL', '土耳其': 'TR', '乌克兰': 'UA',
    'china': 'CN', 'united states': 'US', 'usa': 'US', 'japan': 'JP',
    'south korea': 'KR', 'korea': 'KR', 'singapore': 'SG', 'hong kong': 'HK',
    'taiwan': 'TW', 'united kingdom': 'GB', 'uk': 'GB', 'germany': 'DE',
    'france': 'FR', 'canada': 'CA', 'australia': 'AU', 'russia': 'RU',
    'india': 'IN', 'brazil': 'BR', 'netherlands': 'NL', 'sweden': 'SE',
    'switzerland': 'CH', 'finland': 'FI', 'norway': 'NO', 'denmark': 'DK',
    'poland': 'PL', 'czech republic': 'CZ', 'czechia': 'CZ', 'austria': 'AT',
    'belgium': 'BE', 'italy': 'IT', 'spain': 'ES', 'portugal': 'PT',
    'ireland': 'IE', 'new zealand': 'NZ', 'mexico': 'MX', 'argentina': 'AR',
    'chile': 'CL', 'south africa': 'ZA', 'thailand': 'TH', 'vietnam': 'VN',
    'malaysia': 'MY', 'indonesia': 'ID', 'philippines': 'PH',
    'united arab emirates': 'AE', 'saudi arabia': 'SA', 'israel': 'IL',
    'turkey': 'TR', 'ukraine': 'UA', 'macao': 'MO', 'macau': 'MO',
  };

  function getFlagUrl(country?: string): string | null {
    if (!country) return null;
    const value = country.trim();
    const code = value.length === 2
      ? value.toUpperCase()
      : COUNTRY_CODES[value.toLowerCase()] ?? COUNTRY_CODES[value];
    return code ? `https://flagcdn.com/w40/${code.toLowerCase()}.png` : null;
  }

  const networkProbeFlagUrl = $derived(getFlagUrl(networkProbeResult?.country));

  function formatProbeLocation(result: { country?: string; region?: string; city?: string }): string {
    const parts = [result.country, result.region, result.city].filter(Boolean);
    return parts.length > 0 ? parts.join(' · ') : '未知地区';
  }

  async function refreshLiteSource() {
    sourceLoading = true;
    const [subscriptionResult, configResult] = await Promise.allSettled([
      listSubscriptions(),
      listProxyConfigs(),
    ]);
    subscriptions = subscriptionResult.status === 'fulfilled' ? subscriptionResult.value : [];
    proxyConfigs = configResult.status === 'fulfilled' ? configResult.value : [];
    sourceLoading = false;
  }

  onMount(() => {
    void refreshLiteSource();
  });

  // Speed derived from history
  const currentDown = $derived(
    overviewData.speedHistory.length > 0
      ? overviewData.speedHistory[overviewData.speedHistory.length - 1].down
      : 0,
  );
  const currentUp = $derived(
    overviewData.speedHistory.length > 0
      ? overviewData.speedHistory[overviewData.speedHistory.length - 1].up
      : 0,
  );
  const sessionTotalBytes = $derived(overviewData.captureSessionTotalBytes);
  const sessionTotalLabel = $derived(formatBytes(sessionTotalBytes));
  const sessionDownLabel = $derived(formatBytes(overviewData.captureSessionDownBytes));
  const sessionUpLabel = $derived(formatBytes(overviewData.captureSessionUpBytes));
  const sessionUpShare = $derived(trafficShare(overviewData.captureSessionUpBytes, sessionTotalBytes));
  const sessionDownShare = $derived(100 - sessionUpShare);
  const sessionRingStyle = $derived(
    `--traffic-up-share: ${sessionUpShare.toFixed(3)}%; --traffic-down-share: ${sessionDownShare.toFixed(3)}%;`,
  );

  const systemProxyEnabled = $derived(guiState.isSystemProxyEnabled);
  const captureEnabled = $derived(guiState.isCaptureEnabled);
  const liteConnected = $derived(guiState.isConnected);
  const isPowerBusy = $derived(guiState.isConnecting || guiState.isDisconnecting);
  const hasConfig = $derived(guiState.configNodes.length > 0 || guiState.proxyMode != null);
  const hasNodes = $derived(guiState.policyGroups.length > 0 || guiState.configNodes.length > 0);
  const networkProbePlaceholder = $derived(
    networkProbeLoading ? '正在检测本地网络环境…' :
    networkProbeError ? '网络检测失败，请检查当前网络后重试' :
    '等待网络检测结果',
  );

  const activeProxyConfig = $derived(proxyConfigs.find((profile) => profile.active) ?? null);
  const activeSubscription = $derived.by(() => {
    const activeId = activeProxyConfig?.id;
    if (!activeId) return null;
    return subscriptions.find((subscription) => subscription.targetProxyConfigId === activeId) ?? null;
  });
  const sourceOptions = $derived(
    subscriptions.map((subscription) => ({ value: subscription.id, label: subscription.name })),
  );

  async function activateSource(id: string) {
    if (!id || activatingSourceId !== null || activeSubscription?.id === id) return;
    const subscription = subscriptions.find((item) => item.id === id);
    if (!subscription || !subscription.enabled) return;

    activatingSourceId = id;
    try {
      let current = subscription;
      let targetId = current.targetProxyConfigId;
      const hasUsableTarget = targetId
        ? proxyConfigs.some((profile) => profile.id === targetId && profile.content != null)
        : false;

      // A never-synced source is still a one-action choice: sync it first,
      // then activate the generated config. Existing cached configs do not
      // require network access merely to become active again.
      if (!hasUsableTarget) {
        current = await syncSubscription(id);
        targetId = current.targetProxyConfigId;
      }
      if (!targetId) {
        throw new Error('订阅尚未生成可用配置');
      }

      await setActiveProxyConfig(targetId);
      await Promise.allSettled([
        refreshLiteSource(),
        guiState.refreshAll(),
      ]);
      toast.success(`已切换到 ${current.name}`);
    } catch (error) {
      toast.error(getAppErrorMessage(error, '切换订阅失败'));
    } finally {
      activatingSourceId = null;
    }
  }

  // Compact mode names the effective leaf outbound, not the policy group that
  // happens to own it. Runtime `selected` values are authoritative for nested
  // selectors and URLTest groups, so follow them recursively and never infer a
  // winner from latency. This stays aligned with the Nodes page's runtime model.
  const activeNodeSummary = $derived.by(() => {
    const groups = guiState.policyGroups;
    const finalOutbound = routeFinalOutbound(activeProxyConfig?.content);
    const rootTag = finalOutbound
      ?? groups.find((group) => group.name.toLowerCase() === 'proxy')?.name
      ?? groups.find((group) => group.selected)?.name
      ?? null;
    const resolved = resolveEffectiveNodeSelection(groups, rootTag);

    if (resolved.leafTag) {
      const parsed = parseNodeName(resolved.leafTag);
      const parentKind = resolved.leafParentKind?.toLowerCase().replaceAll('-', '_');
      const urlTestSelected = parentKind === 'url_test' || parentKind === 'urltest';
      return {
        name: parsed.cleanName || resolved.leafTag,
        flagCode: parsed.flagCode,
        emoji: parsed.flagCode ? undefined : parsed.emoji,
        meta: resolved.groupPath.length > 0
          ? `归属：${resolved.groupPath.join(' → ')}${urlTestSelected ? ' · URLTest 实时选择' : ''}`
          : '当前配置直接出站',
      };
    }

    if (resolved.unresolvedGroupTag) {
      return {
        name: resolved.cycleDetected ? '策略链异常' : '等待内核选择',
        flagCode: undefined,
        emoji: undefined,
        meta: resolved.cycleDetected
          ? `循环策略链：${resolved.groupPath.join(' → ')}`
          : `策略组：${resolved.groupPath.join(' → ') || resolved.unresolvedGroupTag} · 暂无实时选中节点`,
      };
    }

    const fallback = guiState.configNodes.find((node) => !node.isSelector) ?? guiState.configNodes[0];
    const parsed = fallback ? parseNodeName(fallback.tag) : null;
    return {
      name: parsed?.cleanName || fallback?.tag || null,
      flagCode: parsed?.flagCode,
      emoji: parsed?.flagCode ? undefined : parsed?.emoji,
      meta: fallback ? '当前配置直接出站' : '暂无可用节点',
    };
  });
  const activeNodeName = $derived(activeNodeSummary.name);
  const activeNodeFlagCode = $derived(activeNodeSummary.flagCode);
  const activeNodeEmoji = $derived(activeNodeSummary.emoji);
  const activeNodeMeta = $derived(activeNodeSummary.meta);

  const sourceName = $derived.by(() => {
    if (sourceLoading) return '正在加载…';
    if (activeSubscription) return activeSubscription.name;
    if (activeProxyConfig) return activeProxyConfig.name;
    return subscriptions.length > 0 ? '选择订阅' : '添加订阅';
  });
  const sourceMeta = $derived.by(() => {
    if (sourceLoading) return '正在读取当前配置来源';
    if (activeSubscription) {
      const nodeCount = activeSubscription.nodeCount != null ? `${activeSubscription.nodeCount} 个节点` : '节点数未知';
      return `${formatSubscriptionFormat(activeSubscription.format)} · ${nodeCount} · ${formatRelativeTime(activeSubscription.lastSyncAtUnixMs)}`;
    }
    if (activeProxyConfig) {
      return `${activeProxyConfig.format || 'Zero'} · 本地/专业配置`;
    }
    if (subscriptions.length > 0) return `${subscriptions.length} 个订阅 · 请选择当前使用来源`;
    return '暂无订阅来源';
  });

  $effect(() => {
    const startedAt = guiState.connection?.startedAtUnixMs;
    if (!startedAt || store.uiMode !== 'pro') return;

    uptimeNowMs = Date.now();
    const timer = window.setInterval(() => {
      uptimeNowMs = Date.now();
    }, 1000);

    return () => window.clearInterval(timer);
  });

  const modeLabel = $derived(
    guiState.proxyMode?.currentMode === 'global' ? '全局' :
    guiState.proxyMode?.currentMode === 'direct' ? '直连' :
    guiState.proxyMode?.currentMode === 'rule' ? '规则' : '—',
  );
  const isCoreAvailable = $derived(
    guiState.connection?.coreAvailable === true || guiState.connection?.processState === 'running',
  );
  const coreStateLabel = $derived(
    captureEnabled ? '服务中' :
    guiState.isProcessRunning ? '监听中' :
    guiState.isStartingCore ? '启动中' :
    guiState.connection?.processState === 'failed' ? '失败' : '已停止',
  );
  const coreStateTone = $derived(
    captureEnabled ? 'on' :
    isCoreAvailable || guiState.isStartingCore ? 'listen' :
    guiState.connection?.processState === 'failed' ? 'error' : 'off',
  );
  const liveUptimeMs = $derived.by(() => {
    const startedAt = guiState.connection?.startedAtUnixMs;
    if (startedAt) {
      return Math.max(0, uptimeNowMs - startedAt);
    }
    return guiState.connection?.uptimeMs;
  });
  const uptimeLabel = $derived(formatUptime(liveUptimeMs));

</script>

{#if store.uiMode === 'pro'}
  <!-- ============ PRO MODE ============ -->
  <div class="flex-1 w-full flex flex-col gap-3 overflow-y-auto overflow-x-hidden animate-fade-in min-h-0 pr-0.5">
    <div class="status-strip flex-shrink-0" role="status" aria-label="运行状态概览">
      <div class="strip-item tone-{coreStateTone}" title="内核状态">
        <span class="strip-dot" class:pulse={guiState.isStartingCore || guiState.isConnecting}></span>
        <span class="strip-key">内核</span>
        <span class="strip-val">{coreStateLabel}</span>
      </div>
      <span class="strip-sep" aria-hidden="true"></span>
      <div class="strip-item {systemProxyEnabled ? 'tone-on' : 'tone-off'}" title="系统代理">
        <span class="strip-key">代理</span>
        <span class="strip-val">{systemProxyEnabled ? '已开启' : '未开启'}</span>
      </div>
      <span class="strip-sep" aria-hidden="true"></span>
      <div class="strip-item {guiState.isTunEnabled ? 'tone-on' : 'tone-off'}" title="TUN 虚拟网卡">
        <span class="strip-key">TUN</span>
        <span class="strip-val">{guiState.isTunEnabled ? '已开启' : '未开启'}</span>
      </div>
      <span class="strip-sep" aria-hidden="true"></span>
      <div class="strip-item" title="路由模式">
        <span class="strip-key">模式</span>
        <span class="strip-val">{modeLabel}</span>
      </div>
      <span class="strip-sep" aria-hidden="true"></span>
      <div class="strip-item down" title="实时下载速度">
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="2 5 6 9 10 5"/></svg>
        <span class="strip-val">{formatSpeed(currentDown)}</span>
      </div>
      <div class="strip-item up" title="实时上传速度">
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="2 7 6 3 10 7"/></svg>
        <span class="strip-val">{formatSpeed(currentUp)}</span>
      </div>
      <div class="strip-spacer"></div>
      <div class="strip-item muted" title="内核运行时长">
        <span class="strip-key">在线</span>
        <span class="strip-val">{uptimeLabel}</span>
      </div>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 flex-shrink-0">
      <CoreStatusCard />

      <div class="overview-card flex flex-col gap-2 overflow-hidden" style="min-height: 96px;">
        <div class="flex items-center justify-between flex-shrink-0">
          <span class="card-label">代理模式</span>
          {#if guiState.proxyMode?.currentMode}
            <span class="mode-indicator">{guiState.proxyMode.currentMode === 'global' ? '全局' : guiState.proxyMode.currentMode === 'rule' ? '规则' : '直连'}</span>
          {/if}
        </div>

        <div class="mt-auto">
          <SegmentedControl.Root
            value={guiState.proxyMode?.currentMode ?? ''}
            onValueChange={(value) => {
              if (value === 'global' || value === 'rule' || value === 'direct') {
                void guiState.setProxyMode(value);
              }
            }}
            disabled={guiState.isSwitchingMode}
            class="proxy-segment"
            aria-label="选择代理模式"
          >
            {#each PROXY_MODES as mode}
              <SegmentedControl.Item value={mode.value} style="flex: 1;">
                {mode.label}
              </SegmentedControl.Item>
            {/each}
          </SegmentedControl.Root>
        </div>
      </div>

      <KernelVersionCard />

      {#if store.isFeatureVisible('tun') || store.isFeatureVisible('systemStack')}
        <TunStackStatus />
      {/if}
    </div>

    <div class="network-strip">
        <span class="card-label network-strip-label">本地网络</span>
        <div class="network-strip-content">
          {#if networkProbeResult}
          {#if networkProbeFlagUrl}
            <img src={networkProbeFlagUrl} alt="" class="network-strip-flag" width="20" height="15" loading="lazy" />
          {/if}
          <span class="network-strip-ip font-mono">{networkProbeResult.ip}</span>
          <span class="network-strip-sep"></span>
          <span class="network-strip-loc" title={formatProbeLocation(networkProbeResult)}>
            {formatProbeLocation(networkProbeResult)}
          </span>
          {#if networkProbeResult.isp || networkProbeResult.org}
            <span class="network-strip-sep"></span>
            <span class="network-strip-isp" title={networkProbeResult.isp || networkProbeResult.org}>
              {networkProbeResult.isp || networkProbeResult.org}
            </span>
          {/if}
          {:else}
            <span class="network-strip-empty" title={networkProbeError ?? undefined}>{networkProbePlaceholder}</span>
          {/if}
        </div>
        <div class="network-strip-actions">
        {#if networkProbeLoading}
          <span class="network-status-badge loading">检测中…</span>
        {/if}
          <button data-slot="surface-button"
            type="button"
            class="network-strip-trigger"
            onclick={() => void guiState.probeNetwork()}
            disabled={networkProbeLoading}
            title={networkProbeLoading ? '检测中' : '重新检测本地网络'}
            aria-label={networkProbeLoading ? '检测中' : '重新检测本地网络'}
          >
            <svg
              class="network-strip-trigger-icon"
              class:spinning={networkProbeLoading}
              width="12"
              height="12"
              viewBox="0 0 12 12"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M10 6A4 4 0 1 1 8.83 3.17" />
              <polyline points="10 2 10 6 6 6" />
            </svg>
            手动测试
          </button>
        </div>
    </div>

    <div class="overview-card flex-shrink-0">
      <button data-slot="surface-button" class="flex items-center justify-between w-full cursor-pointer" onclick={() => testExpanded = !testExpanded} style="background: none; border: none; padding: 0; color: inherit;">
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

    <div class="traffic-panel">
      <div class="w-full h-full overflow-hidden">
        <TrafficChart history={overviewData.speedHistory} unsupported={!guiState.supportsTrafficStats} />
      </div>
    </div>

  </div>

{:else}
  <!-- ============ LITE MODE ============ -->
  <div class="lite-root animate-fade-in">
    <div class="lite-main">
      <div
        class="lite-power-orbit"
        class:on={liteConnected}
        class:idle={!liteConnected}
        class:unsupported={!guiState.supportsTrafficStats}
      >
        <div
          class="lite-session-traffic lite-metric-help lite-metric-help-below"
          data-tooltip={guiState.supportsTrafficStats ? `本次总流量 ${sessionTotalLabel}` : '本次总流量不可用'}
        >
          <span class="sr-only">本次总流量：</span>
          <strong>{guiState.supportsTrafficStats ? sessionTotalLabel : '—'}</strong>
        </div>

        <div
          class="lite-traffic-ring"
          class:flowing={currentUp > 0.001 || currentDown > 0.001}
          style={sessionRingStyle}
          aria-hidden="true"
        ></div>

        <div class="lite-traffic-totals">
          <span
            class="lite-total-up lite-metric-help"
            data-tooltip={guiState.supportsTrafficStats ? `本次上传 ${sessionUpLabel}` : '本次上传不可用'}
          >
            <span class="sr-only">本次上传：</span>
            <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="2 7 6 3 10 7"/></svg>
            <span>{guiState.supportsTrafficStats ? sessionUpLabel : '—'}</span>
          </span>
          <span
            class="lite-total-down lite-metric-help"
            data-tooltip={guiState.supportsTrafficStats ? `本次下载 ${sessionDownLabel}` : '本次下载不可用'}
          >
            <span class="sr-only">本次下载：</span>
            <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="2 5 6 9 10 5"/></svg>
            <span>{guiState.supportsTrafficStats ? sessionDownLabel : '—'}</span>
          </span>
        </div>

        <button data-slot="surface-button"
          class="lite-power"
          class:on={liteConnected}
          class:connecting={isPowerBusy}
          onclick={() => liteConnected ? guiState.disconnect() : guiState.connect()}
          disabled={isPowerBusy}
          aria-label={liteConnected ? '关闭代理' : '开启代理'}
          title={liteConnected ? '关闭代理' : '开启代理'}
        >
          {#if isPowerBusy}
            <span class="lite-power-spin">⟳</span>
          {:else}
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M18.36 6.64a9 9 0 1 1-12.73 0"/>
              <line x1="12" y1="2" x2="12" y2="12"/>
            </svg>
          {/if}
        </button>

        <div class="lite-live-rates">
          <span
            class="lite-live-up lite-metric-help"
            data-tooltip={guiState.supportsTrafficStats ? `实时上传速率 ${formatSpeed(currentUp)}` : '实时上传速率不可用'}
          >
            <span class="sr-only">实时上传速率：</span>
            <svg width="10" height="10" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="2 7 6 3 10 7"/></svg>
            <span>{guiState.supportsTrafficStats ? formatSpeed(currentUp) : '—'}</span>
          </span>
          <span
            class="lite-live-down lite-metric-help"
            data-tooltip={guiState.supportsTrafficStats ? `实时下载速率 ${formatSpeed(currentDown)}` : '实时下载速率不可用'}
          >
            <span class="sr-only">实时下载速率：</span>
            <svg width="10" height="10" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="2 5 6 9 10 5"/></svg>
            <span>{guiState.supportsTrafficStats ? formatSpeed(currentDown) : '—'}</span>
          </span>
        </div>
      </div>
    </div>

    <div class="lite-mode-block">
      <span class="lite-section-label">代理模式</span>
      <SegmentedControl.Root
        value={guiState.proxyMode?.currentMode ?? ''}
        onValueChange={(value) => {
          if (value === 'global' || value === 'rule' || value === 'direct') {
            void guiState.setProxyMode(value);
          }
        }}
        disabled={guiState.isSwitchingMode || !guiState.proxyMode}
        class="lite-proxy-segment"
        aria-label="选择代理模式"
      >
        {#each PROXY_MODES as mode}
          <SegmentedControl.Item value={mode.value} style="flex: 1;">
            {mode.label}
          </SegmentedControl.Item>
        {/each}
      </SegmentedControl.Root>
    </div>

    <div class="lite-entry-list">
      <div class="lite-entry lite-source-entry">
        <span class="lite-entry-icon" aria-hidden="true">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 11a9 9 0 0 1 9 9"/><path d="M4 4a16 16 0 0 1 16 16"/><circle cx="5" cy="19" r="1"/>
          </svg>
        </span>
        <span class="lite-entry-summary">
          <span class="lite-entry-label">配置来源</span>
          <span class="lite-entry-current">{sourceName}</span>
          <span class="lite-entry-meta">{sourceMeta}</span>
        </span>
        <span class="lite-source-controls">
          {#if subscriptions.length > 0}
            <Select.Root
              type="single"
              value={activeSubscription?.id ?? ''}
              items={sourceOptions}
              disabled={sourceLoading || activatingSourceId !== null}
              onValueChange={(value) => {
                if (typeof value === 'string' && value) void activateSource(value);
              }}
            >
              <Select.Trigger class="lite-source-select" aria-label="切换配置来源">
                <Select.Value />
              </Select.Trigger>
              <Select.Content>
                {#each subscriptions as subscription}
                  <Select.Item
                    value={subscription.id}
                    label={subscription.name}
                    disabled={!subscription.enabled}
                  >
                    {subscription.name}{subscription.id === activeSubscription?.id ? ' · 当前' : ''}
                  </Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          {/if}
          <button data-slot="surface-button"
            type="button"
            class="lite-manage-source"
            onclick={() => (store.activeTab = 'subscriptions')}
          >
            {subscriptions.length > 0 ? '管理' : '添加'}
          </button>
        </span>
      </div>

      {#if hasConfig || hasNodes}
        <button data-slot="surface-button"
          type="button"
          class="lite-entry"
          onclick={() => (store.activeTab = 'nodes')}
          aria-label="打开节点页面"
        >
          <span class="lite-entry-icon" aria-hidden="true">
            <svg width="13" height="13" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="5" cy="5" r="3"/><line x1="5" y1="0" x2="5" y2="1.2"/><line x1="5" y1="8.8" x2="5" y2="10"/><line x1="0" y1="5" x2="1.2" y2="5"/><line x1="8.8" y1="5" x2="10" y2="5"/>
            </svg>
          </span>
          <span class="lite-entry-summary">
            <span class="lite-entry-label">当前节点</span>
            <span class="lite-entry-current lite-node-current">
              {#if activeNodeFlagCode}
                <span
                  class="lite-node-country-flag fi fi-{activeNodeFlagCode.toLowerCase()}"
                  role="img"
                  title="国旗 {activeNodeFlagCode}"
                  aria-label="国旗 {activeNodeFlagCode}"
                ></span>
              {:else if activeNodeEmoji}
                <span class="lite-node-emoji">{activeNodeEmoji}</span>
              {/if}
              <span class="lite-node-name-text">{activeNodeName ?? '暂无节点'}</span>
            </span>
            <span class="lite-entry-meta">{activeNodeMeta}</span>
          </span>
          <span class="lite-entry-action">节点</span>
          <svg class="lite-entry-chevron" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><polyline points="4.5 3 7.5 6 4.5 9"/></svg>
        </button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .status-strip {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    overflow-x: auto;
    scrollbar-width: none;
  }

  .status-strip::-webkit-scrollbar { display: none; }

  .strip-item {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .strip-item.up { color: #22C55E; }
  .strip-item.down { color: #3B82F6; }
  .strip-item.muted { color: var(--muted-foreground); }

  :global(.dark) .strip-item.up { color: #4ADE80; }
  :global(.dark) .strip-item.down { color: #60A5FA; }

  .strip-item.tone-on .strip-val { color: #16A34A; }
  .strip-item.tone-listen .strip-val { color: #D97706; }
  .strip-item.tone-error .strip-val { color: var(--destructive); }
  .strip-item.tone-off .strip-val { color: var(--muted-foreground); }

  :global(.dark) .strip-item.tone-on .strip-val { color: #4ADE80; }
  :global(.dark) .strip-item.tone-listen .strip-val { color: #FBBF24; }

  .strip-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--muted-foreground);
    opacity: 0.5;
    transition: background 0.2s ease, opacity 0.2s ease;
  }

  .strip-item.tone-on .strip-dot { background: #22C55E; opacity: 1; }
  .strip-item.tone-listen .strip-dot { background: #F59E0B; opacity: 1; }
  .strip-item.tone-error .strip-dot { background: #EF4444; opacity: 1; }

  .strip-dot.pulse { animation: pulse-dot 1.4s ease-in-out infinite; }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  .strip-key {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--muted-foreground);
    opacity: 0.7;
    letter-spacing: 0.01em;
  }

  .strip-val {
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-mono, monospace);
    font-variant-numeric: tabular-nums;
    color: var(--foreground);
  }

  .strip-sep {
    display: block;
    width: 1px;
    height: 13px;
    background: var(--border);
    border-radius: 1px;
    flex-shrink: 0;
  }

  .strip-spacer { flex: 1; min-width: 8px; }

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

  :global(.dark) .overview-card { box-shadow: 0 1px 3px rgba(0, 0, 0, 0.22); }
  :global(.dark) .overview-card:hover { box-shadow: 0 2px 8px rgba(0, 0, 0, 0.32); }

  .card-label { font-size: 12px; font-weight: 500; color: var(--muted-foreground); letter-spacing: 0.01em; }

  .mode-indicator { font-size: 11px; font-weight: 600; color: var(--muted-foreground); font-variant-numeric: tabular-nums; }

  :global(.proxy-segment) { width: 100%; }

  .expand-chevron { transition: transform 0.2s ease; opacity: 0.5; flex-shrink: 0; }
  .expand-chevron.expanded { transform: rotate(180deg); }

  .test-checks { margin-top: 10px; padding-top: 10px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 6px; }
  .test-check-row { display: flex; align-items: flex-start; gap: 6px; font-size: 11.5px; }
  .test-check-info { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .test-check-name { font-weight: 600; color: var(--foreground); }
  .test-check-msg { color: var(--muted-foreground); font-size: 11px; line-height: 1.4; word-break: break-all; }

  .traffic-panel {
    width: 100%;
    min-height: 240px;
    flex: 1 0 240px;
    overflow: hidden;
  }

  .network-strip {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 2fr) minmax(0, 1fr);
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 12px;
    overflow: hidden;
    flex-shrink: 0;
  }
  .network-strip-label,
  .network-strip-ip,
  .network-strip-sep {
    flex-shrink: 0;
  }
  .network-strip-label {
    justify-self: start;
    white-space: nowrap;
  }
  .network-strip-flag {
    width: 20px;
    height: 15px;
    border-radius: 2px;
    object-fit: cover;
    flex-shrink: 0;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }
  .network-strip-content {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 100%;
    min-width: 0;
    overflow: hidden;
  }
  .network-strip-sep {
    width: 1px;
    height: 12px;
    background: var(--border);
  }
  .network-strip-ip {
    font-weight: 600;
    color: var(--foreground);
  }
  .network-strip-loc,
  .network-strip-isp {
    color: var(--muted-foreground);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .network-strip-isp { opacity: 0.8; }
  .network-strip-empty {
    color: var(--muted-foreground);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .network-strip-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    min-width: 0;
  }
  .network-strip-trigger {
    appearance: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 24px;
    width: 24px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--background);
    color: var(--foreground);
    font-size: 0;
    font-weight: 600;
    line-height: 0;
    cursor: pointer;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease, box-shadow 0.12s ease;
  }
  .network-strip-trigger-icon { flex-shrink: 0; }
  .network-strip-trigger-icon.spinning { animation: network-trigger-spin 0.8s linear infinite; }
  .network-strip-trigger:hover:not(:disabled) {
    background: var(--muted);
    border-color: rgba(0, 0, 0, 0.18);
  }
  :global(.dark) .network-strip-trigger:hover:not(:disabled) { border-color: rgba(255, 255, 255, 0.16); }
  .network-strip-trigger:focus-visible {
    outline: none;
    border-color: var(--ring);
    box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.12);
  }
  .network-strip-trigger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    box-shadow: none;
  }
  @keyframes network-trigger-spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
  .network-status-badge {
    flex-shrink: 0;
    height: 18px;
    padding: 0 6px;
    border-radius: 4px;
    font-size: 10px;
    color: var(--muted-foreground);
    background: var(--muted);
  }
  .network-status-badge.loading { animation: network-pulse 1.5s ease-in-out infinite; }
  @keyframes network-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  @property --traffic-up-share {
    syntax: '<percentage>';
    inherits: false;
    initial-value: 50%;
  }

  .lite-root {
    width: 100%;
    max-width: 720px;
    margin: 0 auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
    overflow-x: hidden;
    min-height: 0;
    padding: 2px 0 8px;
  }

  .lite-main {
    min-height: 210px;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    flex-shrink: 0;
    padding-top: 2px;
  }

  .lite-power-orbit {
    position: relative;
    width: 240px;
    height: 184px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    overflow: visible;
  }

  .lite-session-traffic {
    position: absolute;
    top: 0;
    left: 50%;
    z-index: 4;
    min-width: 88px;
    height: 18px;
    padding: 0 7px;
    display: flex;
    align-items: center;
    justify-content: center;
    transform: translateX(-50%);
    border-radius: 9px;
    background: var(--background);
    color: var(--foreground);
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    line-height: 1;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.01em;
    white-space: nowrap;
    transition: opacity 0.2s ease, color 0.2s ease;
  }

  .lite-session-traffic strong {
    font-weight: 700;
  }

  .lite-power-orbit.idle .lite-session-traffic {
    color: var(--muted-foreground);
    opacity: 0.72;
  }

  .lite-power-orbit.unsupported .lite-session-traffic {
    opacity: 0.45;
  }

  .lite-traffic-ring {
    --traffic-up: #22C55E;
    --traffic-down: #3B82F6;
    --traffic-up-share: 50%;
    position: absolute;
    top: 20px;
    left: 50%;
    z-index: 1;
    width: 164px;
    height: 164px;
    transform: translateX(-50%);
    border-radius: 50%;
    background: conic-gradient(
      from 180deg,
      var(--traffic-up) 0 var(--traffic-up-share),
      var(--traffic-down) var(--traffic-up-share) 100%
    );
    pointer-events: none;
    opacity: 0.92;
    transition: --traffic-up-share 0.38s ease, opacity 0.2s ease, filter 0.2s ease;
  }

  .lite-traffic-ring::after {
    content: '';
    position: absolute;
    inset: 5px;
    border-radius: 50%;
    background: var(--background);
  }

  :global(.dark) .lite-traffic-ring {
    --traffic-up: #4ADE80;
    --traffic-down: #60A5FA;
  }

  .lite-power-orbit.idle .lite-traffic-ring {
    opacity: 0.38;
  }

  .lite-power-orbit.on .lite-traffic-ring {
    opacity: 0.92;
  }

  .lite-power-orbit.unsupported .lite-traffic-ring {
    background: var(--border);
    opacity: 0.42;
  }

  .lite-traffic-ring.flowing {
    animation: lite-ring-flow 1.15s ease-in-out infinite;
  }

  @keyframes lite-ring-flow {
    0%, 100% { filter: drop-shadow(0 0 0 rgba(59, 130, 246, 0)); }
    50% { filter: drop-shadow(0 0 2.5px rgba(59, 130, 246, 0.24)); }
  }

  .lite-power {
    position: relative;
    z-index: 3;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 96px;
    height: 96px;
    margin-top: 18px;
    border-radius: 50%;
    border: 1.5px solid var(--border);
    background: color-mix(in srgb, var(--muted) 82%, var(--card));
    color: var(--muted-foreground);
    cursor: pointer;
    box-shadow: 0 5px 18px rgba(0, 0, 0, 0.055), inset 0 0 0 1px color-mix(in srgb, var(--background) 55%, transparent);
    transition: transform 0.18s ease, border-color 0.2s ease, background 0.2s ease, color 0.2s ease, box-shadow 0.2s ease;
    flex-shrink: 0;
  }

  .lite-power:hover:not(:disabled) {
    border-color: rgba(34, 197, 94, 0.46);
    color: #16A34A;
    box-shadow: 0 7px 22px rgba(0, 0, 0, 0.07), 0 0 18px rgba(34, 197, 94, 0.09);
  }

  .lite-power:active:not(:disabled) { transform: scale(0.96); }
  .lite-power:disabled { opacity: 0.48; cursor: not-allowed; }

  .lite-power.on {
    border-color: rgba(34, 197, 94, 0.42);
    background: rgba(34, 197, 94, 0.075);
    color: #16A34A;
    box-shadow: 0 6px 22px rgba(34, 197, 94, 0.08), inset 0 0 0 1px rgba(34, 197, 94, 0.05);
  }

  .lite-power.on:hover:not(:disabled) {
    border-color: rgba(239, 68, 68, 0.45);
    color: var(--destructive, #EF4444);
    box-shadow: 0 6px 22px rgba(239, 68, 68, 0.08);
  }

  :global(.dark) .lite-power {
    box-shadow: 0 6px 22px rgba(0, 0, 0, 0.24), inset 0 0 0 1px rgba(255, 255, 255, 0.025);
  }
  :global(.dark) .lite-power.on { color: #4ADE80; border-color: rgba(74,222,128,0.38); }
  :global(.dark) .lite-power.on:hover:not(:disabled) { color: #F87171; border-color: rgba(248,113,113,0.42); }

  .lite-power.connecting {
    border-color: rgba(245, 158, 11, 0.48);
    color: #F59E0B;
  }

  .lite-power-spin { font-size: 23px; animation: spin 0.8s linear infinite; }

  .lite-traffic-totals {
    position: absolute;
    inset: 0;
    z-index: 2;
    pointer-events: none;
  }

  .lite-total-up,
  .lite-total-down {
    position: absolute;
    top: 82px;
    width: 76px;
    min-width: 0;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 9.5px;
    font-weight: 650;
    font-family: var(--font-mono, monospace);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    opacity: 0.84;
    transition: opacity 0.2s ease, color 0.2s ease;
  }

  .lite-total-up {
    left: 0;
    justify-content: flex-end;
    color: #22C55E;
    text-align: right;
  }

  .lite-total-down {
    right: 0;
    justify-content: flex-start;
    color: #3B82F6;
    text-align: left;
  }

  :global(.dark) .lite-total-up { color: #4ADE80; }
  :global(.dark) .lite-total-down { color: #60A5FA; }

  .lite-power-orbit.idle .lite-total-up,
  .lite-power-orbit.idle .lite-total-down {
    color: var(--muted-foreground);
    opacity: 0.54;
  }

  .lite-power-orbit.unsupported .lite-total-up,
  .lite-power-orbit.unsupported .lite-total-down {
    opacity: 0.38;
  }

  .lite-live-rates {
    position: absolute;
    top: 188px;
    left: 50%;
    width: 174px;
    transform: translateX(-50%);
    z-index: 4;
    display: grid;
    grid-template-columns: 1fr 1fr;
    align-items: center;
    column-gap: 14px;
    pointer-events: none;
    font-family: var(--font-mono, monospace);
    font-variant-numeric: tabular-nums;
    font-size: 9.5px;
    font-weight: 600;
    color: var(--muted-foreground);
  }

  .lite-live-up,
  .lite-live-down {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
    opacity: 0.72;
  }

  .lite-live-up { justify-content: flex-end; }
  .lite-live-down { justify-content: flex-start; }

  .lite-power-orbit.on .lite-live-up {
    color: #22C55E;
    opacity: 0.86;
  }

  .lite-power-orbit.on .lite-live-down {
    color: #3B82F6;
    opacity: 0.86;
  }

  :global(.dark) .lite-power-orbit.on .lite-live-up { color: #4ADE80; }
  :global(.dark) .lite-power-orbit.on .lite-live-down { color: #60A5FA; }

  .lite-power-orbit.unsupported .lite-live-up,
  .lite-power-orbit.unsupported .lite-live-down {
    opacity: 0.38;
  }

  .lite-metric-help {
    pointer-events: auto;
  }

  .lite-metric-help::after {
    content: attr(data-tooltip);
    position: absolute;
    left: 50%;
    bottom: calc(100% + 7px);
    z-index: 20;
    max-width: 190px;
    padding: 5px 7px;
    transform: translate(-50%, 2px);
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--popover);
    color: var(--popover-foreground);
    box-shadow: 0 5px 16px rgba(0, 0, 0, 0.12);
    font-family: var(--font-sans);
    font-size: 10.5px;
    font-weight: 500;
    line-height: 1.25;
    letter-spacing: 0;
    white-space: nowrap;
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition: opacity 0.12s ease, transform 0.12s ease, visibility 0.12s ease;
  }

  .lite-metric-help-below::after {
    top: calc(100% + 7px);
    bottom: auto;
    transform: translate(-50%, -2px);
  }

  .lite-metric-help:hover::after {
    opacity: 1;
    visibility: visible;
    transform: translate(-50%, 0);
  }

  .lite-mode-block {
    display: flex;
    flex-direction: column;
    gap: 5px;
    flex-shrink: 0;
  }
  .lite-section-label {
    padding: 0 4px;
    font-size: 10px;
    font-weight: 600;
    color: var(--muted-foreground);
  }
  :global(.lite-proxy-segment) { width: 100%; }

  .lite-entry-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex-shrink: 0;
  }
  .lite-entry {
    width: 100%;
    min-height: 58px;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 9px 12px;
    border-radius: 9px;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--foreground);
    text-align: left;
    transition: border-color 0.13s ease, box-shadow 0.13s ease, background 0.13s ease;
  }
  button.lite-entry { cursor: pointer; }
  button.lite-entry:hover {
    border-color: var(--ring, rgba(99,102,241,0.3));
    background: color-mix(in srgb, var(--card) 94%, var(--muted));
  }
  button.lite-entry:focus-visible {
    outline: none;
    border-color: var(--ring);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--ring) 16%, transparent);
  }
  .lite-entry-icon {
    width: 30px;
    height: 30px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 7px;
    background: var(--muted);
    color: var(--muted-foreground);
    flex-shrink: 0;
  }
  .lite-entry-summary {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .lite-entry-label {
    font-size: 9.5px;
    color: var(--muted-foreground);
  }
  .lite-entry-current {
    font-size: 12.5px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lite-node-current {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }
  .lite-node-country-flag {
    display: inline-block;
    width: 16px;
    height: 12px;
    border-radius: 2px;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--border) 82%, transparent);
    overflow: hidden;
    flex-shrink: 0;
  }
  .lite-node-emoji {
    flex-shrink: 0;
    font-family: "Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", sans-serif;
  }
  .lite-node-name-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lite-entry-meta {
    font-size: 9.5px;
    color: var(--muted-foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lite-entry-action {
    font-size: 10.5px;
    color: var(--muted-foreground);
    white-space: nowrap;
  }
  .lite-entry-chevron {
    color: var(--muted-foreground);
    opacity: 0.55;
    flex-shrink: 0;
  }
  .lite-source-controls {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  :global(.lite-source-select) {
    width: 138px;
    height: 28px;
    font-size: 11px;
  }
  .lite-manage-source {
    height: 28px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--background);
    color: var(--muted-foreground);
    font-size: 10.5px;
    cursor: pointer;
  }
  .lite-manage-source:hover {
    background: var(--muted);
    color: var(--foreground);
  }

  @media (max-width: 620px) {
    .lite-source-entry {
      align-items: flex-start;
      flex-wrap: wrap;
    }
    .lite-source-controls {
      width: 100%;
      padding-left: 39px;
    }
    :global(.lite-source-select) {
      flex: 1;
      width: auto;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .lite-traffic-ring.flowing,
    .lite-power-spin {
      animation: none;
    }
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
