<script lang="ts">
  import { createProbeJobs } from 'virtual:znet-node-probes';
  import { NodeScreenState } from '$lib/features/nodes/screen.svelte';
  import { Button } from '$lib/components/ui/button';
  import { onDestroy, onMount } from 'svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { store } from '$lib/services/store.svelte';
  import { appendLog, getNodeScreenSnapshot, guiSelectPolicy } from '$lib/services/core';
  import { listen } from '@tauri-apps/api/event';
  import { getGroupKindStyle, parseNodeName } from '$lib/services/node-utils';
  import type { ProxyNode } from '$lib/types/protocol';
  import type { NodeScreenSnapshot, PolicyGroup, ProbeJobSnapshot } from '$lib/types/gui-api';
  import NodesDelayPopover from '$lib/components/tabs/NodesDelayPopover.svelte';
  import NodesGridCard from '$lib/components/tabs/NodesGridCard.svelte';
  import NodesGroupSidebar from '$lib/components/tabs/NodesGroupSidebar.svelte';
  import NodesListRow from '$lib/components/tabs/NodesListRow.svelte';
  import NodesToolbar from '$lib/components/tabs/NodesToolbar.svelte';
  import { nodeInventory, globalTargetReason } from './nodes-inventory';
  import { isUrlTestGroup } from '$lib/components/tabs/nodes-display-preferences.svelte';
  import { error as toastError } from '$lib/services/toast.svelte';
  import {
    collectProbingPolicyNodeTags,
    filterNodes,
    getActiveNodeTag,
    planProbeTargets,
    summarizeProbeProgress,
  } from '$lib/components/tabs/nodes-view-model';
  import {
    mergeActiveProbeJobs,
  } from '$lib/components/tabs/nodes-probe-state';

  // View state
  type ViewMode = 'list' | 'grid';
  const VIEW_MODE_KEY = 'znet-nodes-view-mode';
  let viewMode = $state<ViewMode>('grid');
  let hideTimer: ReturnType<typeof setTimeout> | null = null;
  let searchQuery = $state('');
  let selectedGroup = $state<string | null>(null);
  let nodeView = $state<'groups' | 'all' | 'wireguard'>('groups');
  const viewingWireguard = $derived(nodeView === 'wireguard');

  function loadViewMode(): ViewMode {
    try {
      return localStorage.getItem(VIEW_MODE_KEY) === 'list' ? 'list' : 'grid';
    } catch {
      return 'grid';
    }
  }

  function setViewMode(mode: ViewMode) {
    viewMode = mode;
    try {
      localStorage.setItem(VIEW_MODE_KEY, mode);
    } catch {
      // View preference persistence is best effort.
    }
  }

  const probeJobs = createProbeJobs?.(getNodeScreenSnapshot);
  const screen = probeJobs ?? new NodeScreenState(getNodeScreenSnapshot);

  // Action state
  let switching = $state<string | null>(null);
  let lastError = $state<string | null>(null);
  type DelayEntry = { delay: number; at: number; selectedTag?: string };

  function reportActionError(message: string) {
    lastError = message;
    toastError(message, 8_000);
  }

  interface ProbeFailureLog {
    message: string;
    scope: 'single' | 'batch' | 'policy';
    targetTag?: string;
    policyTag?: string;
    failedTargets?: string[];
    outcome?: 'failed' | 'timeout';
  }

  function recordProbeFailure(failure: ProbeFailureLog) {
    const target = failure.targetTag ?? failure.policyTag;
    const timedOut = failure.outcome === 'timeout';
    const message = target
      ? `${timedOut ? '节点测速超时' : '节点测速失败'}（${target}）：${failure.message}`
      : `${timedOut ? '节点测速超时' : '节点测速失败'}：${failure.message}`;
    void appendLog({
      source: 'app',
      level: timedOut ? 'info' : 'warn',
      message,
      fields: {
        schema: 'znet.node-probe.v1',
        area: 'nodes',
        operation: 'probe',
        scope: failure.scope,
        targetTag: failure.targetTag,
        policyTag: failure.policyTag,
        failedTargets: failure.failedTargets,
        outcome: failure.outcome ?? 'failed',
      },
    }).catch((logError) => {
      console.error('[nodes] failed to persist probe failure', logError);
    });
  }

  // Kernel connection state
  const isCoreAvailable = $derived(
    screen.nodeScreen?.sourceStatus === 'ready' || screen.nodeScreen?.sourceStatus === 'degraded',
  );
  const probeDisabledReason = $derived(
    !isCoreAvailable ? '内核未就绪，无法测速' : null,
  );
  function refreshNodeScreen(reason: string) { return screen.refresh(reason); }
  function applyProbeJob(job: ProbeJobSnapshot) { probeJobs?.apply(job); }

  function probeScope(job: ProbeJobSnapshot): 'single' | 'batch' | 'policy' {
    if (job.kind === 'manual_policy') return 'policy';
    return job.targetTags.length > 1 ? 'batch' : 'single';
  }

  function handleProbeJobUpdate(job: ProbeJobSnapshot) {
    if (!probeJobs || probeJobs.disposed) return;
    applyProbeJob(job);
    if (job.state === 'running') return;

    // Node observations/history are refreshed only after the authoritative job
    // enters a terminal state. The spinner is driven by the job snapshot itself.
    void refreshNodeScreen('probe_terminal');
    if (probeJobs.reportedProbeJobs.has(job.id)) return;
    probeJobs.reportedProbeJobs.add(job.id);
    if (job.state === 'failed' || job.state === 'partially_failed' || job.state === 'timed_out') {
      recordProbeFailure({
        message: job.state === 'timed_out'
          ? `probe job ${job.id} timed out with ${job.completed}/${job.targetTags.length} completed`
          : `${job.failed}/${job.targetTags.length} targets failed`,
        scope: probeScope(job),
        policyTag: job.kind === 'manual_policy' && job.targetTags.length === 1 ? job.targetTags[0] : undefined,
        targetTag: job.kind === 'outbound' && job.targetTags.length === 1 ? job.targetTags[0] : undefined,
        failedTargets: job.results.filter((result) => !result.reachable).map((result) => result.targetTag),
        outcome: job.state === 'timed_out' ? 'timeout' : 'failed',
      });
    }
  }

  onMount(() => {
    viewMode = loadViewMode();
    void refreshNodeScreen('mount');
    if (probeJobs) void probeJobs.attach(listen<ProbeJobSnapshot>('client-core:probe-job-updated', (event) => {
      handleProbeJobUpdate(event.payload);
    }));
    void screen.attach(listen('client-core:updated', () => {
      void refreshNodeScreen('client_core_updated');
    }));
  });

  onDestroy(() => {
    if (hideTimer) clearTimeout(hideTimer);
    screen.dispose();
  });

  // Presentation adapters over the single authoritative Rust snapshot.
  const groups = $derived.by<PolicyGroup[]>(() =>
    (screen.nodeScreen?.groups ?? []).map((group) => ({
      name: group.tag,
      kind: group.kind,
      selected: group.selected,
      available: group.available,
      reason: group.reason,
      outbounds: group.memberTags.map((tag) => {
        const node = screen.nodeScreen?.nodes.find((candidate) => candidate.tag === tag);
        return {
          tag,
          type: node?.protocol ?? 'unknown',
          delayMs: node?.latencyMs,
          alive: node?.alive,
          lastCheckedUnixMs: node?.lastObservedAtUnixMs,
        };
      }),
    })),
  );

  const canSortByDelay = $derived.by(() => {
    if (!selectedGroup) return groups.some((group) => isUrlTestGroup(group));
    return isUrlTestGroup(groups.find((group) => group.name === selectedGroup));
  });

  const allNodes = $derived.by<ProxyNode[]>(() => {
    return (screen.nodeScreen?.nodes ?? []).map((node) => {
      const parsed = parseNodeName(node.tag);
      return {
        id: `${node.id.profileId}:${node.id.configRevision}:${node.id.tag}`,
        tag: node.tag,
        name: node.tag,
        emoji: parsed.emoji,
        flagCode: parsed.flagCode,
        cleanName: parsed.cleanName,
        protocol: node.protocol !== 'unknown' ? node.protocol : 'proxy',
        delay: node.latencyMs ?? 0,
        lastProbeAt: node.lastObservedAtUnixMs,
        domain: node.groupTags[0] ?? 'default',
        server: node.server,
        port: node.port,
        localAddresses: node.localAddresses,
        udp: node.udp,
        network: node.network,
        tls: node.tls,
        sni: node.sni,
        cipher: node.cipher,
        selected: node.selectedIn.length > 0,
        alive: node.alive,
      };
    });
  });

  const activeProbeJobs = $derived.by(() => mergeActiveProbeJobs(
    screen.nodeScreen?.activeProbeJobs ?? [],
    probeJobs?.directProbeJobs ?? new Map(),
    probeJobs?.terminalProbeJobIds ?? new Set(),
  ));
  const probingNodeTags = $derived.by(() => new Set(
    activeProbeJobs
      .filter((job) => job.kind === 'outbound')
      .flatMap((job) => job.targetTags),
  ));
  const probingPolicyTags = $derived.by(() => new Set(
    activeProbeJobs
      .filter((job) => job.kind === 'manual_policy')
      .flatMap((job) => job.targetTags),
  ));
  const probingNodeIds = $derived.by(() => new Set(
    inventory.filter((node) => probingNodeTags.has(node.tag)).map((node) => node.id),
  ));
  // Only an actual multi-target outbound job is a page-wide batch probe.
  // A single outbound or policy probe must not disable unrelated node actions.
  const probingAll = $derived(activeProbeJobs.some(
    (job) => job.kind === 'outbound' && job.targetTags.length > 1,
  ));
  const probingRequested = $derived(activeProbeJobs.length > 0);
  const probeProgress = $derived.by(() =>
    summarizeProbeProgress(groups, activeProbeJobs),
  );

  const inventory = $derived(nodeInventory(allNodes, groups));
  const isGlobalMode = $derived(guiState.proxyMode?.currentMode === 'global');
  const leafInventory = $derived(inventory.filter(node => !groups.some(group => group.name === node.tag)));
  const filteredNodes = $derived.by(() => {
    return filterNodes({
      allNodes: selectedGroup || isGlobalMode ? inventory : leafInventory,
      groups,
      query: searchQuery.trim().toLowerCase(),
      selectedGroup,
    }).filter((node) => !viewingWireguard || node.protocol.toLowerCase() === 'wireguard');
  });
  const wireguardCount = $derived(allNodes.filter((node) => node.protocol.toLowerCase() === 'wireguard').length);

  const viewingGlobal = $derived(isGlobalMode && nodeView === 'all');
  const activeNodeId = $derived(viewingGlobal ? guiState.proxyMode?.globalOutbound ?? null
    : selectedGroup ? getActiveNodeTag(groups, selectedGroup) : null);
  function selectionHint(node: ProxyNode): string | undefined {
    if (viewingGlobal) return globalTargetReason(node.tag, allNodes, groups) ?? '设为全局出口';
    if (nodeView === 'all') return '查看和测速；请选择左侧策略组后切换节点';
    return undefined;
  }
  const selectionOperable = $derived(viewingGlobal
    ? !guiState.isSwitchingMode && !guiState.isSelectingPolicy
    : store.isActionOperable('policies.select'));

  const plannedProbeTargets = $derived.by(() =>
    planProbeTargets({ groups, selectedGroup, visibleNodes: filteredNodes }),
  );

  const probingPolicyNodeTags = $derived.by(() =>
    collectProbingPolicyNodeTags(groups, probingPolicyTags),
  );

  function isNodeProbing(node: ProxyNode): boolean {
    return probingNodeIds.has(node.id) || probingPolicyNodeTags.has(node.tag);
  }

  // Actions
  /** Check if a node is a direct member of a selector group.
   *  A nested group (e.g. urltest) inside a selector is selectable —
   *  policies.select sends the direct member tag, and the kernel resolves
   *  it recursively during engine resolve. */
  function isNodeSelectable(node: ProxyNode): boolean {
    if (viewingWireguard) return false;
    // If user is browsing a specific group, check if that group is a selector
    if (selectedGroup) {
      const browsingGroup = groups.find((g) => g.name === selectedGroup);
      if (browsingGroup && browsingGroup.kind?.toLowerCase() === 'selector') {
        // Node is a direct member of this selector → selectable
        return browsingGroup.outbounds.some((o) => o.tag === node.tag);
      }
      // Browsing a non-selector group → not selectable
      return false;
    }

    // Inventory browsing never guesses a policy group to mutate.
    return viewingGlobal && globalTargetReason(node.tag, allNodes, groups) === null;
  }

  async function handleSelect(node: ProxyNode) {
    if (switching || !selectionOperable) return;
    if (!isCoreAvailable) {
      reportActionError('内核未就绪，无法切换节点');
      return;
    }
    if (!isNodeSelectable(node)) {
      reportActionError(selectionHint(node) ?? '当前策略组不支持手动切换节点');
      return;
    }
    switching = node.id;
    lastError = null;
    try {
      if (viewingGlobal) {
        const result = await guiState.setProxyMode('global', { globalOutbound: node.tag, notify: false });
        if (!result.ok) reportActionError(result.message ?? '全局出口尚未确认生效');
      } else {
        // Only the explicitly opened selector receives a membership change.
        const policyTag = selectedGroup!;
        const result = await guiSelectPolicy(policyTag, node.tag);
        if (!result.accepted) reportActionError(result.message ?? '内核未接受此选择');
      }
      await refreshNodeScreen('policy_select');
    } catch (e) {
      reportActionError((e as { message?: string }).message ?? '切换节点失败');
    } finally {
      switching = null;
    }
  }

  function isUrlTestPolicyNode(node: ProxyNode): boolean {
    const protocol = (node.protocol ?? '').toLowerCase().replaceAll('-', '_');
    return protocol === 'url_test' || protocol === 'urltest';
  }

  async function handleProbe(node: ProxyNode) {
    if (!probeJobs) return;
    const policyProbe = isUrlTestPolicyNode(node);
    if (!isCoreAvailable) {
      recordProbeFailure({
        message: '内核未就绪',
        scope: policyProbe ? 'policy' : 'single',
        policyTag: policyProbe ? node.tag : undefined,
        targetTag: policyProbe ? undefined : node.tag,
      });
      return;
    }

    try {
      const job = await probeJobs.start({
        kind: policyProbe ? 'manual_policy' : 'outbound',
        targetTags: [node.tag],
        timeoutMs: 30_000,
      });
      applyProbeJob(job);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      recordProbeFailure({
        message,
        scope: policyProbe ? 'policy' : 'single',
        policyTag: policyProbe ? node.tag : undefined,
        targetTag: policyProbe ? undefined : node.tag,
      });
      reportActionError(message);
    }
  }

  let stoppingProbes = $state(false);
  async function handleStopProbes() {
    if (!probeJobs || stoppingProbes) return;
    stoppingProbes = true;
    try {
      const results = await Promise.allSettled(activeProbeJobs.map(async job => {
        handleProbeJobUpdate(await probeJobs.cancel(job.id));
      }));
      const failed = results.find(result => result.status === 'rejected');
      if (failed?.status === 'rejected') reportActionError(failed.reason);
    } finally { stoppingProbes = false; }
  }

  async function handleProbeAll() {
    if (!probeJobs) return;
    if (!isCoreAvailable) {
      recordProbeFailure({ message: '内核未就绪', scope: 'batch' });
      return;
    }
    if (probingRequested || probingAll || probingNodeIds.size > 0 || probingPolicyTags.size > 0) {
      return;
    }
    const targets = plannedProbeTargets;
    if (targets.nodes.length === 0) return;

    lastError = null;
    try {
      const waves = Math.max(1, Math.ceil(targets.nodes.length / 8));
      const job = await probeJobs.start({
        kind: 'outbound',
        targetTags: targets.nodes.map((node) => node.tag),
        timeoutMs: Math.min(300_000, Math.max(30_000, 15_000 + waves * 15_000)),
      });
      applyProbeJob(job);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      recordProbeFailure({
        message,
        scope: 'batch',
        outcome: 'failed',
      });
      reportActionError(message);
    }
  }

  let previousMode: string | undefined;
  $effect(() => {
    const mode = guiState.proxyMode?.currentMode;
    if (mode !== previousMode) {
      const wasGlobal = previousMode === 'global';
      previousMode = mode;
      if (mode === 'global' && (nodeView === 'groups' || nodeView === 'all')) {
        selectedGroup = null; nodeView = 'all';
      } else if (wasGlobal && nodeView === 'all') {
        nodeView = 'groups'; selectedGroup = groups[0]?.name ?? null;
      }
    }
    if (nodeView !== 'groups' || mode === 'global' || groups.length === 0) return;
    if (!groups.some(group => group.name === selectedGroup)) selectedGroup = groups[0].name;
  });

  // Render the popover in document.body. Merely placing it after .nodes-root
  // is not enough: the tab transition viewport has overflow:hidden and a
  // transformed containing block, which clips even position:fixed children.
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      },
    };
  }

  interface PopoverState {
    visible: boolean;
    anchor: HTMLElement | null;
    node: ProxyNode | null;
  }
  type PopoverPlacement = 'above' | 'below';
  const POPOVER_MAX_INTERACTIVE_HEIGHT = 236;
  let popover = $state<PopoverState>({ visible: false, anchor: null, node: null });
  let popoverElement = $state<HTMLDivElement | null>(null);
  let popoverPositionVersion = $state(0);
  let popoverPlacement = $state<PopoverPlacement | null>(null);
  function historyForNode(tag: string): DelayEntry[] {
    return (screen.nodeScreen?.nodes.find((node) => node.tag === tag)?.history ?? []).flatMap((entry) => {
      if (entry.latencyMs != null) {
        return [{
          delay: entry.latencyMs,
          at: entry.observedAtUnixMs,
          selectedTag: entry.selectedTag,
        }];
      }
      if (entry.reachable) {
        // A reachable observation without a measured latency is incomplete,
        // not a real 0 ms result. Skip it rather than fabricating a datapoint.
        return [];
      }
      return [{
        delay: -1,
        at: entry.observedAtUnixMs,
        selectedTag: entry.selectedTag,
      }];
    });
  }
  const popoverHistory = $derived.by<DelayEntry[]>(() =>
    popover.node ? historyForNode(popover.node.tag) : [],
  );

  function choosePopoverPlacement(anchor: HTMLElement): PopoverPlacement {
    const r = anchor.getBoundingClientRect();
    const gap = 6;
    const edgePadding = 8;
    const viewportHeight = window.innerHeight;
    const spaceAbove = r.top - gap - edgePadding;
    const spaceBelow = viewportHeight - r.bottom - gap - edgePadding;

    // Choose against the largest interactive view, not the initial chart.
    // The placement is then locked for this hover session so chart/list
    // resizing cannot flip the popover across the anchor under the pointer.
    if (spaceAbove >= POPOVER_MAX_INTERACTIVE_HEIGHT) return 'above';
    if (spaceBelow >= POPOVER_MAX_INTERACTIVE_HEIGHT) return 'below';
    return spaceAbove >= spaceBelow ? 'above' : 'below';
  }

  function showPopover(e: MouseEvent, node: ProxyNode) {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    const hist = historyForNode(node.tag);
    if (hist.length === 0) return;
    const anchor = e.currentTarget as HTMLElement;
    popoverPlacement = choosePopoverPlacement(anchor);
    popover = { visible: true, anchor, node };
  }

  function hidePopover(delay = 300) {
    if (hideTimer) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      popover = { visible: false, anchor: null, node: null };
      popoverPlacement = null;
      hideTimer = null;
    }, delay);
  }

  function keepPopover() {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
  }

  $effect(() => {
    if (!popover.visible || typeof window === 'undefined') return;

    const refreshPosition = () => {
      popoverPositionVersion += 1;
    };
    const resizeObserver = typeof ResizeObserver === 'undefined'
      ? null
      : new ResizeObserver(refreshPosition);
    if (popoverElement) resizeObserver?.observe(popoverElement);

    window.addEventListener('resize', refreshPosition);
    window.addEventListener('scroll', refreshPosition, true);
    return () => {
      resizeObserver?.disconnect();
      window.removeEventListener('resize', refreshPosition);
      window.removeEventListener('scroll', refreshPosition, true);
    };
  });

  function popoverStyle(): string {
    void popoverPositionVersion;
    if (!popover.anchor) return '';
    const r = popover.anchor.getBoundingClientRect();
    const gap = 6;
    const edgePadding = 8;
    const popoverHeight = popoverElement?.offsetHeight ?? 112;
    const popoverWidth = popoverElement?.offsetWidth ?? 220;
    const viewportHeight = typeof window === 'undefined' ? 800 : window.innerHeight;
    const viewportWidth = typeof window === 'undefined' ? 1200 : window.innerWidth;
    const left = Math.max(
      edgePadding,
      Math.min(viewportWidth - popoverWidth - edgePadding, r.left + (r.width - popoverWidth) / 2),
    );
    const placement = popoverPlacement ?? (r.top >= viewportHeight - r.bottom ? 'above' : 'below');
    const preferredTop = placement === 'above' ? r.top - gap - popoverHeight : r.bottom + gap;
    const top = Math.max(
      edgePadding,
      Math.min(viewportHeight - popoverHeight - edgePadding, preferredTop),
    );

    return `position:fixed; left:${Math.round(left)}px; top:${Math.round(top)}px; z-index:9999;`;
  }
