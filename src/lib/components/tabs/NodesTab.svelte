<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { store } from '$lib/services/store.svelte';
  import { selectPolicy, guiClientProbeNode, guiClientProbeStart } from '$lib/services/core';
  import { listen } from '@tauri-apps/api/event';
  import { getGroupKindStyle } from '$lib/services/node-utils';
  import type { ProxyNode } from '$lib/types/protocol';
  import type { PolicyGroup } from '$lib/types/gui-api';
  import NodesDelayPopover from '$lib/components/tabs/NodesDelayPopover.svelte';
  import NodesGridCard from '$lib/components/tabs/NodesGridCard.svelte';
  import NodesGroupSidebar from '$lib/components/tabs/NodesGroupSidebar.svelte';
  import NodesListRow from '$lib/components/tabs/NodesListRow.svelte';
  import NodesToolbar from '$lib/components/tabs/NodesToolbar.svelte';
  import { createNodesProbeController } from '$lib/components/tabs/nodes-probe-controller.js';
  import { delayHistory, type DelayEntry } from '$lib/services/delay-history.svelte';
  import {
    buildAllNodes,
    buildRuntimeOverlay,
    buildSections,
    filterNodes,
    getActiveNodeTag,
    isSelectableGroup,
    normalizeSelectedGroup,
    resolveNodeGroup,
    type NodeSection,
  } from '$lib/components/tabs/nodes-view-model';

  // View state
  type ViewMode = 'list' | 'grid';
  let viewMode = $state<ViewMode>(store.uiMode === 'lite' ? 'list' : 'grid');
  let isLite = $derived(store.uiMode === 'lite');
  let searchQuery = $state('');
  let sortMode = $state<'delay' | 'name'>('delay');
  let selectedGroup = $state<string | null>(null);

  // Action state
  let switching = $state<string | null>(null);
  let probingNodeIds = $state<Set<string>>(new Set());
  let probingAll = $state(false);
  let probeProgress = $state({ done: 0, total: 0 });
  let lastError = $state<string | null>(null);
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
    recordDelay: (targetTag: string, latencyMs: number | undefined, reachable: boolean) =>
      delayHistory.record(targetTag, latencyMs, reachable),
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
    !isCoreAvailable ? '\u5185\u6838\u672a\u5c31\u7eea\uff0c\u65e0\u6cd5\u6d4b\u901f' : null,
  );
  // On mount, reload config-derived data so the page reflects the active profile.
  // Also pull runtime policy groups once in case the kernel is already connected.
  onMount(() => {
    void Promise.allSettled([
      guiState.refreshConfigNodes(),
      guiState.refreshConfigPolicyGroups(),
      guiState.refreshPolicyGroups(),
    ]);
  });

  onDestroy(() => {
    probeController.cleanup();
  });

  // Policy groups: config skeleton first, runtime overlay second
  const groups = $derived.by<PolicyGroup[]>(() => {
    const config = guiState.configPolicyGroups;
    const runtime = guiState.policyGroups;

    // Merge selected tag from runtime onto config groups.
    const runtimeSelected = new Map<string, string | undefined>();
    for (const rg of runtime) {
      if (rg.selected) runtimeSelected.set(rg.name, rg.selected);
    }

    if (config.length > 0) {
      return config.map((cg) => ({
        ...cg,
        selected: runtimeSelected.get(cg.name) ?? cg.selected,
      }));
    }
    return runtime;
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
      fallbackNodes: overviewData.proxyNodes,
    });
  });

  const filteredNodes = $derived.by(() => {
    return filterNodes({
      allNodes,
      groups,
      query: searchQuery.trim().toLowerCase(),
      selectedGroup,
      sortMode,
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
      sortMode,
    });
  });

  // Active selected tag for row/card highlight
  const activeNodeId = $derived.by(() => {
    return getActiveNodeTag(groups);
  });

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

  /** Only selector groups allow manual outbound selection. */
  function isNodeSelectable(node: ProxyNode): boolean {
    return isSelectableGroup(groupForNode(node));
  }

  async function handleSelect(node: ProxyNode) {
    if (switching) return;
    if (!isCoreAvailable) {
      lastError = '\u5185\u6838\u672a\u5c31\u7eea\uff0c\u65e0\u6cd5\u5207\u6362\u8282\u70b9';
      return;
    }
    if (!isNodeSelectable(node)) {
      lastError = '\u5f53\u524d\u7b56\u7565\u7ec4\u4e3a\u81ea\u52a8\u9009\u62e9\u7ec4\uff0c\u4e0d\u652f\u6301\u624b\u52a8\u5207\u6362\u8282\u70b9';
      return;
    }
    switching = node.id;
    lastError = null;
    try {
      const policyTag =
        selectedGroup
        ?? runtimeOverlay.get(node.tag)?.groupName
        ?? groups.find((g) => g.outbounds.some((o) => o.tag === node.tag))?.name
        ?? 'proxy';
      const result = await selectPolicy(policyTag, node.tag);
      if (!result.available) {
        lastError = '\u5185\u6838\u672a\u5c31\u7eea\uff0c\u65e0\u6cd5\u5207\u6362\u8282\u70b9';
      } else if (result.error) {
        lastError = result.error.message;
      } else {
        await guiState.refreshPolicyGroups();
      }
    } catch (e) {
      lastError = String(e);
    } finally {
      switching = null;
    }
  }

  async function handleProbe(node: ProxyNode) {
    if (!isCoreAvailable) {
      lastError = '\u5185\u6838\u672a\u5c31\u7eea\uff0c\u65e0\u6cd5\u6d4b\u901f';
      return;
    }
    await probeController.handleProbe(node);
  }

  async function handleProbeAll() {
    if (!isCoreAvailable) {
      lastError = '\u5185\u6838\u672a\u5c31\u7eea\uff0c\u65e0\u6cd5\u6d4b\u901f';
      return;
    }
    if (probingAll || probingNodeIds.size > 0) {
      return;
    }
    await probeController.handleProbeAll(filteredNodes);
  }

  $effect(() => {
    const normalized = normalizeSelectedGroup(selectedGroup, groups);
    if (normalized !== selectedGroup) {
      selectedGroup = normalized;
    }
  });

  // Render the popover outside .nodes-root so transformed ancestors do not
  // turn it into a clipped descendant of the animated panel container.
  interface PopoverState {
    visible: boolean;
    anchor: DOMRect | null;
    node: ProxyNode | null;
    hist: DelayEntry[];
  }
  let popover = $state<PopoverState>({ visible: false, anchor: null, node: null, hist: [] });

  function showPopover(e: MouseEvent, node: ProxyNode) {
    const hist = delayHistory.getHistory(node.tag);
    if (hist.length < 2) return;
    const el = e.currentTarget as HTMLElement;
    popover = { visible: true, anchor: el.getBoundingClientRect(), node, hist };
  }

  function hidePopover() {
    popover = { visible: false, anchor: null, node: null, hist: [] };
  }

  function popoverStyle(): string {
    if (!popover.anchor) return '';
    const r = popover.anchor;
    const top = r.top - 6;
    const left = r.left + r.width / 2;
    return `position:fixed; left:${Math.round(left)}px; top:${Math.round(top)}px; transform:translate(-50%, -100%); z-index:9999;`;
  }
