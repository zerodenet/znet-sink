<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import TrafficChart from '$lib/components/TrafficChart.svelte';
  import CoreStatusCard from '$lib/components/core/CoreStatusCard.svelte';
  import KernelVersionCard from '$lib/components/core/KernelVersionCard.svelte';
  import TunStackStatus from '$lib/components/core/TunStackStatus.svelte';
  import LogPanel from '$lib/components/core/LogPanel.svelte';
  import UpdateBanner from '$lib/components/UpdateBanner.svelte';
  import {
    selectPolicy,
  } from '$lib/services/core';

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

  let testExpanded = $state(false);

  const PROXY_MODES = [
    { value: 'global', label: '全局' },
    { value: 'rule',   label: '规则' },
    { value: 'direct', label: '直连' },
  ] as const;

  // ── Lite mode state ──
  let nodeDropdownOpen = $state(false);
  let nodeSwitching = $state<string | null>(null);

  // Network probe from guiState (auto-triggered)
  const networkProbeResult = $derived(guiState.networkProbe);
  const networkProbeLoading = $derived(guiState.networkProbeLoading);

  // Country name to code mapping (supports multiple formats)
  const COUNTRY_NAME_MAP: Record<string, string> = {
    // Chinese names
    '中国': 'CN', '美国': 'US', '日本': 'JP', '韩国': 'KR', '新加坡': 'SG',
    '香港': 'HK', '台湾': 'TW', '澳门': 'MO', '英国': 'GB', '德国': 'DE',
    '法国': 'FR', '加拿大': 'CA', '澳大利亚': 'AU', '俄罗斯': 'RU', '印度': 'IN',
    '巴西': 'BR', '荷兰': 'NL', '瑞典': 'SE', '瑞士': 'CH', '芬兰': 'FI',
    '挪威': 'NO', '丹麦': 'DK', '波兰': 'PL', '捷克': 'CZ', '奥地利': 'AT',
    '比利时': 'BE', '意大利': 'IT', '西班牙': 'ES', '葡萄牙': 'PT', '爱尔兰': 'IE',
    '新西兰': 'NZ', '墨西哥': 'MX', '阿根廷': 'AR', '智利': 'CL', '南非': 'ZA',
    '泰国': 'TH', '越南': 'VN', '马来西亚': 'MY', '印度尼西亚': 'ID', '菲律宾': 'PH',
    '阿联酋': 'AE', '沙特阿拉伯': 'SA', '以色列': 'IL', '土耳其': 'TR', '乌克兰': 'UA',
    '哈萨克斯坦': 'KZ', '蒙古': 'MN', '老挝': 'LA', '柬埔寨': 'KH',
    // English names (lowercase for case-insensitive matching)
    'china': 'CN', 'united states': 'US', 'usa': 'US', 'japan': 'JP', 'korea': 'KR',
    'south korea': 'KR', 'singapore': 'SG', 'hong kong': 'HK', 'taiwan': 'TW',
    'united kingdom': 'GB', 'uk': 'GB', 'germany': 'DE', 'france': 'FR',
    'canada': 'CA', 'australia': 'AU', 'russia': 'RU', 'india': 'IN',
    'brazil': 'BR', 'netherlands': 'NL', 'sweden': 'SE', 'switzerland': 'CH',
    'finland': 'FI', 'norway': 'NO', 'denmark': 'DK', 'poland': 'PL',
    'czech republic': 'CZ', 'czechia': 'CZ', 'austria': 'AT', 'belgium': 'BE',
    'italy': 'IT', 'spain': 'ES', 'portugal': 'PT', 'ireland': 'IE',
    'new zealand': 'NZ', 'mexico': 'MX', 'argentina': 'AR', 'chile': 'CL',
    'south africa': 'ZA', 'thailand': 'TH', 'vietnam': 'VN', 'malaysia': 'MY',
    'indonesia': 'ID', 'philippines': 'PH', 'united arab emirates': 'AE',
    'saudi arabia': 'SA', 'israel': 'IL', 'turkey': 'TR', 'ukraine': 'UA',
    'kazakhstan': 'KZ', 'mongolia': 'MN', 'laos': 'LA', 'cambodia': 'KH',
    'myanmar': 'MM', 'burma': 'MM', 'nepal': 'NP', 'bangladesh': 'BD',
    'sri lanka': 'LK', 'pakistan': 'PK', 'iran': 'IR', 'iraq': 'IQ',
    'syria': 'SY', 'jordan': 'JO', 'lebanon': 'LB', 'kuwait': 'KW',
    'qatar': 'QA', 'bahrain': 'BH', 'oman': 'OM', 'yemen': 'YE',
    'macao': 'MO', 'macau': 'MO',
  };

  // Convert country name/code to 2-letter code
  function getCountryCode(country?: string): string | undefined {
    if (!country) return undefined;
    const trimmed = country.trim();
    // Already a 2-letter code
    if (trimmed.length === 2) return trimmed.toUpperCase();
    // Try direct lookup (case-insensitive)
    const lower = trimmed.toLowerCase();
    return COUNTRY_NAME_MAP[lower] ?? COUNTRY_NAME_MAP[trimmed];
  }

  // Get flag image URL for a country code
  function getFlagUrl(country?: string): string | null {
    const code = getCountryCode(country);
    if (!code) return null;
    return `https://flagcdn.com/w40/${code.toLowerCase()}.png`;
  }

  // Format location with flag
  function formatLocationWithFlag(result: { country?: string; region?: string; city?: string }): { flagUrl: string | null; text: string } {
    const flagUrl = getFlagUrl(result.country);
    const parts = [result.country, result.region, result.city].filter(Boolean);
    return {
      flagUrl,
      text: parts.length > 0 ? parts.join(' · ') : '未知地区',
    };
  }

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

  const isConnected = $derived(guiState.isConnected);
  const proxyEnabled = $derived(guiState.isSystemProxyEnabled);
  const isCoreRunning = $derived(guiState.isProcessRunning);
  const isPowerBusy = $derived(guiState.isConnecting || guiState.isDisconnecting);
  const hasConfig = $derived(guiState.configNodes.length > 0 || guiState.proxyMode != null);
  const hasNodes = $derived(guiState.policyGroups.length > 0 || guiState.configNodes.length > 0);

  const powerLabel = $derived(
    guiState.isConnecting ? '启用中' :
    guiState.isDisconnecting ? '关闭中' :
    proxyEnabled ? '服务中' :
    isCoreRunning ? '开启系统代理' :
    '开启服务'
  );

  // Current node for display — config nodes as primary source,
  // runtime data (selected, delay, alive) from policy groups when connected.
  const activeNodeName = $derived.by(() => {
    for (const g of guiState.policyGroups) {
      if (g.selected) return g.selected;
    }
    const cn = guiState.configNodes;
    return cn.find((n) => !n.isSelector)?.tag ?? cn[0]?.tag ?? null;
  });

  // Flat node list for dropdown — config nodes as base, runtime data when connected
  const dropdownGroups = $derived.by(() => {
    const groups = guiState.policyGroups;
    if (groups.length > 0) {
      return groups.map((g) => ({
        name: g.name,
        selected: g.selected,
        nodes: g.outbounds.map((o) => ({ tag: o.tag, type: o.type, delayMs: o.delayMs, alive: o.alive })),
      }));
    }
    // Fall back to config nodes (no core required)
    const selectorNodes = guiState.configNodes.filter((n) => n.isSelector);
    const regularNodes = guiState.configNodes.filter((n) => !n.isSelector);
    if (selectorNodes.length > 0) {
      return selectorNodes.map((s) => ({
        name: s.tag,
        selected: null as string | null,
        nodes: regularNodes.map((n) => ({ tag: n.tag, type: n.protocol })),
      }));
    }
    if (regularNodes.length > 0) {
      return [{
        name: '节点',
        selected: null as string | null,
        nodes: regularNodes.map((n) => ({ tag: n.tag, type: n.protocol })),
      }];
    }
    return [];
  });

  // Close dropdown on outside click
  let dropdownRef: HTMLDivElement | undefined = $state();

  function closeDropdown(e: MouseEvent) {
    if (dropdownRef && !dropdownRef.contains(e.target as Node)) {
      nodeDropdownOpen = false;
    }
  }

  $effect(() => {
    if (nodeDropdownOpen) {
      document.addEventListener('click', closeDropdown, true);
    } else {
      document.removeEventListener('click', closeDropdown, true);
    }
    return () => document.removeEventListener('click', closeDropdown, true);
  });

  async function handleNodeSelect(groupName: string, tag: string) {
    if (nodeSwitching) return;
    if (!isCoreRunning) return;
    nodeSwitching = tag;
    try {
      await selectPolicy(groupName, tag);
    } catch { /* non-blocking */ }
    nodeSwitching = null;
    nodeDropdownOpen = false;
  }

  // ── Pro status-strip derived values ──
  const modeLabel = $derived(
    guiState.proxyMode?.currentMode === 'global' ? '全局' :
    guiState.proxyMode?.currentMode === 'direct' ? '直连' :
    guiState.proxyMode?.currentMode === 'rule' ? '规则' : '—',
  );
  const isCoreAvailable = $derived(
    guiState.connection?.coreAvailable === true || guiState.connection?.processState === 'running',
  );
  const coreStateLabel = $derived(
    proxyEnabled ? '服务中' :
    isCoreRunning ? '监听中' :
    guiState.isStartingCore ? '启动中' :
    guiState.connection?.processState === 'failed' ? '失败' : '已停止',
  );
  const coreStateTone = $derived(
    proxyEnabled ? 'on' :
    isCoreAvailable || guiState.isStartingCore ? 'listen' :
    guiState.connection?.processState === 'failed' ? 'error' : 'off',
  );
  const uptimeLabel = $derived(formatUptime(guiState.connection?.uptimeMs));