</script>

<div class="nodes-root animate-fade-in">
  <NodesGroupSidebar
    {groups}
    allNodesCount={isGlobalMode ? inventory.length : leafInventory.length}
    allNodesLabel={isGlobalMode ? '全局出口' : '全部节点'}
    {selectedGroup}
    {wireguardCount}
    {viewingWireguard}
    onSelectWireguard={() => { selectedGroup = null; nodeView = 'wireguard'; }}
    onSelectGroup={(groupName) => { selectedGroup = groupName; nodeView = groupName === null ? 'all' : 'groups'; }}
  />

  <!-- Right: Node panel -->
  <div class="node-panel">
    <NodesToolbar
      selectedGroup={viewingWireguard ? 'WireGuard 端点' : viewingGlobal ? '全局出口' : selectedGroup}
      filteredCount={filteredNodes.length}
      isCoreAvailable={isCoreAvailable}
      {searchQuery}
      {viewMode}
      probing={probingRequested}
      {probeProgress}
      {canSortByDelay}
      canProbeAll={isCoreAvailable && !probingRequested && !probingAll && probingNodeIds.size === 0 && probingPolicyTags.size === 0 && plannedProbeTargets.nodes.length > 0}
      {probeDisabledReason}
      onSearchQueryChange={(value) => (searchQuery = value)}
      onViewModeChange={setViewMode}
      onProbeAll={probeJobs ? handleProbeAll : undefined}
      onStopProbes={probeJobs && activeProbeJobs.length ? handleStopProbes : undefined}
      {stoppingProbes}
    />
    {#if lastError}<p role="alert" class="node-action-error">{lastError}</p>{/if}
    {#if viewingGlobal}
      <p class="endpoint-note">当前全局出口：{guiState.proxyMode?.globalOutbound ?? '未确认'}。选择节点或非 selector 组作为统一出口；手动选择组在左侧管理，循环引用的组不可选。</p>
    {:else if nodeView === 'all'}
      <p class="endpoint-note">全部出站节点的去重列表，节点组在左侧独立展示。此处用于查看和测速；切换节点请先选择左侧策略组。</p>
    {/if}
    {#if viewingWireguard}
      <p class="endpoint-note">端点来自当前配置，可独立用于路由和 DNS。节点选择仍由策略组成员决定；此视图不会切换策略。</p>
    {/if}

    <!-- Node content -->
    {#if filteredNodes.length === 0}
      <div class="node-empty">
        <div class="empty-icon">
          <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round">
            <circle cx="12" cy="12" r="10"/>
            <line x1="12" y1="8" x2="12" y2="12"/>
            <line x1="12" y1="16" x2="12.01" y2="16"/>
          </svg>
        </div>
        {#if searchQuery}
          <span class="empty-text">无匹配节点</span>
          <Button variant="link" size="sm"  onclick={() => (searchQuery = '')}>清除搜索</Button>
        {:else if allNodes.length === 0}
          <span class="empty-text">暂无节点数据</span>
          <span class="empty-hint">
            {#if !isCoreAvailable}
              内核未连接，且当前没有生效的代理配置。请先在“配置”页导入并启用一份配置。
            {:else}
              当前配置不包含节点。请在“配置”页导入一份包含 outbounds 的代理配置。
            {/if}
          </span>
          <Button variant="link" size="sm"  onclick={() => (store.activeTab = 'profiles')}>前往代理配置页</Button>
        {:else}
          <span class="empty-text">暂无节点数据</span>
        {/if}
      </div>
    {:else}
      <!-- A flat inventory or the direct members of an explicit group. -->
      {#if viewMode === 'list'}
        <div class="node-list node-list-scroll">
          {#each filteredNodes as node (node.id)}
            <NodesListRow
              {node}
              isActive={activeNodeId === node.tag}
              isSwitching={switching === node.id}
              isProbing={isNodeProbing(node)}
              probingAll={probingAll}
              probeDisabled={!isCoreAvailable}
              selectDisabled={!isCoreAvailable || switching !== null || !selectionOperable || !isNodeSelectable(node)}
              selectionHint={selectionHint(node)}
              readOnly={nodeView === 'all' && !viewingGlobal}
              onSelectNode={handleSelect}
              onProbeNode={probeJobs ? handleProbe : undefined}
              onShowPopover={showPopover}
              onHidePopover={hidePopover}
            />
          {/each}
        </div>
      {:else}
        <div class="node-grid">
          {#each filteredNodes as node (node.id)}
            <NodesGridCard
              {node}
              isActive={activeNodeId === node.tag}
              isSwitching={switching === node.id}
              isProbing={isNodeProbing(node)}
              probingAll={probingAll}
              probeDisabled={!isCoreAvailable}
              selectDisabled={!isCoreAvailable || switching !== null || !selectionOperable || !isNodeSelectable(node)}
              selectionHint={selectionHint(node)}
              readOnly={nodeView === 'all' && !viewingGlobal}
              onSelectNode={handleSelect}
              onProbeNode={probeJobs ? handleProbe : undefined}
              onShowPopover={showPopover}
              onHidePopover={hidePopover}
            />
          {/each}
        </div>
      {/if}
    {/if}

  </div>
</div>

{#if popover.visible && popover.node}
  <div
    bind:this={popoverElement}
    use:portal
    class="popover-anchor"
    style={popoverStyle()}
    onmouseenter={keepPopover}
    onmouseleave={() => hidePopover()}
    role="tooltip"
  >
    <NodesDelayPopover
      node={popover.node}
      hist={popoverHistory}
    />
  </div>
{/if}

<style>
  /* Root layout */
  .nodes-root {
    flex: 1;
    display: flex;
    gap: 0;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    min-height: 0;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
  }

  /* Node panel */
  .node-panel {
    container-type: inline-size;
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    position: relative;
  }
  .endpoint-note {
    padding: 8px 12px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--muted-foreground);
    border-bottom: 1px solid var(--border);
  }

  /* Empty state */
  .node-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    opacity: 0.5;
    padding: 24px;
  }

  .empty-icon {
    color: var(--muted-foreground);
    opacity: 0.4;
  }

  .empty-text {
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .empty-hint {
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--muted-foreground);
    opacity: 0.7;
    max-width: 280px;
    text-align: center;
  }

  .node-action-error { color:var(--destructive); padding:8px 12px; font-size:12px; }

  /* List view */
  .node-list {
    padding: 4px 6px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .node-list-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  /* Grid view */
  .node-grid {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 10px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
    gap: 10px;
    align-content: start;
  }

  /* Error bar */
  /* Popover anchor */
  .popover-anchor {
    position: fixed;
    z-index: 9999;
    pointer-events: auto;
  }

  /* Responsive layout */
  @media (max-width: 700px) {
    .node-grid {
      grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    }
  }

</style>
