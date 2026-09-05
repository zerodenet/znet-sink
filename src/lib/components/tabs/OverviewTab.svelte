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
  import ProfessionalOverview from '$lib/components/overview/ProfessionalOverview.svelte';
  import { buildOverview, type Destination } from '$lib/components/overview/model';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import * as Select from '$lib/components/ui/select';

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

  let refreshingOverview = $state(false);
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
    if (store.uiMode !== 'pro') return;
    uptimeNowMs = Date.now();
    const timer = window.setInterval(() => { uptimeNowMs = Date.now(); }, 1000);
    return () => window.clearInterval(timer);
  });
  const professionalModel = $derived(buildOverview({
    now: uptimeNowMs, connection: guiState.connection,
    connectionAt: guiState.connectionUpdatedAt, connectionError: guiState.connectionError,
    core: guiState.coreOverview, tun: guiState.tunStatus, tunError: guiState.tunStatusError,
    selfTest: guiState.selfTest, selfTestAt: guiState.selfTestUpdatedAt,
    mode: guiState.proxyMode, groups: guiState.policyGroups,
  }));
  const trafficUnavailable = $derived(!guiState.supportsTrafficStats ? '内核不支持流量查询'
    : !professionalModel.ready ? '内核未就绪，暂停展示实时速率'
    : !overviewData.lastSampleAtUnixMs ? '等待第一份流量采样'
    : uptimeNowMs - overviewData.lastSampleAtUnixMs > 10_000 ? '流量采样已过期，等待恢复'
    : !overviewData.isLive ? '正在建立流量采样基线' : null);
  function navigateOverview(target: Destination) {
    if (target === 'nodes' || target === 'profiles' || target === 'connections') store.activeTab = target;
    else store.openSettings(target);
  }
  async function refreshOverview() {
    if (refreshingOverview) return;
    refreshingOverview = true;
    try { await guiState.refreshAll(); } finally { refreshingOverview = false; }
  }

</script>

{#if store.uiMode === 'pro'}
  <ProfessionalOverview model={professionalModel} busy={guiState.isStartingCore || guiState.isSwitchingMode || guiState.isStoppingCore}
    refreshing={refreshingOverview} navigate={navigateOverview} refresh={() => void refreshOverview()}
    start={() => void guiState.startCore()} restart={() => void guiState.restartCore()} setMode={(mode) => void guiState.setProxyMode(mode)}>
    {#snippet traffic()}
      <TrafficChart history={overviewData.speedHistory} unsupported={!guiState.supportsTrafficStats} unavailableReason={trafficUnavailable} />
    {/snippet}
  </ProfessionalOverview>

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
