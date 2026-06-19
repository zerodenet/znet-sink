<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { coreEvents } from '$lib/services/core-events.svelte';
  import { store } from '$lib/services/store.svelte';
  import { selectPolicy, guiClientProbeNode, guiClientProbeStart } from '$lib/services/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import type { ProxyNode } from '$lib/types/protocol';
  import type { PolicyGroup } from '$lib/types/gui-api';
  import {
    parseNodeName,
    getProtocolStyle,
    gradeDelay,
    formatDelay,
    delayBarWidth,
    getGroupKindStyle,
    getNodeChips,
  } from '$lib/services/node-utils';
  import { createBatchProbeState } from '$lib/components/tabs/nodes-probe-state.js';
  import { delayHistory, type DelayEntry } from '$lib/services/delay-history.svelte';

  // ── View state ──
  type ViewMode = 'list' | 'grid';
  let viewMode = $state<ViewMode>(store.uiMode === 'lite' ? 'list' : 'grid');
  let isLite = $derived(store.uiMode === 'lite');
  let searchQuery = $state('');
  let sortMode = $state<'delay' | 'name'>('delay');
  let selectedGroup = $state<string | null>(null);

  // ── Action state ──
  let switching = $state<string | null>(null);
  let probingNodeIds = $state<Set<string>>(new Set());
  let probingAll = $state(false);
  let probeProgress = $state({ done: 0, total: 0 });
  let lastError = $state<string | null>(null);
  let activeProbeResultUnlisten = $state<UnlistenFn | null>(null);
  let activeProbeProgressUnlisten = $state<UnlistenFn | null>(null);
  let activeProbeCompleteUnlisten = $state<UnlistenFn | null>(null);
  let activeProbeCompletionResolve = $state<(() => void) | null>(null);
  let activeBatchProbeState = $state<ReturnType<typeof createBatchProbeState> | null>(null);

  // ── Collapsible group sections (persisted to localStorage) ──
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

  // ── Kernel connection state ──
  const isConnected = $derived(guiState.isConnected);

  // ── On mount: re-fetch config-derived data so the node page always
  //     reflects the current active profile. Also pull runtime policy
  //     groups once in case the kernel is already connected. ──
  onMount(() => {
    void Promise.allSettled([
      guiState.refreshConfigNodes(),
      guiState.refreshConfigPolicyGroups(),
      guiState.refreshPolicyGroups(),
    ]);
  });

  onDestroy(() => {
    cleanupProbeListeners();
  });

  function addProbingNodeIds(ids: Iterable<string>) {
    const next = new Set(probingNodeIds);
    for (const id of ids) next.add(id);
    probingNodeIds = next;
  }

  function removeProbingNodeIds(ids: Iterable<string>) {
    const next = new Set(probingNodeIds);
    for (const id of ids) next.delete(id);
    probingNodeIds = next;
  }

  function cleanupProbeListeners() {
    activeProbeResultUnlisten?.();
    activeProbeProgressUnlisten?.();
    activeProbeCompleteUnlisten?.();
    activeProbeCompletionResolve?.();
    activeProbeResultUnlisten = null;
    activeProbeProgressUnlisten = null;
    activeProbeCompleteUnlisten = null;
    activeProbeCompletionResolve = null;
    activeBatchProbeState = null;
  }

  // ── Kernel interaction: keep runtime overlay fresh while this tab is open.
  //     CoreStatusCard normally drives refreshOnTick, but it only mounts in
  //     Overview, so we mirror its tick watcher here. Also watch modeTick
  //     so a rule/global switch re-syncs the node list immediately. ──
  $effect(() => {
    const tick = coreEvents.statusTick;
    const modeTick = guiState.modeTick;
    void modeTick; // re-run on mode change
    if ((tick > 0 || modeTick > 0) && guiState.isConnected) {
      void guiState.refreshPolicyGroups();
    }
  });

  // ── Policy groups (config skeleton first, runtime overlays second) ──
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

  // ── Runtime overlay: tag → { delay, alive, selected, group } ──
  interface RuntimeOverlay {
    delayMs?: number;
    alive?: boolean;
    selected?: boolean;
    groupName?: string;
  }

  const runtimeOverlay = $derived.by(() => {
    const map = new Map<string, RuntimeOverlay>();
    for (const group of groups) {
      for (const ob of group.outbounds) {
        map.set(ob.tag, {
          delayMs: ob.delayMs,
          alive: ob.alive,
          selected: group.selected === ob.tag,
          groupName: group.name,
        });
      }
    }
    return map;
  });

  // ── Build the full node list: config static attributes + runtime overlay.
  //     Falls back to runtime-only / event-scraped nodes when no config. ──
  const allNodes = $derived.by<ProxyNode[]>(() => {
    // touch reactive sources explicitly
    const cnodes = guiState.configNodes;
    void runtimeOverlay;
    void delayHistory.history; // re-run when history updates

    // Path A: config nodes (static skeleton) + runtime overlay + delay history
    if (cnodes.length > 0) {
      return cnodes
        .filter((cn) => !cn.isSelector)
        .map<ProxyNode>((cn) => {
          const rt = runtimeOverlay.get(cn.tag);
          const parsed = parseNodeName(cn.tag);
          const histLatest = delayHistory.latest(cn.tag);
          const delay = rt?.delayMs ?? histLatest ?? 0;
          return {
            id: cn.tag,
            tag: cn.tag,
            name: cn.tag,
            emoji: parsed.emoji,
            cleanName: parsed.cleanName,
            protocol: cn.protocol !== 'unknown' ? cn.protocol : 'proxy',
            delay,
            selected: rt?.selected,
            alive: rt?.alive,
            domain: rt?.groupName ?? 'policy',
            server: cn.server,
            port: cn.port,
            udp: cn.udp,
            network: cn.network,
            tls: cn.tls,
            sni: cn.sni,
            cipher: cn.cipher,
          };
        });
    }

    // Path B: runtime groups only (kernel connected, no static config)
    const seen = new Set<string>();
    const out: ProxyNode[] = [];
    for (const group of groups) {
      for (const outbound of group.outbounds) {
        const key = outbound.tag.toLowerCase();
        if (seen.has(key)) continue;
        seen.add(key);
        const parsed = parseNodeName(outbound.tag);
        out.push({
          id: `${group.name}:${outbound.tag}`,
          tag: outbound.tag,
          name: outbound.tag,
          emoji: parsed.emoji,
          cleanName: parsed.cleanName,
          protocol: outbound.type || 'proxy',
          delay: outbound.delayMs ?? delayHistory.latest(outbound.tag) ?? 0,
          selected: group.selected === outbound.tag,
          alive: outbound.alive,
          domain: group.name,
        });
      }
    }
    if (out.length > 0) return out;

    // Path C: last resort — nodes scraped from events
    return overviewData.proxyNodes;
  });

  // ── Filtering + sorting ──
  function matchesSearch(node: ProxyNode, q: string): boolean {
    if (!q) return true;
    const hay = `${node.name} ${node.protocol} ${node.server ?? ''} ${node.cleanName ?? ''}`.toLowerCase();
    return hay.includes(q);
  }

  function sortNodes(nodes: ProxyNode[]): ProxyNode[] {
    return [...nodes].sort((a, b) => {
      if (sortMode === 'delay') {
        if (a.delay === 0 && b.delay > 0) return 1;
        if (b.delay === 0 && a.delay > 0) return -1;
        return a.delay - b.delay;
      }
      return a.name.localeCompare(b.name);
    });
  }

  /** Recursively collect real-node tags in a group, expanding nested groups
   *  (a group whose outbounds reference another group). Guards against
   *  cycles. Fixes "组套组" — selecting a parent group must show the child
   *  group's nodes too, and the count must reflect real nodes, not group
   *  references. */
  function collectGroupNodeTags(groupName: string, visited: Set<string> = new Set()): Set<string> {
    if (visited.has(groupName)) return new Set();
    visited.add(groupName);
    const group = groups.find((g) => g.name === groupName);
    if (!group) return new Set();
    const groupTags = new Set(groups.map((g) => g.name));
    const tags = new Set<string>();
    for (const ob of group.outbounds) {
      if (groupTags.has(ob.tag)) {
        // Nested group — recurse into its members.
        for (const t of collectGroupNodeTags(ob.tag, visited)) tags.add(t);
      } else {
        tags.add(ob.tag);
      }
    }
    return tags;
  }

  const filteredNodes = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    let nodes = allNodes.filter((n) => matchesSearch(n, q));

    if (selectedGroup) {
      const tags = collectGroupNodeTags(selectedGroup);
      if (tags.size > 0) {
        const matched = nodes.filter((n) => tags.has(n.tag));
        // Only apply group filter if it doesn't empty the list (a group may
        // exist in config but not yet loaded from the kernel).
        if (matched.length > 0) nodes = matched;
      }
    }
    return sortNodes(nodes);
  });

  // ── "All nodes" view: partition by policy group for collapsible sections.
  //     A node can belong to multiple groups; we assign it to the first
  //     matching group to avoid duplicates. Ungrouped nodes go to "其他". ──
  interface NodeSection {
    name: string;
    kind?: string;
    nodes: ProxyNode[];
  }

  const sections = $derived.by<NodeSection[]>(() => {
    const q = searchQuery.trim().toLowerCase();
    const filtered = allNodes.filter((n) => matchesSearch(n, q));
    if (filtered.length === 0) return [];

    // tag → first group that contains it
    const tagToGroup = new Map<string, string>();
    for (const g of groups) {
      for (const ob of g.outbounds) {
        if (!tagToGroup.has(ob.tag)) tagToGroup.set(ob.tag, g.name);
      }
    }

    const buckets = new Map<string, ProxyNode[]>();
    const groupOrder: string[] = groups.map((g) => g.name);
    const orphan: ProxyNode[] = [];

    for (const node of filtered) {
      const gname = tagToGroup.get(node.tag);
      if (gname) {
        if (!buckets.has(gname)) buckets.set(gname, []);
        buckets.get(gname)!.push(node);
      } else {
        orphan.push(node);
      }
    }

   const out: NodeSection[] = [];
   for (const name of groupOrder) {
     const items = buckets.get(name);
     if (items && items.length > 0) {
       const g = groups.find((x) => x.name === name);
       out.push({ name, kind: g?.kind, nodes: sortNodes(items) });
     }
   }
    // When policy groups exist, hide ungrouped (plain) nodes so the view
    // shows real groups first. Only fall back to the orphan bucket when
    // no group section rendered anything (e.g. groups exist but haven't
    // loaded their node data yet).
    if (orphan.length > 0 && out.length === 0) {
     out.push({ name: '其他', nodes: sortNodes(orphan) });
   }
    return out;
  });

  // ── Active selected tag (for highlight) ──
  const activeNodeId = $derived.by(() => {
    for (const g of groups) {
      if (g.selected) return g.selected;
    }
    return undefined;
  });

 // ── Actions ──
 /// Resolve the PolicyGroup a node belongs to (by selectedGroup sidebar,
 /// runtime overlay, or outbound membership), mirroring handleSelect logic.