</script>

<div class="nodes-root animate-fade-in">
  <NodesGroupSidebar
    {groups}
    allNodesCount={allNodes.length}
    {selectedGroup}
    onSelectGroup={(groupName) => (selectedGroup = groupName)}
  />

  <!-- Right: Node panel -->
  <div class="node-panel">
    <NodesToolbar
      {selectedGroup}
      filteredCount={filteredNodes.length}
      isCoreAvailable={isCoreAvailable}
      {searchQuery}
      {sortMode}
      {viewMode}
      {isLite}
      {probingAll}
      {probeProgress}
      canProbeAll={isCoreAvailable && !probingAll && probingNodeIds.size === 0 && filteredNodes.length > 0}
      {probeDisabledReason}
      onSearchQueryChange={(value) => (searchQuery = value)}
      onSortModeChange={(mode) => (sortMode = mode)}
      onViewModeChange={(mode) => (viewMode = mode)}
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
          <span class="empty-text">\u65e0\u5339\u914d\u8282\u70b9</span>
          <button class="empty-clear" onclick={() => (searchQuery = '')}>\u6e05\u9664\u641c\u7d22</button>
        {:else if allNodes.length === 0}
          <span class="empty-text">\u6682\u65e0\u8282\u70b9\u6570\u636e</span>
          <span class="empty-hint">
            {#if !isCoreAvailable}
              \u5185\u6838\u672a\u8fde\u63a5\uff0c\u4e14\u5f53\u524d\u6ca1\u6709\u751f\u6548\u7684\u4ee3\u7406\u914d\u7f6e\u3002\u8bf7\u5148\u5728\u201c\u914d\u7f6e\u201d\u9875\u5bfc\u5165\u5e76\u542f\u7528\u4e00\u4efd\u914d\u7f6e\u3002
            {:else}
              \u5f53\u524d\u914d\u7f6e\u4e0d\u5305\u542b\u8282\u70b9\u3002\u8bf7\u5728\u201c\u914d\u7f6e\u201d\u9875\u5bfc\u5165\u4e00\u4efd\u5305\u542b outbounds \u7684\u4ee3\u7406\u914d\u7f6e\u3002
            {/if}
          </span>
          <button class="empty-clear" onclick={() => (store.activeTab = 'profiles')}>\u524d\u5f80\u914d\u7f6e\u9875</button>
        {:else}
          <span class="empty-text">\u6682\u65e0\u8282\u70b9\u6570\u636e</span>
        {/if}
      </div>
    {:else if selectedGroup}
      <!-- Single group view -->
      {#if viewMode === 'list'}
        <div class="node-list">
          {#each filteredNodes as node (node.id)}
            <NodesListRow
              {node}
              isActive={activeNodeId === node.tag}
              isSwitching={switching === node.id}
              isProbing={probingNodeIds.has(node.id)}
              {probingAll}
              probeDisabled={!isCoreAvailable}
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
              isProbing={probingNodeIds.has(node.id)}
              {probingAll}
              probeDisabled={!isCoreAvailable}
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
                      isProbing={probingNodeIds.has(node.id)}
                      {probingAll}
                      probeDisabled={!isCoreAvailable}
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
                      isProbing={probingNodeIds.has(node.id)}
                      {probingAll}
                      probeDisabled={!isCoreAvailable}
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

    <!-- Error toast -->
    {#if lastError}
      <div class="node-error">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="flex-shrink-0">
          <circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>
        </svg>
        <span>{lastError}</span>
        <button class="error-dismiss" onclick={() => (lastError = null)} aria-label="\u5173\u95ed\u9519\u8bef\u63d0\u793a">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><line x1="2" y1="2" x2="8" y2="8"/><line x1="8" y1="2" x2="2" y2="8"/></svg>
        </button>
      </div>
    {/if}
  </div>
</div>

{#if popover.visible && popover.node}
  <NodesDelayPopover
    node={popover.node}
    hist={popover.hist}
    positionStyle={popoverStyle()}
  />
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
  .node-error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 6px;
    padding: 8px 10px;
    background: rgba(239, 68, 68, 0.06);
    border: 1px solid rgba(239, 68, 68, 0.14);
    border-radius: 6px;
    font-size: 11px;
    color: var(--destructive);
    flex-shrink: 0;
  }

  .error-dismiss {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-left: auto;
    background: none;
    border: none;
    color: var(--destructive);
    opacity: 0.5;
    cursor: pointer;
    padding: 2px;
  }

  .error-dismiss:hover { opacity: 1; }

  /* Responsive layout */
  @media (max-width: 700px) {
    .node-grid {
      grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    }
  }
</style>