</script>

<UpdateBanner />

{#if store.uiMode === 'pro'}
  <!-- ============ PRO MODE ============ -->
  <div class="flex-1 w-full flex flex-col gap-3 overflow-y-auto overflow-x-hidden animate-fade-in min-h-0 pr-0.5">

    <!-- Row 0: Compact status strip — all key state at a glance -->
    <div class="status-strip flex-shrink-0" role="status" aria-label="运行状态概览">
      <div class="strip-item tone-{coreStateTone}" title="内核状态">
        <span class="strip-dot" class:pulse={guiState.isStartingCore || guiState.isConnecting}></span>
        <span class="strip-key">内核</span>
        <span class="strip-val">{coreStateLabel}</span>
      </div>
      <span class="strip-sep" aria-hidden="true"></span>
      <div class="strip-item {proxyEnabled ? 'tone-on' : 'tone-off'}" title="系统代理">
        <span class="strip-key">代理</span>
        <span class="strip-val">{proxyEnabled ? '已开启' : '未开启'}</span>
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

    <!-- Row 1: Status cards -->
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 flex-shrink-0">
      <CoreStatusCard />

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

      <!-- Kernel version -->
      <KernelVersionCard />

      <!-- TUN / Stack status (v0.0.5+) -->
      {#if store.isFeatureVisible('tun') || store.isFeatureVisible('systemStack')}
        <TunStackStatus />
      {/if}
    </div>

    <!-- Network probe: compact horizontal strip (visible on small screens, hidden on lg+) -->
    {#if store.isFeatureVisible('policySelection') && (hasConfig || hasNodes) && networkProbeResult}
      {@const loc = formatLocationWithFlag(networkProbeResult)}
      <div class="network-strip lg:hidden">
        <span class="card-label network-strip-label">网络检测</span>
        <div class="network-strip-content">
          {#if loc.flagUrl}
            <img src={loc.flagUrl} alt="" class="network-strip-flag" width="20" height="15" loading="lazy" />
          {/if}
          <span class="network-strip-ip font-mono">{networkProbeResult.ip}</span>
          <span class="network-strip-sep"></span>
          <span class="network-strip-loc">{loc.text}</span>
          {#if networkProbeResult.isp || networkProbeResult.org}
            <span class="network-strip-sep"></span>
            <span class="network-strip-isp truncate">{networkProbeResult.isp || networkProbeResult.org}</span>
          {/if}
        </div>
        {#if networkProbeLoading}
          <span class="network-status-badge loading">检测中…</span>
        {/if}
      </div>
    {/if}

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

    <!-- Row 3: Chart + Network probe card (card only visible on lg+) -->
    <div class="flex-1 w-full flex flex-col lg:flex-row gap-3 overflow-hidden min-h-0" style="min-height: 180px;">
      <div class="w-full {store.isFeatureVisible('policySelection') && (hasConfig || hasNodes) ? 'lg:w-2/3' : ''} overflow-hidden min-h-[120px]">
        <TrafficChart history={overviewData.speedHistory} unsupported={!guiState.supportsTrafficStats} />
      </div>
      {#if store.isFeatureVisible('policySelection') && (hasConfig || hasNodes)}
      <div class="hidden lg:block lg:w-1/3 min-w-0">
        <div class="overview-card network-card">
          <div class="network-card-header">
            <span class="card-label">网络检测</span>
            {#if networkProbeLoading}
              <span class="network-status-badge loading">检测中…</span>
            {/if}
          </div>
          {#if networkProbeResult}
            {@const loc = formatLocationWithFlag(networkProbeResult)}
            <div class="network-card-body">
              <div class="network-main">
                {#if loc.flagUrl}
                  <img src={loc.flagUrl} alt="" class="network-flag" width="36" height="27" loading="lazy" />
                {/if}
                <span class="network-ip font-mono">{networkProbeResult.ip}</span>
              </div>
              <div class="network-location">{loc.text}</div>
              {#if networkProbeResult.isp || networkProbeResult.org}
                <div class="network-isp truncate">{networkProbeResult.isp || networkProbeResult.org}</div>
              {/if}
            </div>
          {:else if !isCoreRunning}
            <div class="network-card-empty">内核未运行</div>
          {:else}
            <div class="network-card-empty">等待网络检测…</div>
          {/if}
        </div>
      </div>
      {/if}
    </div>

    <!-- Row 5: Log panel -->
    {#if store.isNavVisible('logs')}
      <div style="min-height: 120px; max-height: 200px;" class="flex-shrink-0 flex-1 min-h-0">
        <LogPanel />
      </div>
    {/if}

  </div>

{:else}
  <!-- ============ LITE MODE ============ -->
  <div class="lite-root animate-fade-in">

    <!-- Node selector: only show when config is active or nodes are loaded -->
    {#if hasConfig || hasNodes}
    <div class="lite-node-wrap" bind:this={dropdownRef}>
      <button class="lite-node-trigger" onclick={() => nodeDropdownOpen = !nodeDropdownOpen}>
        <svg width="13" height="13" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" class="lite-node-icon">
          <circle cx="5" cy="5" r="3"/><line x1="5" y1="0" x2="5" y2="1.2"/><line x1="5" y1="8.8" x2="5" y2="10"/><line x1="0" y1="5" x2="1.2" y2="5"/><line x1="8.8" y1="5" x2="10" y2="5"/>
        </svg>
        <span class="lite-node-current">
          {activeNodeName ?? '暂无节点'}
        </span>
        <svg class="lite-chevron" class:open={nodeDropdownOpen} width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><polyline points="3 4.5 6 7.5 9 4.5"/></svg>
      </button>

      {#if nodeDropdownOpen}
        <div class="lite-node-dropdown">
          {#each dropdownGroups as group}
            <div class="lite-ngroup">
              <div class="lite-ngroup-label">{group.name}</div>
              {#each group.nodes as node}
                <button
                  class="lite-nitem {group.selected === node.tag ? 'active' : ''}"
                  onclick={() => handleNodeSelect(group.name, node.tag)}
                  disabled={nodeSwitching !== null || !isCoreRunning}
                >
                  <span class="lite-nitem-dot {group.selected === node.tag ? 'on' : ''}"></span>
                  <span class="lite-nitem-name">{node.tag}</span>
                  <span class="lite-nitem-type">{node.type}</span>
                  {#if nodeSwitching === node.tag}
                    <span class="lite-nitem-spin">⟳</span>
                  {/if}
                </button>
              {/each}
            </div>
          {/each}
          {#if dropdownGroups.length === 0 || (dropdownGroups.length === 1 && dropdownGroups[0].nodes.length === 0)}
            <div class="lite-node-empty">暂无节点数据</div>
          {/if}
          <button class="lite-node-manage" onclick={() => { nodeDropdownOpen = false; store.activeTab = 'nodes'; }}>
            管理节点 →
          </button>
        </div>
      {/if}
    </div>
    {/if}

    <!-- Main row: power button centered as the sole focus -->
    <div class="lite-main">

      <!-- Center: big power button -->
      <button
        class="lite-power"
        class:on={proxyEnabled}
        class:connecting={isPowerBusy}
        onclick={() => isConnected ? guiState.disconnect() : guiState.connect()}
        disabled={isPowerBusy || (!isConnected && !guiState.canConnect)}
        aria-label={isConnected ? '关闭服务' : isCoreRunning ? '开启系统代理' : '开启服务'}
      >
        {#if isPowerBusy}
          <span class="lite-power-spin">⟳</span>
        {:else}
          <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18.36 6.64a9 9 0 1 1-12.73 0"/>
            <line x1="12" y1="2" x2="12" y2="12"/>
          </svg>
        {/if}
        <span class="lite-power-label">{powerLabel}</span>
      </button>
    </div>

    <!-- Inline real-time speed (replaces the old stats column + speed bar) -->
    <div class="lite-speed-inline">
      <span class="lite-speed-down">
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="2 5 6 9 10 5"/></svg>
        {formatSpeed(currentDown)}
      </span>
      <span class="lite-speed-up">
        <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="2 7 6 3 10 7"/></svg>
        {formatSpeed(currentUp)}
      </span>
    </div>

    <!-- Traffic chart -->
    <div class="lite-chart">
      <TrafficChart history={overviewData.speedHistory} unsupported={!guiState.supportsTrafficStats} />
    </div>

  </div>
{/if}

<style>
  /* ─────────────── Shared (Pro) ─────────────── */

  /* Compact status strip — one-row at-a-glance overview */
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
    width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
    background: var(--muted-foreground); opacity: 0.5;
    transition: background 0.2s ease, opacity 0.2s ease;
  }
  .strip-item.tone-on .strip-dot { background: #22C55E; opacity: 1; }
  .strip-item.tone-listen .strip-dot { background: #F59E0B; opacity: 1; }
  .strip-item.tone-error .strip-dot { background: #EF4444; opacity: 1; }
  .strip-dot.pulse { animation: pulse-dot 1.4s ease-in-out infinite; }
  @keyframes pulse-dot { 0%, 100% { opacity: 1; } 50% { opacity: 0.3; } }

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

  .proxy-segment { display: flex; align-items: center; gap: 1px; background: var(--segment-bg, rgba(0,0,0,0.055)); padding: 2px; border-radius: 7px; width: 100%; }
  .proxy-seg-btn {
    flex: 1; display: inline-flex; align-items: center; justify-content: center;
    height: 24px; border-radius: 5px; border: none; background: transparent;
    color: var(--muted-foreground); font-size: 11.5px; font-weight: 500;
    cursor: pointer; transition: all 0.13s ease; white-space: nowrap;
  }
  .proxy-seg-btn:hover:not(:disabled) { color: var(--foreground); }
  .proxy-seg-btn.active { background: var(--segment-active-bg, #fff); box-shadow: var(--segment-active-shadow, 0 1px 3px rgba(0,0,0,0.12)); color: var(--foreground); font-weight: 600; }
  .proxy-seg-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .expand-chevron { transition: transform 0.2s ease; opacity: 0.5; flex-shrink: 0; }
  .expand-chevron.expanded { transform: rotate(180deg); }

  .test-checks { margin-top: 10px; padding-top: 10px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 6px; }
  .test-check-row { display: flex; align-items: flex-start; gap: 6px; font-size: 11.5px; }
  .test-check-info { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .test-check-name { font-weight: 600; color: var(--foreground); }
  .test-check-msg { color: var(--muted-foreground); font-size: 11px; line-height: 1.4; word-break: break-all; }

  .node-link-btn {
    display: inline-flex; align-items: center; height: 22px; padding: 0 9px;
    border-radius: 5px; border: 1px solid var(--border); background: transparent;
    color: var(--muted-foreground); font-size: 11px; font-weight: 500;
    cursor: pointer; transition: background 0.12s ease, color 0.12s ease;
  }
  .node-link-btn:hover { background: var(--muted); color: var(--foreground); }
  .node-link-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .active-node-name { font-size: 14px; font-weight: 700; color: var(--foreground); }
  .active-node-meta { font-size: 11.5px; color: var(--muted-foreground); font-family: var(--font-mono); }

  .network-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 100px;
    overflow: hidden;
  }
  .network-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }
  .network-card-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 0;
    overflow: hidden;
    text-align: center;
  }
  .network-main {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .network-flag {
    width: 36px;
    height: 27px;
    border-radius: 3px;
    object-fit: cover;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }
  .network-ip {
    font-size: 18px;
    font-weight: 700;
    color: var(--foreground);
    letter-spacing: 0.02em;
  }
  .network-location {
    font-size: 12px;
    color: var(--muted-foreground);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .network-isp {
    font-size: 11px;
    color: var(--muted-foreground);
    opacity: 0.8;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .network-card-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    color: var(--muted-foreground);
    text-align: center;
  }

  /* Network strip: compact horizontal layout for small screens */
  .network-strip {
    display: flex;
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
  .network-strip-label {
    flex-shrink: 0;
  }
  .network-strip-content {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
  }
  .network-strip-sep {
    width: 1px;
    height: 12px;
    background: var(--border);
    flex-shrink: 0;
  }
  .network-strip-flag {
    width: 20px;
    height: 15px;
    border-radius: 2px;
    object-fit: cover;
    flex-shrink: 0;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }
  .network-strip-ip {
    font-weight: 600;
    color: var(--foreground);
    flex-shrink: 0;
  }
  .network-strip-loc {
    color: var(--muted-foreground);
    flex-shrink: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .network-strip-isp {
    color: var(--muted-foreground);
    opacity: 0.8;
    flex-shrink: 1;
    min-width: 0;
    max-width: 120px;
  }

  .network-status-badge {
    display: inline-flex;
    align-items: center;
    height: 18px;
    padding: 0 6px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 500;
    color: var(--muted-foreground);
    background: var(--muted);
  }
  .network-status-badge.loading {
    animation: pulse 1.5s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  /* ─────────────── Lite mode ─────────────── */

  .lite-root {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
    overflow-x: hidden;
    min-height: 0;
  }

  /* ---- Node selector ---- */
  .lite-node-wrap { position: relative; flex-shrink: 0; }

  .lite-node-trigger {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px;
    border-radius: 9px;
    border: 1px solid var(--border);
    background: var(--card);
    color: var(--foreground);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: border-color 0.13s ease, box-shadow 0.13s ease;
  }
  .lite-node-trigger:hover { border-color: var(--ring, rgba(99,102,241,0.3)); }

  .lite-node-icon { color: var(--muted-foreground); flex-shrink: 0; }
  .lite-node-current { flex: 1; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .lite-chevron { flex-shrink: 0; transition: transform 0.2s ease; opacity: 0.5; }
  .lite-chevron.open { transform: rotate(180deg); }

  .lite-node-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    max-height: 280px;
    overflow-y: auto;
    border-radius: 9px;
    border: 1px solid var(--border);
    background: var(--popover, var(--card));
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
    z-index: 40;
    padding: 4px 0;
  }
  :global(.dark) .lite-node-dropdown { box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4); }

  .lite-ngroup { padding: 2px 0; }
  .lite-ngroup:not(:last-child) { border-bottom: 1px solid var(--border); }

  .lite-ngroup-label {
    padding: 6px 14px 2px;
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    opacity: 0.6;
  }

  .lite-nitem {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 14px;
    border: none;
    background: transparent;
    color: var(--foreground);
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s ease;
  }
  .lite-nitem:hover { background: var(--muted); }
  .lite-nitem.active { background: rgba(99,102,241,0.06); }
  .lite-nitem:disabled { opacity: 0.5; cursor: not-allowed; }

  .lite-nitem-dot {
    width: 7px; height: 7px; border-radius: 50%;
    background: var(--muted-foreground); opacity: 0.2; flex-shrink: 0;
    transition: all 0.15s ease;
  }
  .lite-nitem-dot.on { background: #22C55E; opacity: 1; }

  .lite-nitem-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .lite-nitem-type {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.03em;
    opacity: 0.6;
  }

  .lite-nitem-spin {
    font-size: 13px; color: var(--muted-foreground);
    animation: spin 0.8s linear infinite; flex-shrink: 0;
  }

  .lite-node-empty {
    padding: 20px 14px;
    text-align: center;
    font-size: 12px;
    color: var(--muted-foreground);
    opacity: 0.5;
  }

  .lite-node-manage {
    display: block;
    width: 100%;
    padding: 8px 14px;
    border: none;
    background: transparent;
    color: var(--primary);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    text-align: center;
    border-top: 1px solid var(--border);
    transition: background 0.1s ease;
  }
  .lite-node-manage:hover { background: var(--muted); }

  /* ---- Main row ---- */
  .lite-main {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    flex-shrink: 0;
    padding: 4px 0;
  }

  /* Big power button */
  .lite-power {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 88px;
    height: 88px;
    border-radius: 50%;
    border: 2.5px solid var(--border);
    background: var(--muted);
    color: var(--muted-foreground);
    cursor: pointer;
    transition: all 0.2s ease;
    flex-shrink: 0;
  }
  .lite-power:hover:not(:disabled) {
    border-color: rgba(34, 197, 94, 0.5);
    color: #16A34A;
    box-shadow: 0 0 20px rgba(34, 197, 94, 0.12);
  }
  .lite-power:active:not(:disabled) { transform: scale(0.96); }
  .lite-power:disabled { opacity: 0.4; cursor: not-allowed; }

  .lite-power.on {
    border-color: rgba(34, 197, 94, 0.5);
    background: rgba(34, 197, 94, 0.08);
    color: #16A34A;
  }
  .lite-power.on:hover:not(:disabled) {
    border-color: rgba(239, 68, 68, 0.5);
    color: var(--destructive, #EF4444);
    box-shadow: 0 0 20px rgba(239, 68, 68, 0.1);
  }
  :global(.dark) .lite-power.on { color: #4ADE80; border-color: rgba(74,222,128,0.4); }
  :global(.dark) .lite-power.on:hover:not(:disabled) { color: #EF4444; border-color: rgba(239,68,68,0.4); }

  .lite-power.connecting {
    border-color: rgba(245, 158, 11, 0.5);
    color: #F59E0B;
  }

  .lite-power-spin { font-size: 22px; animation: spin 0.8s linear infinite; }

  .lite-power-label {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }

  /* ---- Inline speed ---- */
  .lite-speed-inline {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 18px;
    flex-shrink: 0;
  }

  .lite-speed-down,
  .lite-speed-up {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12.5px;
    font-weight: 700;
    font-family: var(--font-mono, monospace);
    font-variant-numeric: tabular-nums;
    color: var(--foreground);
  }
  .lite-speed-down { color: #3B82F6; }
  .lite-speed-up { color: #22C55E; }
  :global(.dark) .lite-speed-down { color: #60A5FA; }
  :global(.dark) .lite-speed-up { color: #4ADE80; }

  /* ---- Chart ---- */
  .lite-chart {
    flex: 1;
    min-height: 100px;
    overflow: hidden;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