function groupForNode(node: ProxyNode): PolicyGroup | undefined {
  const byName = (name: string | null | undefined) =>
    name ? groups.find((g) => g.name === name) : undefined;
  return (
     byName(selectedGroup) ??
     byName(runtimeOverlay.get(node.tag)?.groupName) ??
     groups.find((g) => g.outbounds.some((o) => o.tag === node.tag))
   );
 }

 /// Only "selector" groups honor a user-picked outbound. url-test /
 /// fallback / load-balance auto-select, so manual selection is meaningless.
 function isNodeSelectable(node: ProxyNode): boolean {
   const group = groupForNode(node);
   if (!group) return true; // no group info yet — allow (backend guards)
   return group.kind?.toLowerCase() === 'selector';
 }

 async function handleSelect(node: ProxyNode) {
   if (switching) return;
   if (!isNodeSelectable(node)) {
     lastError = 'auto-select group — manual selection not available';
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
      const result = await selectPolicy(policyTag, node.name);
      if (!result.available) {
        lastError = '内核未连接，无法切换节点';
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
    if (probingAll || probingNodeIds.has(node.id)) return;
    addProbingNodeIds([node.id]);
    lastError = null;
    try {
      const result = await guiClientProbeNode(node.tag);
      delayHistory.record(node.tag, result.latencyMs, result.reachable);
      await guiState.refreshPolicyGroups();
    } catch (e) {
      lastError = String(e);
    } finally {
      removeProbingNodeIds([node.id]);
    }
  }

  async function handleProbeAll() {
    if (probingAll) return;
    probingAll = true;
    const batchNodes = [...filteredNodes];
    const tags = batchNodes.map((n) => n.tag);
    const batchProbeState = createBatchProbeState(batchNodes);
    activeBatchProbeState = batchProbeState;
    probingNodeIds = batchProbeState.probingNodeIds();
    probeProgress = { done: 0, total: batchNodes.length };
    lastError = null;

    try {
      cleanupProbeListeners();

      let resolveCompletion: (() => void) | null = null;
      const completion = new Promise<void>((resolve) => {
        resolveCompletion = resolve;
      });
      activeProbeCompletionResolve = resolveCompletion;

      activeProbeResultUnlisten = await listen<{ targetTag: string; reachable: boolean; latencyMs?: number }>(
        'probe:result',
        (event) => {
          const { targetTag, reachable, latencyMs } = event.payload;
          delayHistory.record(targetTag, latencyMs, reachable);
          batchProbeState.resolveTag(targetTag);
          probingNodeIds = batchProbeState.probingNodeIds();
        },
      );

      activeProbeProgressUnlisten = await listen<{ done: number; total: number }>(
        'probe:progress',
        (event) => {
          probeProgress = { done: event.payload.done, total: event.payload.total };
        },
      );

      activeProbeCompleteUnlisten = await listen<{ total: number; reachable: number; failed: number }>(
        'probe:complete',
        async () => {
          batchProbeState.clear();
          probingNodeIds = batchProbeState.probingNodeIds();
          await guiState.refreshPolicyGroups();
          probingAll = false;
          resolveCompletion?.();
          activeProbeCompletionResolve = null;
        },
      );

      await guiClientProbeStart(tags);
      await completion;
    } catch (e) {
      lastError = String(e);
      probingAll = false;
      batchProbeState.clear();
      probingNodeIds = batchProbeState.probingNodeIds();
    } finally {
      cleanupProbeListeners();
    }
  }

  function selectFirstGroup() {
    if (groups.length > 0 && !selectedGroup) {
      selectedGroup = groups[0].name;
    }
  }

  $effect(() => {
    selectFirstGroup();
  });

  // ── Sparkline geometry for delay-history popover ──
  interface SparkPoint {
    x: number;
    y: number;
    alive: boolean;
  }

  interface Sparkline {
    maxDelay: number;
    points: SparkPoint[];
  }

  /** Build the normalized point set for a mini delay sparkline (120×32 viewBox). */
  function buildSparkline(hist: DelayEntry[]): Sparkline {
    const maxDelay = Math.max(1, ...hist.map((h) => h.delay));
    const points = hist.map((h, i) => {
      const x = (i / Math.max(1, hist.length - 1)) * 120;
      const y = 30 - (h.delay > 0 ? Math.min(28, (h.delay / maxDelay) * 28) : 0);
      return { x, y, alive: h.delay > 0 };
    });
    return { maxDelay, points };
  }

  /** Reduce sparkline points to an SVG `polyline points=` string. */
  function sparklinePath(spark: Sparkline): string {
    return spark.points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ');
  }

  /** Mean latency across alive probes, for the popover footer. */
  function meanDelay(hist: DelayEntry[]): string {
    const alive = hist.filter((h) => h.delay > 0);
    if (alive.length === 0) return '—';
    return String(Math.round(alive.reduce((a, b) => a + b.delay, 0) / alive.length));
  }

  // ── Portal popover: rendered outside .nodes-root with position:fixed so it
  //     escapes any ancestor overflow:hidden / transform-induced containing
  //     block.  The @keyframes fade-in animation on .nodes-root creates a new
  //     CSS containing block (via `transform`), trapping descendants inside
  //     overflow:hidden boundaries. ──
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
  <!-- Left: Policy group sidebar -->
  <aside class="group-sidebar">
    <div class="group-header">
      <span class="group-header-label">策略组</span>
      <span class="group-header-count">{groups.length}</span>
    </div>

    <button
      class="group-item {!selectedGroup ? 'active' : ''}"
      onclick={() => (selectedGroup = null)}
    >
      <div class="group-info">
        <span class="group-name">全部节点</span>
      </div>
      <span class="group-count">{allNodes.length}</span>
    </button>

    {#each groups as group}
      <button
        class="group-item {selectedGroup === group.name ? 'active' : ''}"
        onclick={() => (selectedGroup = group.name)}
      >
        <div class="group-info">
          <div class="group-name-row">
            <span class="group-name truncate">{group.name}</span>
            {#if getGroupKindStyle(group.kind)}
              <span
                class="group-kind"
                style="color: {getGroupKindStyle(group.kind)?.color}"
              >{getGroupKindStyle(group.kind)?.label}</span>
            {/if}
          </div>
          {#if group.selected}
            <span class="group-selected truncate">
              <span class="group-selected-dot"></span>
              {group.selected}
            </span>
          {/if}
        </div>
        <span class="group-count">{collectGroupNodeTags(group.name).size}</span>
      </button>
    {/each}

    {#if groups.length === 0}
      {#if allNodes.length > 0}
        <div class="group-empty">配置节点 ({allNodes.length})</div>
      {:else}
        <div class="group-empty">等待数据…</div>
      {/if}
    {/if}
  </aside>

  <!-- Right: Node panel -->
  <div class="node-panel">
    <!-- Toolbar -->
    <div class="node-toolbar">
      <div class="toolbar-left">
        <span class="node-title">{selectedGroup || '全部节点'}</span>
        <span class="node-count">{filteredNodes.length}</span>
        <span
          class="conn-badge {isConnected ? 'on' : 'off'}"
          title={isConnected ? '内核已连接' : '内核未连接，延迟与切换不可用'}
        >
          <span class="conn-dot"></span>
          {isConnected ? '已连接' : '未连接'}
        </span>
      </div>
      <div class="toolbar-right">
        <!-- Search -->
        <div class="search-wrap">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="search-icon">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input bind:value={searchQuery} placeholder="搜索节点…" class="search-input" />
        </div>

        <!-- Sort toggle -->
        <div class="sort-seg">
          <button class="sort-btn {sortMode === 'delay' ? 'active' : ''}" onclick={() => (sortMode = 'delay')}>延迟</button>
          <button class="sort-btn {sortMode === 'name' ? 'active' : ''}" onclick={() => (sortMode = 'name')}>名称</button>
        </div>

        <!-- View mode toggle (Pro mode only; Lite always uses list) -->
        {#if !isLite}
          <div class="view-seg">
            <button
              class="view-btn {viewMode === 'list' ? 'active' : ''}"
              onclick={() => (viewMode = 'list')}
              title="列表视图"
              aria-label="列表视图"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/>
                <line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>
              </svg>
            </button>
            <button
              class="view-btn {viewMode === 'grid' ? 'active' : ''}"
              onclick={() => (viewMode = 'grid')}
              title="网格视图"
              aria-label="网格视图"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/>
                <rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/>
              </svg>
            </button>
          </div>
        {/if}

        <!-- Probe all -->
        <button class="probe-all-btn" onclick={handleProbeAll} disabled={probingAll || filteredNodes.length === 0}>
          {#if probingAll}
            <span class="probe-spinner">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" class="animate-spin">
                <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
              </svg>
            </span>
            <span class="probe-progress-text">{probeProgress.done}/{probeProgress.total}</span>
          {:else}
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
            </svg>
            <span>测速</span>
          {/if}
        </button>
      </div>
    </div>

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
            {#if !isConnected}
              内核未连接，且当前没有生效的代理配置。请先在「配置」页导入并启用一份配置。
            {:else}
              当前配置不包含节点，请在「配置」页导入一份包含 outbounds 的代理配置。
            {/if}
          </span>
          <button class="empty-clear" onclick={() => (store.activeTab = 'profiles')}>前往配置页</button>
        {:else}
          <span class="empty-text">暂无节点数据</span>
        {/if}
      </div>
    {:else if selectedGroup}
      <!-- ═══════ SINGLE GROUP VIEW ═══════ -->
      {#if viewMode === 'list'}
        <div class="node-list">
          {#each filteredNodes as node (node.id)}
            {@render nodeRow(node)}
          {/each}
        </div>
      {:else}
        <div class="node-grid">
          {#each filteredNodes as node (node.id)}
            {@render nodeCard(node)}
          {/each}
        </div>
      {/if}
    {:else}
      <!-- ═══════ ALL-NODES VIEW: collapsible group sections ═══════ -->
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
                    {@render nodeRow(node)}
                  {/each}
                </div>
              {:else}
                <div class="node-grid">
                  {#each section.nodes as node (node.id)}
                    {@render nodeCard(node)}
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
        <button class="error-dismiss" onclick={() => (lastError = null)} aria-label="关闭错误提示">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><line x1="2" y1="2" x2="8" y2="8"/><line x1="8" y1="2" x2="2" y2="8"/></svg>
        </button>
      </div>
    {/if}
  </div>
</div>

<!-- Portal popover: rendered outside .nodes-root so it escapes
     overflow:hidden clipping and the CSS transform containing block
     created by the @keyframes fade-in animation.  position:fixed
     anchors to the viewport because no ancestor here has a transform. -->
{#if popover.visible && popover.node}
  {@const pnode = popover.node}
  {@const pds = gradeDelay(pnode.delay, pnode.alive)}
  {@const spark = buildSparkline(popover.hist)}
  <div class="delay-portal-popover" style={popoverStyle()}>
    <span class="popover-title">历史延迟 ({popover.hist.length})</span>
    <svg class="sparkline" width="120" height="32" viewBox="0 0 120 32" preserveAspectRatio="none">
      <polyline points={sparklinePath(spark)} fill="none" stroke={pds.bar} stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round" />
      {#each spark.points as p}
        <circle cx={p.x.toFixed(1)} cy={p.y.toFixed(1)} r="1.5" fill={p.alive ? pds.bar : 'var(--destructive)'} />
      {/each}
    </svg>
    <span class="popover-stats">
      最新 {popover.hist[popover.hist.length - 1].delay > 0 ? popover.hist[popover.hist.length - 1].delay + 'ms' : '超时'}
      · 均 {meanDelay(popover.hist)}ms
    </span>
  </div>
{/if}

<!-- ═══════════════════════════════════════
     NODE ROW SNIPPET (list view)
     ═══════════════════════════════════════ -->
{#snippet nodeRow(node: ProxyNode)}
  {@const isActive = activeNodeId === node.tag}
  {@const isSwitching = switching === node.id}
  {@const isProbing = probingNodeIds.has(node.id)}
  {@const ds = gradeDelay(node.delay, node.alive)}
  {@const ps = getProtocolStyle(node.protocol)}
  {@const chips = getNodeChips(node)}

  <div class="node-row {isActive ? 'active' : ''}" role="listitem">
    <!-- Main click area: select node -->
   <button
     class="node-main"
     onclick={() => handleSelect(node)}
     disabled={switching !== null || !store.isActionOperable('policies.select') || !isNodeSelectable(node)}
   >
     <!-- Radio indicator -->
      <span class="node-radio {isActive ? 'on' : ''}">
        {#if isSwitching}
          <span class="node-spin-inline">⟳</span>
        {/if}
      </span>

      <!-- Node info -->
      <div class="node-info">
        <span class="node-name" class:active-name={isActive}>
          {#if node.emoji}<span class="node-emoji">{node.emoji}</span>{/if}
          {node.cleanName || node.name}
        </span>
        <div class="node-meta">
          <span class="proto-label" style="background: {ps.bg}; color: {ps.color};">{ps.label}</span>
          {#each chips as chip (chip.key)}
            <span class="attr-chip tone-{chip.tone}" title={chip.title}>{chip.label}</span>
          {/each}
          {#if node.domain && node.domain !== 'selected' && node.domain !== 'policy' && node.domain !== 'unavailable'}
            <span class="node-domain">{node.domain}</span>
          {/if}
          {#if ds.level === 'dead'}
            <span class="node-unavailable">离线</span>
          {/if}
        </div>
      </div>
    </button>

    <!-- Right side: delay + probe -->
    <div class="node-actions">
      <!-- Delay pill: hover triggers portal popover -->
      <div class="delay-wrap" role="presentation"
        onmouseenter={(e) => showPopover(e, node)}
        onmouseleave={hidePopover}
      >
        <span class="delay-pill" style="color: {ds.color}; background: {ds.bg};">
          {formatDelay(node.delay)}
          {#if node.delay > 0}<span class="delay-unit">ms</span>{/if}
          {#if ds.grade && ds.grade !== '—'}
            <span class="delay-grade">{ds.grade}</span>
          {/if}
        </span>
      </div>

      <!-- Delay bar -->
      <div class="delay-bar-track">
        <div class="delay-bar-fill" style="width: {delayBarWidth(node.delay)}; background: {ds.bar};"></div>
      </div>

      <!-- Probe button -->
      <button
        class="probe-btn"
        onclick={() => handleProbe(node)}
        disabled={isProbing || probingAll}
        title="测试延迟"
        aria-label="测试 {node.name} 延迟"
      >
        {#if isProbing}
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" class="animate-spin">
            <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
          </svg>
        {:else}
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
          </svg>
        {/if}
      </button>
    </div>
  </div>
{/snippet}

<!-- ═══════════════════════════════════════
     NODE CARD SNIPPET (grid view)
     ═══════════════════════════════════════ -->
{#snippet nodeCard(node: ProxyNode)}
  {@const isActive = activeNodeId === node.tag}
  {@const isSwitching = switching === node.id}
  {@const isProbing = probingNodeIds.has(node.id)}
  {@const ds = gradeDelay(node.delay, node.alive)}
  {@const ps = getProtocolStyle(node.protocol)}
  {@const chips = getNodeChips(node)}

  <div
    class="grid-card-wrap"
    role="listitem"
    onmouseenter={(e) => showPopover(e, node)}
    onmouseleave={hidePopover}
  >
    <button
     class="grid-card {isActive ? 'active' : ''} {isSwitching ? 'switching' : ''}"
     onclick={() => handleSelect(node)}
     disabled={switching !== null || !store.isActionOperable('policies.select') || !isNodeSelectable(node)}
   >
     <!-- Header: name + status -->
      <div class="grid-card-header">
        <span class="grid-card-name" class:active-name={isActive}>
          {#if node.emoji}<span class="node-emoji">{node.emoji}</span>{/if}
          {node.cleanName || node.name}
        </span>
        {#if isActive}
          <span class="grid-check" aria-hidden="true">
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="2,5 4,7 8,3"/>
            </svg>
          </span>
        {/if}
      </div>

      <!-- Protocol + attribute badges -->
      <div class="grid-badges">
        <span class="proto-label" style="background: {ps.bg}; color: {ps.color};">{ps.label}</span>
        {#each chips as chip (chip.key)}
          <span class="attr-chip tone-{chip.tone}" title={chip.title}>{chip.label}</span>
        {/each}
      </div>

      <!-- Bottom: delay -->
      <div class="grid-card-footer">
        {#if isSwitching}
          <span class="grid-spin">⟳</span>
        {:else}
          <span class="grid-delay" style="color: {ds.color};">
            {formatDelay(node.delay)}{#if node.delay > 0}<span class="grid-delay-unit">ms</span>{/if}
            {#if ds.grade && ds.grade !== '—'}<span class="grid-delay-grade">{ds.grade}</span>{/if}
          </span>
        {/if}
      </div>

      <!-- Bottom delay bar -->
      <div class="grid-bar-track">
        <div class="grid-bar-fill" style="width: {delayBarWidth(node.delay)}; background: {ds.bar};"></div>
      </div>
    </button>

    <!-- Independent probe button (P3-D): the whole card selects the node;
         this button only probes latency. Placed OUTSIDE the card <button>
         so its clicks never trigger selection (avoiding the old mis-fire). -->
    <button
      class="grid-probe-btn"
      onclick={() => handleProbe(node)}
      disabled={isProbing || probingAll}
      title="测试延迟"
      aria-label="测试 {node.name} 延迟"
    >
      {#if isProbing}<span class="grid-probe-spin">⟳</span>{:else}测速{/if}
    </button>
  </div>
{/snippet}

<style>
  /* ═══════════════════════════════════════
     ROOT LAYOUT
     ═══════════════════════════════════════ */
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

  /* ═══════════════════════════════════════
     GROUP SIDEBAR
     ═══════════════════════════════════════ */
  .group-sidebar {
    width: 168px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 10px 8px;
    border-right: 1px solid var(--border);
    background: var(--surface, rgba(0,0,0,0.015));
    overflow-y: auto;
  }

  :global(.dark) .group-sidebar {
    background: rgba(255,255,255,0.012);
  }

  .group-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 8px 8px;
  }

  .group-header-label {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    opacity: 0.55;
  }

  .group-header-count {
    font-size: 10px;
    font-weight: 600;
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    opacity: 0.45;
  }

  .group-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    width: 100%;
    padding: 7px 8px;
    border-radius: 6px;
    border: none;
    background: transparent;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s ease, box-shadow 0.12s ease;
  }

  .group-item:hover { background: var(--muted); }

  .group-item.active {
    background: rgba(99, 102, 241, 0.08);
    box-shadow: inset 2px 0 0 rgba(99, 102, 241, 0.5);
  }

  :global(.dark) .group-item.active {
    background: rgba(99, 102, 241, 0.1);
    box-shadow: inset 2px 0 0 rgba(165, 180, 252, 0.5);
  }

  .group-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .group-name-row {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .group-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-item.active .group-name { font-weight: 600; }

  .group-kind {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    opacity: 0.8;
    flex-shrink: 0;
  }

  .group-selected {
    font-size: 10.5px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    display: flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-selected-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #22C55E;
    flex-shrink: 0;
  }

  :global(.dark) .group-selected-dot { background: #4ADE80; }

  .group-count {
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-mono);
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--muted-foreground);
    flex-shrink: 0;
  }

  .group-item.active .group-count {
    background: rgba(99, 102, 241, 0.12);
    color: var(--accent-foreground);
  }

  .group-empty {
    font-size: 11px;
    color: var(--muted-foreground);
    padding: 16px 8px;
    text-align: center;
    opacity: 0.5;
  }

  /* ═══════════════════════════════════════
     NODE PANEL
     ═══════════════════════════════════════ */
  .node-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    position: relative;
  }

  /* ── Toolbar ── */
  .node-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .toolbar-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .toolbar-right {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .node-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--foreground);
  }

  .node-count {
    font-size: 11px;
    font-weight: 600;
    font-family: var(--font-mono);
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--muted-foreground);
  }

  /* Kernel connection badge */
  .conn-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 20px;
    padding: 0 8px;
    border-radius: 4px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .conn-badge.on {
    background: rgba(34, 197, 94, 0.10);
    color: #16A34A;
  }

  .conn-badge.off {
    background: rgba(245, 158, 11, 0.10);
    color: #D97706;
  }

  :global(.dark) .conn-badge.on {
    background: rgba(74, 222, 128, 0.10);
    color: #4ADE80;
  }

  :global(.dark) .conn-badge.off {
    background: rgba(251, 191, 36, 0.10);
    color: #FBBF24;
  }

  .conn-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
  }

  .conn-badge.on .conn-dot {
    box-shadow: 0 0 0 2px rgba(34, 197, 94, 0.18);
  }

  /* Search */
  .search-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-icon {
    position: absolute;
    left: 8px;
    color: var(--muted-foreground);
    opacity: 0.4;
    pointer-events: none;
  }

  .search-input {
    width: 130px;
    height: 28px;
    padding: 0 8px 0 26px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--foreground);
    font-size: 12px;
    outline: none;
    transition: border-color 0.15s ease, width 0.2s ease;
  }

  .search-input::placeholder {
    color: var(--muted-foreground);
    opacity: 0.5;
  }

  .search-input:focus {
    border-color: rgba(99, 102, 241, 0.4);
    width: 180px;
  }

  /* Sort / view segments */
  .sort-seg,
  .view-seg {
    display: inline-flex;
    gap: 1px;
    background: var(--segment-bg);
    padding: 2px;
    border-radius: 6px;
  }

  .sort-btn {
    height: 24px;
    padding: 0 9px;
    border-radius: 4px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.12s ease;
  }

  .sort-btn.active,
  .view-btn.active {
    background: var(--segment-active-bg);
    color: var(--foreground);
    font-weight: 600;
    box-shadow: var(--segment-active-shadow);
  }

  .view-btn {
    width: 28px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: all 0.12s ease;
  }

  /* Probe all button */
  .probe-all-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--foreground);
    font-size: 11.5px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.12s ease, border-color 0.12s ease;
    white-space: nowrap;
  }

  .probe-all-btn:hover:not(:disabled) {
    background: var(--surface);
    border-color: rgba(99, 102, 241, 0.2);
  }

  .probe-all-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .probe-progress-text {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }

  .probe-spinner {
    display: inline-flex;
    color: var(--accent-foreground);
  }

  /* ═══════════════════════════════════════
     EMPTY STATE
     ═══════════════════════════════════════ */
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

  /* ═══════════════════════════════════════
     COLLAPSIBLE SECTIONS (all-nodes view)
     ═══════════════════════════════════════ */
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

  /* ═══════════════════════════════════════
     LIST VIEW
     ═══════════════════════════════════════ */
  .node-list {
    padding: 4px 6px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .node-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px 0 0;
    border-radius: 8px;
    border: 1px solid transparent;
    transition: background 0.12s ease, border-color 0.12s ease;
    position: relative;
  }

  .node-row:hover { background: var(--muted); }

  .node-row.active {
    background: rgba(99, 102, 241, 0.05);
    border-color: rgba(99, 102, 241, 0.12);
  }

  :global(.dark) .node-row.active {
    background: rgba(99, 102, 241, 0.08);
    border-color: rgba(165, 180, 252, 0.12);
  }

  /* Select area (main click target) */
  .node-main {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    padding: 8px 4px 8px 6px;
    cursor: pointer;
    text-align: left;
  }

  .node-main:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Radio indicator */
  .node-radio {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--muted-foreground);
    opacity: 0.3;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
  }

  .node-radio.on {
    border-color: var(--accent-foreground);
    opacity: 1;
    background: var(--accent-foreground);
    box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.15);
  }

  :global(.dark) .node-radio.on {
    border-color: #A5B4FC;
    background: #A5B4FC;
    box-shadow: 0 0 0 2px rgba(165, 180, 252, 0.15);
  }

  .node-spin-inline {
    font-size: 10px;
    color: var(--muted-foreground);
    animation: spin 0.8s linear infinite;
  }

  .node-radio.on .node-spin-inline {
    color: white;
  }

  :global(.dark) .node-radio.on .node-spin-inline {
    color: #0F1014;
  }

  /* Node info column */
  .node-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .node-name {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.3;
  }

  .active-name {
    font-weight: 600;
  }

  .node-emoji {
    margin-right: 2px;
  }

  .node-meta {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
  }

  /* Protocol + attribute chips */
  .proto-label {
    display: inline-flex;
    align-items: center;
    height: 15px;
    padding: 0 5px;
    border-radius: 3px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .attr-chip {
    display: inline-flex;
    align-items: center;
    height: 15px;
    padding: 0 4px;
    border-radius: 3px;
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.02em;
    white-space: nowrap;
    flex-shrink: 0;
    border: 1px solid transparent;
  }

  .attr-chip.tone-success {
    background: rgba(34, 197, 94, 0.10);
    color: #16A34A;
    border-color: rgba(34, 197, 94, 0.18);
  }

  .attr-chip.tone-accent {
    background: rgba(99, 102, 241, 0.10);
    color: var(--accent-foreground);
    border-color: rgba(99, 102, 241, 0.18);
  }

  .attr-chip.tone-warning {
    background: rgba(245, 158, 11, 0.10);
    color: #D97706;
    border-color: rgba(245, 158, 11, 0.18);
  }

  .attr-chip.tone-muted {
    background: var(--muted);
    color: var(--muted-foreground);
  }

  :global(.dark) .attr-chip.tone-success {
    color: #4ADE80;
  }

  :global(.dark) .attr-chip.tone-warning {
    color: #FBBF24;
  }

  .node-domain {
    font-size: 10.5px;
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    opacity: 0.6;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 160px;
  }

  .node-unavailable {
    font-size: 10px;
    font-weight: 600;
    color: var(--destructive);
    opacity: 0.7;
  }

  /* Right-side actions */
  .node-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding-right: 2px;
  }

  /* Delay pill + hover popover */
  .delay-wrap {
    position: relative;
  }

  .delay-pill {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 22px;
    padding: 0 8px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 700;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    line-height: 1;
    white-space: nowrap;
    min-width: 52px;
    justify-content: center;
    cursor: default;
  }

  .delay-unit {
    font-size: 9px;
    font-weight: 600;
    opacity: 0.6;
    margin-left: 1px;
  }

  .delay-grade {
    font-size: 8.5px;
    font-weight: 700;
    opacity: 0.7;
    margin-left: 2px;
  }

  /* ── Portal delay-popover (position:fixed escapes overflow clipping) ── */
  .delay-portal-popover {
    position: fixed; /* overwritten by inline style for top/left/transform */
    z-index: 9999;
    min-width: 148px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--dialog-bg, #fff);
    border: 1px solid var(--border);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.16), 0 0 0 0.5px rgba(0, 0, 0, 0.05);
    display: flex;
    flex-direction: column;
    gap: 4px;
    pointer-events: none;
  }

  :global(.dark) .delay-portal-popover {
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5), 0 0 0 0.5px rgba(255, 255, 255, 0.06);
  }

  .popover-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .sparkline {
    display: block;
    width: 100%;
    overflow: visible;
  }

  .popover-stats {
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--muted-foreground);
    font-variant-numeric: tabular-nums;
  }

  /* Delay bar */
  .delay-bar-track {
    width: 48px;
    height: 3px;
    border-radius: 1.5px;
    background: var(--muted);
    overflow: hidden;
    flex-shrink: 0;
  }

  .delay-bar-fill {
    height: 100%;
    border-radius: 1.5px;
    transition: width 0.3s ease;
  }

  /* Probe button */
  .probe-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }

  .probe-btn:hover:not(:disabled) {
    background: var(--muted);
    color: var(--foreground);
  }

  .probe-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* ═══════════════════════════════════════
     GRID VIEW
     ═══════════════════════════════════════ */
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

  .grid-card-wrap {
    position: relative;
  }

  .grid-card {
    display: flex;
    flex-direction: column;
    gap: 5px;
    width: 100%;
    padding: 12px 13px 14px;
    background: var(--card);
    border: 1.5px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s ease, border-color 0.15s ease, box-shadow 0.15s ease;
    position: relative;
    overflow: hidden;
  }

  .grid-card:hover {
    background: var(--surface);
    border-color: rgba(128, 128, 160, 0.18);
  }

  .grid-card.active {
    background: rgba(99, 102, 241, 0.06);
    border-color: rgba(99, 102, 241, 0.3);
    box-shadow: 0 0 0 1px rgba(99, 102, 241, 0.08);
  }

  :global(.dark) .grid-card.active {
    background: rgba(99, 102, 241, 0.08);
    border-color: rgba(165, 180, 252, 0.25);
    box-shadow: 0 0 0 1px rgba(165, 180, 252, 0.08);
  }

  .grid-card.switching {
    opacity: 0.5;
    pointer-events: none;
  }

  .grid-card:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .grid-card-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 4px;
  }

  .grid-card-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--foreground);
    line-height: 1.25;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    flex: 1;
    min-width: 0;
  }

  .active-name {
    color: var(--accent-foreground);
  }

  :global(.dark) .active-name {
    color: #A5B4FC;
  }

  .grid-check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: rgba(99, 102, 241, 0.18);
    color: var(--accent-foreground);
    flex-shrink: 0;
    margin-top: 1px;
  }

  :global(.dark) .grid-check {
    background: rgba(165, 180, 252, 0.18);
    color: #A5B4FC;
  }

  .grid-badges {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-wrap: wrap;
    min-height: 15px;
  }

  .grid-card-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    margin-top: auto;
    min-height: 18px;
  }

  .grid-delay {
    display: inline-flex;
    align-items: baseline;
    font-size: 13px;
    font-weight: 700;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }

  .grid-delay-unit {
    font-size: 9px;
    font-weight: 600;
    opacity: 0.5;
    margin-left: 1px;
  }

  .grid-delay-grade {
    font-size: 8.5px;
    font-weight: 700;
    opacity: 0.6;
    margin-left: 3px;
  }

  .grid-spin {
    font-size: 12px;
    color: var(--muted-foreground);
    animation: spin 0.8s linear infinite;
  }

  /* Independent probe button on the card (P3-D) — top-right corner, a
     separate click target from the card's "select node" action so probing
     never accidentally switches the active node. */
  .grid-probe-btn {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 34px;
    height: 20px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--muted-foreground);
    font-size: 10px;
    font-weight: 600;
    cursor: pointer;
    z-index: 2;
    transition: background 0.13s ease, color 0.13s ease, border-color 0.13s ease;
  }
  .grid-probe-btn:hover:not(:disabled) {
    background: var(--accent, var(--muted));
    color: var(--foreground);
    border-color: var(--primary);
  }
  .grid-probe-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .grid-probe-spin {
    display: inline-block;
    animation: spin 0.8s linear infinite;
  }

  /* Grid delay bar (bottom strip) */
  .grid-bar-track {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: 2.5px;
    background: var(--muted);
    opacity: 0.25;
    border-radius: 0 0 8px 8px;
    overflow: hidden;
  }

  .grid-bar-fill {
    height: 100%;
    border-radius: 0 0 8px 8px;
    transition: width 0.3s ease;
  }

  .grid-card:hover .grid-bar-track,
  .grid-card.active .grid-bar-track {
    opacity: 0.5;
  }

  /* ═══════════════════════════════════════
     ERROR BAR
     ═══════════════════════════════════════ */
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

  /* ═══════════════════════════════════════
     ANIMATIONS
     ═══════════════════════════════════════ */
  .animate-spin { animation: spin 0.8s linear infinite; }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  /* ═══════════════════════════════════════
     RESPONSIVE
     ═══════════════════════════════════════ */
  @media (max-width: 700px) {
    .group-sidebar {
      width: 120px;
      padding: 8px 6px;
    }
    .search-input { width: 100px; }
    .search-input:focus { width: 140px; }
    .delay-bar-track { display: none; }
    .node-grid {
      grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    }
  }
</style>
