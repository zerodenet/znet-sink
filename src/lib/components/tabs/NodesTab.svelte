<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { coreEvents, PolicyProbeTimeoutError } from '$lib/services/core-events.svelte';
  import { store } from '$lib/services/store.svelte';
  import { appendLog, guiSelectPolicy, guiClientProbeNode, guiClientProbeStart, guiProbePolicy } from '$lib/services/core';
  import { listen } from '@tauri-apps/api/event';
  import { getGroupKindStyle, isSpecialOutboundProtocol } from '$lib/services/node-utils';
  import type { ProxyNode } from '$lib/types/protocol';
  import type { PolicyGroup } from '$lib/types/gui-api';
  import NodesDelayPopover from '$lib/components/tabs/NodesDelayPopover.svelte';
  import NodesGridCard from '$lib/components/tabs/NodesGridCard.svelte';
  import NodesGroupSidebar from '$lib/components/tabs/NodesGroupSidebar.svelte';
  import NodesListRow from '$lib/components/tabs/NodesListRow.svelte';
  import NodesToolbar from '$lib/components/tabs/NodesToolbar.svelte';
  import { createNodesProbeController } from '$lib/components/tabs/nodes-probe-controller.js';
  import { delayHistory, type DelayEntry } from '$lib/services/delay-history.svelte';
  import { buildProbeHistoryUpdates } from '$lib/services/policy-probe-history';
  import { error as toastError } from '$lib/services/toast.svelte';
  import {
    buildAllNodes,
    buildRuntimeOverlay,
    buildSections,
    collectProbingPolicyNodeTags,
    filterNodes,
    getActiveNodeTag,
    mergePolicyGroups,
    planProbeTargets,
    policyProbeTagForNode,
    resolveNodeGroup,
    type NodeSection,
  } from '$lib/components/tabs/nodes-view-model';

  // View state
  type ViewMode = 'list' | 'grid';
  const VIEW_MODE_KEY = 'znet-nodes-view-mode';
  let viewMode = $state<ViewMode>(store.uiMode === 'lite' ? 'list' : 'grid');
  let hideTimer: ReturnType<typeof setTimeout> | null = null;
  let isLite = $derived(store.uiMode === 'lite');
  let searchQuery = $state('');
  let selectedGroup = $state<string | null>(null);

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

  // Action state
  let switching = $state<string | null>(null);
  let probingNodeIds = $state<Set<string>>(new Set());
  let probingPolicyTags = $state<Set<string>>(new Set());
  let probingAll = $state(false);
  let probingRequested = $state(false);
  let probeProgress = $state({ done: 0, total: 0 });
  let lastError = $state<string | null>(null);

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
  type ProbeControllerState = {
    probingNodeIds: Set<string>;
    probingAll: boolean;
    probeProgress: { done: number; total: number };
    lastError: string | null;
  };
  const probeController = createNodesProbeController({
    listen,
    probeNode: guiClientProbeNode,
    probeAll: guiClientProbeStart,
    recordDelay: (targetTag: string, latencyMs: number | undefined, reachable: boolean) => {
      const at = Date.now();
      for (const update of buildProbeHistoryUpdates(groups, targetTag, latencyMs, reachable, at)) {
        delayHistory.record(
          update.tag,
          update.delayMs,
          update.reachable,
          update.at,
          update.selectedTag,
        );
      }
    },
    onProbeFailure: (failure: { targetTag?: string; message: string; scope: 'single' | 'batch' }) =>
      recordProbeFailure(failure),
    refreshPolicyGroups: () => guiState.refreshPolicyGroups(),
    onStateChange: (state: ProbeControllerState) => {
      probingNodeIds = state.probingNodeIds;
      probingAll = state.probingAll;
      probeProgress = state.probeProgress;
      lastError = state.lastError;
    },
  });

  // Collapsible group sections persisted to localStorage
  const COLLAPSE_KEY = 'znet-nodes-collapsed';
  let collapsedGroups = $state<Set<string>>(loadCollapsed());

  function loadCollapsed(): Set<string> {
    try {
      const raw = localStorage.getItem(COLLAPSE_KEY);
      if (!raw) return new Set();
      return new Set(JSON.parse(raw) as string[]);
    } catch {
      return new Set();
    }
  }

  function toggleCollapse(name: string) {
    const next = new Set(collapsedGroups);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    collapsedGroups = next;
    try {
      localStorage.setItem(COLLAPSE_KEY, JSON.stringify([...next]));
    } catch {
      // best-effort persistence
    }
  }

  // Kernel connection state
  const isCoreAvailable = $derived(guiState.isProcessRunning);
  const probeDisabledReason = $derived(
    !isCoreAvailable ? '内核未就绪，无法测速' : null,
  );
  // On mount, reload config-derived data so the page reflects the active profile.
  // Also pull runtime policy groups once in case the kernel is already connected.
  onMount(() => {
    viewMode = isLite ? 'list' : loadViewMode();
    void Promise.allSettled([
      guiState.refreshConfigNodes(),
      guiState.refreshConfigPolicyGroups(),
      guiState.refreshPolicyGroups(),
    ]);
  });

  onDestroy(() => {
    if (hideTimer) clearTimeout(hideTimer);
    probeController.cleanup();
  });

  // Policy groups: config skeleton first, runtime overlay second
  const groups = $derived.by<PolicyGroup[]>(() => {
    const config = guiState.configPolicyGroups;
    const runtime = guiState.policyGroups;

    return mergePolicyGroups(config, runtime);
  });

  const runtimeOverlay = $derived.by(() => {
    return buildRuntimeOverlay(groups);
  });

  // Build the full node list from config data plus runtime overlay.
  // Falls back to runtime-only or event-derived nodes when no config exists.
  const allNodes = $derived.by<ProxyNode[]>(() => {
    void runtimeOverlay;
    void delayHistory.history; // re-run when history updates
    return buildAllNodes({
      configNodes: guiState.configNodes,
      groups,
      runtimeOverlay,
      latestDelay: (tag) => delayHistory.latest(tag),
      latestProbeTime: (tag) => delayHistory.latestTime(tag),
      fallbackNodes: overviewData.proxyNodes,
    });
  });

  const filteredNodes = $derived.by(() => {
    return filterNodes({
      allNodes,
      groups,
      query: searchQuery.trim().toLowerCase(),
      selectedGroup,
    });
  });

  // In the all-nodes view, partition nodes by policy group into collapsible sections.
  // A node can belong to multiple groups, so assign it to the first match
  // to avoid duplicates. Ungrouped nodes fall back to the default section.
  const sections = $derived.by<NodeSection[]>(() => {
    return buildSections({
      allNodes,
      groups,
      query: searchQuery.trim().toLowerCase(),
    });
  });

  // Active selected tag for row/card highlight
  const activeNodeId = $derived.by(() => {
    return getActiveNodeTag(groups, selectedGroup);
  });

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
  /** Resolve the policy group a node belongs to. */
  function groupForNode(node: ProxyNode): PolicyGroup | undefined {
    return resolveNodeGroup({
      groups,
      runtimeOverlay,
      selectedGroup,
      nodeTag: node.tag,
    });
  }

  const GROUP_NODE_PROTOCOLS = new Set([
    'selector', 'url_test', 'urltest', 'fallback', 'load_balance', 'loadbalance', 'relay',
  ]);

  function isGroupNode(node: ProxyNode): boolean {
    return GROUP_NODE_PROTOCOLS.has((node.protocol ?? '').toLowerCase());
  }

  /** Check if a node is a direct member of a selector group.
   *  A nested group (e.g. urltest) inside a selector is selectable —
   *  policies.select sends the direct member tag, and the kernel resolves
   *  it recursively during engine resolve. */
  function isNodeSelectable(node: ProxyNode): boolean {
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

    // Global view: find the node's parent group
    const parentGroup = groupForNode(node);
    if (!parentGroup) return true; // fallback: allow selection
    return parentGroup.kind?.toLowerCase() === 'selector';
  }

  async function handleSelect(node: ProxyNode) {
    if (switching) return;
    if (!isCoreAvailable) {
      reportActionError('内核未就绪，无法切换节点');
      return;
    }
    if (!isNodeSelectable(node)) {
      reportActionError('当前策略组为自动选择组，不支持手动切换节点');
      return;
    }
    switching = node.id;
    lastError = null;
    try {
      // Resolve the selector group that contains this node as a direct member.
      // For nested groups (e.g. urltest inside selector), we send the group tag
      // as target — the kernel resolves it recursively during engine resolve.
      const policyTag = resolvePolicyTag(node);
      const result = await guiSelectPolicy(policyTag, node.tag);
      if (!result.accepted) {
        reportActionError(result.message ?? '内核未接受此选择');
      } else if (result.selected && result.selected !== node.tag) {
        // Kernel may have resolved to a different target — this is informational, not an error
        await guiState.refreshPolicyGroups();
      } else {
        await guiState.refreshPolicyGroups();
      }
    } catch (e) {
      reportActionError((e as { message?: string }).message ?? '切换节点失败');
    } finally {
      switching = null;
    }
  }

  /** Resolve the selector policy tag that contains the node as a direct member. */
  function resolvePolicyTag(node: ProxyNode): string {
    // 1. If user is browsing a specific selector group, use that
    if (selectedGroup) {
      const browsingGroup = groups.find((g) => g.name === selectedGroup);
      if (browsingGroup?.kind?.toLowerCase() === 'selector') {
        return selectedGroup;
      }
    }
    // 2. Try runtime overlay
    const overlayGroup = runtimeOverlay.get(node.tag)?.groupName;
    if (overlayGroup) {
      const g = groups.find((g) => g.name === overlayGroup);
      if (g?.kind?.toLowerCase() === 'selector') return overlayGroup;
    }
    // 3. Find first selector group containing this node
    const selectorGroup = groups.find(
      (g) => g.kind?.toLowerCase() === 'selector' && g.outbounds.some((o) => o.tag === node.tag),
    );
    if (selectorGroup) return selectorGroup.name;
    // 4. Fallback
    return 'proxy';
  }

  async function handleProbe(node: ProxyNode) {
    if (isSpecialOutboundProtocol(node.protocol)) return;
    if (!isCoreAvailable) {
      recordProbeFailure({ message: '内核未就绪', scope: 'single', targetTag: node.tag });
      return;
    }

    // A nested url_test group is rendered as a regular node card, but its
    // probe contract remains group-level: policies.probe measures every
    // member and completes through policy.probe.completed. A leaf card inside
    // that group still uses the ordinary single-outbound probe controller.
    const policyTag = policyProbeTagForNode(groups, node.tag);
    if (policyTag) {
      if (probingPolicyTags.has(policyTag)) return;
      lastError = null;
      try {
        await probePolicy(policyTag);
      } catch (error) {
        const timedOut = error instanceof PolicyProbeTimeoutError;
        recordProbeFailure({
          message: timedOut ? '等待内核返回测速结果超时' : (error instanceof Error ? error.message : String(error)),
          scope: 'policy',
          policyTag,
          outcome: timedOut ? 'timeout' : 'failed',
        });
      }
      return;
    }

    await probeController.handleProbe(node);
  }

  async function probePolicy(policyTag: string) {
    probingPolicyTags = new Set([...probingPolicyTags, policyTag]);
    // Register before sending the command so even an extremely fast probe
    // completion cannot race past the UI waiter.
    const completion = coreEvents.waitForPolicyProbe(policyTag, { trigger: 'manual' });
    try {
      const accepted = await guiProbePolicy(policyTag);
      if (!accepted.accepted || accepted.result?.probeTriggered === false) {
        void completion.catch(() => undefined);
        throw new Error(`内核未接受 ${policyTag} 的策略测速请求`);
      }
      const completed = await completion;
      const failedMembers = completed.members.filter(
        (member) => member.alive === false || !!member.lastError,
      );
      if (failedMembers.length > 0) {
        recordProbeFailure({
          message: `${failedMembers.length}/${completed.members.length} 个节点不可达`,
          scope: 'policy',
          policyTag,
          failedTargets: failedMembers.map((member) => member.tag),
        });
      }
    } catch (error) {
      void completion.catch(() => undefined);
      throw error;
    } finally {
      const next = new Set(probingPolicyTags);
      next.delete(policyTag);
      probingPolicyTags = next;
    }
  }

  async function handleProbeAll() {
    if (!isCoreAvailable) {
      recordProbeFailure({ message: '内核未就绪', scope: 'batch' });
      return;
    }
    if (probingRequested || probingAll || probingNodeIds.size > 0 || probingPolicyTags.size > 0) {
      return;
    }
    const targets = plannedProbeTargets;
    if (targets.nodes.length === 0 && targets.policyTags.length === 0) return;

    probingRequested = true;
    lastError = null;
    try {
      const policyRequests = targets.policyTags.map((policyTag) => probePolicy(policyTag));
      await Promise.all([
        targets.nodes.length > 0 ? probeController.handleProbeAll(targets.nodes) : Promise.resolve(),
        ...policyRequests,
      ]);
    } catch (error) {
      const timedOut = error instanceof PolicyProbeTimeoutError;
      recordProbeFailure({
        message: timedOut ? '等待内核返回测速结果超时' : (error instanceof Error ? error.message : String(error)),
        scope: 'batch',
        outcome: timedOut ? 'timeout' : 'failed',
      });
    } finally {
      probingRequested = false;
    }
  }

  $effect(() => {
    // Non-global mode hides the "全部节点" sidebar entry (gated to global
    // in NodesGroupSidebar), so the page must land on a concrete group —
    // default to the first one when groups load or when the current
    // selection becomes stale. Global mode keeps the all-nodes view.
    const proxyMode = guiState.proxyMode?.currentMode;
    if (proxyMode === 'global') return;
    if (groups.length === 0) return;
    if (!groups.some((g) => g.name === selectedGroup)) {
      selectedGroup = groups[0].name;
    }
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
  let popover = $state<PopoverState>({ visible: false, anchor: null, node: null });
  let popoverElement = $state<HTMLDivElement | null>(null);
  let popoverPositionVersion = $state(0);
  const popoverHistory = $derived.by<DelayEntry[]>(() =>
    popover.node ? delayHistory.getHistory(popover.node.tag) : [],
  );
  function showPopover(e: MouseEvent, node: ProxyNode) {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    const hist = delayHistory.getHistory(node.tag);
    if (hist.length === 0) return;
    popover = { visible: true, anchor: e.currentTarget as HTMLElement, node };
  }

  function hidePopover(delay = 300) {
    if (hideTimer) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      popover = { visible: false, anchor: null, node: null };
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
    // Use the rendered dimensions instead of an estimate. The chart and list
    // views have different heights, so an estimate can choose the wrong side.
    const popoverHeight = popoverElement?.offsetHeight ?? 112;
    const popoverWidth = popoverElement?.offsetWidth ?? 220;
    const viewportHeight = typeof window === 'undefined' ? 800 : window.innerHeight;
    const viewportWidth = typeof window === 'undefined' ? 1200 : window.innerWidth;
    const left = Math.max(
      edgePadding,
      Math.min(viewportWidth - popoverWidth - edgePadding, r.left + (r.width - popoverWidth) / 2),
    );
    const spaceAbove = r.top - gap - edgePadding;
    const spaceBelow = viewportHeight - r.bottom - gap - edgePadding;
    const placeAbove = spaceAbove >= popoverHeight || spaceAbove >= spaceBelow;
    const preferredTop = placeAbove ? r.top - gap - popoverHeight : r.bottom + gap;
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
    allNodesCount={allNodes.length}
    {selectedGroup}
    proxyMode={guiState.proxyMode?.currentMode}
    onSelectGroup={(groupName) => (selectedGroup = groupName)}
  />

  <!-- Right: Node panel -->
  <div class="node-panel">
    <NodesToolbar
      {selectedGroup}
      filteredCount={filteredNodes.length}
      isCoreAvailable={isCoreAvailable}
      {searchQuery}
      {viewMode}
      {isLite}
      probingAll={probingRequested || probingAll}
      {probeProgress}
      canProbeAll={isCoreAvailable && !probingRequested && !probingAll && probingNodeIds.size === 0 && probingPolicyTags.size === 0 && (plannedProbeTargets.nodes.length > 0 || plannedProbeTargets.policyTags.length > 0)}
      {probeDisabledReason}
      onSearchQueryChange={(value) => (searchQuery = value)}
      onViewModeChange={setViewMode}
      onProbeAll={handleProbeAll}
    />

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
          <button class="empty-clear" onclick={() => (searchQuery = '')}>清除搜索</button>
        {:else if allNodes.length === 0}
          <span class="empty-text">暂无节点数据</span>
          <span class="empty-hint">
            {#if !isCoreAvailable}
              内核未连接，且当前没有生效的代理配置。请先在“配置”页导入并启用一份配置。
            {:else}
              当前配置不包含节点。请在“配置”页导入一份包含 outbounds 的代理配置。
            {/if}
          </span>
          <button class="empty-clear" onclick={() => (store.activeTab = 'profiles')}>前往配置页</button>
        {:else}
          <span class="empty-text">暂无节点数据</span>
        {/if}
      </div>
    {:else if selectedGroup}
      <!-- Single group view -->
      {#if viewMode === 'list'}
        <div class="node-list node-list-scroll">
          {#each filteredNodes as node (node.id)}
            <NodesListRow
              {node}
              isActive={activeNodeId === node.tag}
              isSwitching={switching === node.id}
              isProbing={isNodeProbing(node)}
              probingAll={probingRequested || probingAll}
              probeDisabled={!isCoreAvailable || isSpecialOutboundProtocol(node.protocol)}
              selectDisabled={!isCoreAvailable || switching !== null || !store.isActionOperable('policies.select') || !isNodeSelectable(node)}
              onSelectNode={handleSelect}
              onProbeNode={handleProbe}
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
              probingAll={probingRequested || probingAll}
              probeDisabled={!isCoreAvailable || isSpecialOutboundProtocol(node.protocol)}
              selectDisabled={!isCoreAvailable || switching !== null || !store.isActionOperable('policies.select') || !isNodeSelectable(node)}
              onSelectNode={handleSelect}
              onProbeNode={handleProbe}
              onShowPopover={showPopover}
              onHidePopover={hidePopover}
            />
          {/each}
        </div>
      {/if}
    {:else}
      <!-- All-nodes view with collapsible group sections -->
      <div class="node-sections">
        {#each sections as section (section.name)}
          {@const isCollapsed = collapsedGroups.has(section.name)}
          <section class="node-section">
            <button class="section-header" onclick={() => toggleCollapse(section.name)}>
              <span class="section-caret {isCollapsed ? 'collapsed' : ''}">
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="9,18 15,12 9,6"/>
                </svg>
              </span>
              <span class="section-title">{section.name}</span>
              {#if getGroupKindStyle(section.kind)}
                <span class="section-kind" style="color: {getGroupKindStyle(section.kind)?.color}">
                  {getGroupKindStyle(section.kind)?.label}
                </span>
              {/if}
              <span class="section-count">{section.nodes.length}</span>
            </button>
            {#if !isCollapsed}
              {#if viewMode === 'list'}
                <div class="node-list">
                  {#each section.nodes as node (node.id)}
                    <NodesListRow
                      {node}
                      isActive={activeNodeId === node.tag}
                      isSwitching={switching === node.id}
                      isProbing={isNodeProbing(node)}
                      probingAll={probingRequested || probingAll}
                      probeDisabled={!isCoreAvailable || isSpecialOutboundProtocol(node.protocol)}
                      selectDisabled={!isCoreAvailable || switching !== null || !store.isActionOperable('policies.select') || !isNodeSelectable(node)}
                      onSelectNode={handleSelect}
                      onProbeNode={handleProbe}
                      onShowPopover={showPopover}
                      onHidePopover={hidePopover}
                    />
                  {/each}
                </div>
              {:else}
                <div class="node-grid">
                  {#each section.nodes as node (node.id)}
                    <NodesGridCard
                      {node}
                      isActive={activeNodeId === node.tag}
                      isSwitching={switching === node.id}
                      isProbing={isNodeProbing(node)}
                      probingAll={probingRequested || probingAll}
                      probeDisabled={!isCoreAvailable || isSpecialOutboundProtocol(node.protocol)}
                      selectDisabled={!isCoreAvailable || switching !== null || !store.isActionOperable('policies.select') || !isNodeSelectable(node)}
                      onSelectNode={handleSelect}
                      onProbeNode={handleProbe}
                      onShowPopover={showPopover}
                      onHidePopover={hidePopover}
                    />
                  {/each}
                </div>
              {/if}
            {/if}
          </section>
        {/each}
      </div>
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
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    position: relative;
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

  .empty-clear {
    font-size: 11px;
    color: var(--accent-foreground);
    background: none;
    border: none;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .empty-clear:hover { opacity: 0.8; }

  /* Collapsible sections in the all-nodes view */
  .node-sections {
    flex: 1;
    overflow-y: auto;
    padding: 4px 6px 8px;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .node-section {
    display: flex;
    flex-direction: column;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border: none;
    background: transparent;
    cursor: pointer;
    border-radius: 6px;
    text-align: left;
    transition: background 0.12s ease;
  }

  .section-header:hover { background: var(--muted); }

  .section-caret {
    display: inline-flex;
    color: var(--muted-foreground);
    transition: transform 0.15s ease;
  }

  .section-caret.collapsed {
    transform: rotate(0deg);
  }

  .section-caret:not(.collapsed) {
    transform: rotate(90deg);
  }

  .section-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--foreground);
  }

  .section-kind {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    opacity: 0.8;
  }

  .section-count {
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-mono);
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--muted-foreground);
    margin-left: auto;
  }

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


