<script lang="ts">
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { store } from '$lib/services/store.svelte';
  import { selectPolicy, guiClientProbeNode, guiClientProbeStart } from '$lib/services/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import type { ProxyNode } from '$lib/types/protocol';

  // ── View state ──
  type ViewMode = 'list' | 'grid';
  let viewMode = $state<ViewMode>(store.uiMode === 'lite' ? 'list' : 'grid');
  let isLite = $derived(store.uiMode === 'lite');
  let searchQuery = $state('');
  let sortMode = $state<'delay' | 'name'>('delay');
  let selectedGroup = $state<string | null>(null);
  let switching = $state<string | null>(null);
  let probing = $state<string | null>(null);
  let probingAll = $state(false);
  let probeProgress = $state({ done: 0, total: 0 });
  let lastError = $state<string | null>(null);
  let probeResults = $state<Record<string, { delayMs?: number; alive?: boolean }>>({});

  // ── Delay colors (Clash-style thresholds) ──
  function getDelayStyle(delay: number): { color: string; bg: string; bar: string } {
    if (delay <= 0) return { color: 'var(--muted-foreground)', bg: 'var(--muted)', bar: 'transparent' };
    if (delay < 200) return { color: '#16A34A', bg: 'rgba(34,197,94,0.10)', bar: '#22C55E' };
    if (delay < 500) return { color: '#D97706', bg: 'rgba(245,158,11,0.10)', bar: '#F59E0B' };
    return { color: '#DC2626', bg: 'rgba(239,68,68,0.10)', bar: '#EF4444' };
  }

  function getDelayColor(delay: number): string {
    if (delay <= 0) return 'var(--muted-foreground)';
    if (delay < 200) return '#22C55E';
    if (delay < 500) return '#FBBF24';
    return '#F87171';
  }

  // ── Data derivation ──
  const groups = $derived(guiState.policyGroups);

  const groupNodes = $derived.by((): ProxyNode[] => {
    if (groups.length === 0) return [];
    const seen = new Set<string>();
    const nodes: ProxyNode[] = [];
    for (const group of groups) {
      for (const outbound of group.outbounds) {
        const key = outbound.tag.toLowerCase();
        if (seen.has(key)) continue;
        seen.add(key);
        const probe = probeResults[outbound.tag];
        nodes.push({
          id: `${group.name}:${outbound.tag}`,
          name: outbound.tag,
          protocol: outbound.type || 'Zero',
          delay: probe?.delayMs ?? outbound.delayMs ?? 0,
          domain: group.selected === outbound.tag ? 'selected' : (probe?.alive ?? outbound.alive) === false ? 'unavailable' : group.name,
        });
      }
    }
    return nodes;
  });

  const allNodes = $derived(groupNodes.length > 0 ? groupNodes : overviewData.proxyNodes);

  const filteredNodes = $derived.by(() => {
    let nodes = allNodes;
    if (selectedGroup) {
      const group = groups.find(g => g.name === selectedGroup);
      if (group) {
        const tags = new Set(group.outbounds.map(o => o.tag));
        nodes = nodes.filter(n => tags.has(n.name));
      }
    }
    if (searchQuery.trim()) {
      const q = searchQuery.trim().toLowerCase();
      nodes = nodes.filter(n => n.name.toLowerCase().includes(q) || n.protocol.toLowerCase().includes(q));
    }
    return [...nodes].sort((a, b) => {
      if (sortMode === 'delay') {
        if (a.delay === 0 && b.delay > 0) return 1;
        if (b.delay === 0 && a.delay > 0) return -1;
        return a.delay - b.delay;
      }
      return a.name.localeCompare(b.name);
    });
  });

  const activeNodeId = $derived(
    groups.flatMap(g => g.outbounds).find(o => o.tag === groups.find(g => g.selected)?.selected)?.tag
    ?? groups.find(g => g.selected)?.selected
  );

  // ── Group kind label ──
  function groupKindLabel(kind?: string): string {
    if (!kind) return '';
    const k = kind.toLowerCase();
    if (k.includes('selector')) return 'Selector';
    if (k.includes('urltest') || k.includes('url_test')) return 'URLTest';
    if (k.includes('fallback')) return 'Fallback';
    if (k.includes('loadbalance') || k.includes('load_balance')) return 'LB';
    return kind;
  }

  function groupKindColor(kind?: string): string {
    if (!kind) return '';
    const k = kind.toLowerCase();
    if (k.includes('selector')) return '#6366F1';
    if (k.includes('urltest') || k.includes('url_test')) return '#F59E0B';
    if (k.includes('fallback')) return '#10B981';
    if (k.includes('loadbalance') || k.includes('load_balance')) return '#EC4899';
    return '';
  }

  // ── Actions ──
  async function handleSelect(node: ProxyNode) {
    if (switching) return;
    switching = node.id;
    lastError = null;
    try {
      const policyTag = selectedGroup
        ?? groups.find(g => g.outbounds.some(o => o.tag === node.name))?.name
        ?? 'proxy';
      const result = await selectPolicy(policyTag, node.name);
      if ((result as any).error) {
        lastError = (result as any).error.message;
      }
    } catch (e) {
      lastError = String(e);
    } finally {
      switching = null;
    }
  }

  async function handleProbe(node: ProxyNode) {
    if (probing) return;
    probing = node.id;
    lastError = null;
    try {
      const result = await guiClientProbeNode(node.name);
      probeResults = {
        ...probeResults,
        [node.name]: {
          delayMs: result.latencyMs,
          alive: result.reachable,
        },
      };
      await guiState.refreshPolicyGroups();
    } catch (e) {
      lastError = String(e);
    } finally {
      probing = null;
    }
  }

  async function handleProbeAll() {
    if (probingAll) return;
    probingAll = true;
    probeProgress = { done: 0, total: filteredNodes.length };
    lastError = null;

    const tags = filteredNodes.map(n => n.name);

    // Listen for progressive results from the backend
    let unlistenResult: UnlistenFn | undefined;
    let unlistenComplete: UnlistenFn | undefined;

    try {
      unlistenResult = await listen<{ targetTag: string; reachable: boolean; latencyMs?: number }>('probe:result', (event) => {
        const { targetTag, reachable, latencyMs } = event.payload;
        probeResults = {
          ...probeResults,
          [targetTag]: { delayMs: latencyMs, alive: reachable },
        };
      });

      unlistenComplete = await listen<{ total: number; reachable: number; failed: number }>('probe:complete', async () => {
        await guiState.refreshPolicyGroups();
        probingAll = false;
      });

      // Start batch — backend handles concurrency and emits events
      await guiClientProbeStart(tags);
    } catch (e) {
      lastError = String(e);
      probingAll = false;
    } finally {
      unlistenResult?.();
      unlistenComplete?.();
    }
  }

  function formatDelay(delay: number): string {
    if (delay <= 0) return '—';
    if (delay < 1000) return `${delay}`;
    return `${(delay / 1000).toFixed(1)}s`;
  }

  // Delay bar width as percentage (max 1000ms = 100%)
  function delayBarWidth(delay: number): string {
    if (delay <= 0) return '0%';
    return `${Math.min(100, (delay / 1000) * 100)}%`;
  }

  function selectFirstGroup() {
    if (groups.length > 0 && !selectedGroup) {
      selectedGroup = groups[0].name;
    }
  }

  $effect(() => {
    selectFirstGroup();
  });
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
      onclick={() => selectedGroup = null}
    >
      <div class="group-info">
        <span class="group-name">全部节点</span>
      </div>
      <span class="group-count">{allNodes.length}</span>
    </button>

    {#each groups as group}
      <button
        class="group-item {selectedGroup === group.name ? 'active' : ''}"
        onclick={() => selectedGroup = group.name}
      >
        <div class="group-info">
          <div class="group-name-row">
            <span class="group-name truncate">{group.name}</span>
            {#if group.kind}
              <span class="group-kind" style="color: {groupKindColor(group.kind)}">{groupKindLabel(group.kind)}</span>
            {/if}
          </div>
          {#if group.selected}
            <span class="group-selected truncate">
              <span class="group-selected-dot"></span>
              {group.selected}
            </span>
          {/if}
        </div>
        <span class="group-count">{group.outbounds.length}</span>
      </button>
    {/each}

    {#if groups.length === 0}
      <div class="group-empty">等待策略数据…</div>
    {/if}
  </aside>

  <!-- Right: Node panel -->
  <div class="node-panel">
    <!-- Toolbar -->
    <div class="node-toolbar">
      <div class="toolbar-left">
        <span class="node-title">{selectedGroup || '全部节点'}</span>
        <span class="node-count">{filteredNodes.length}</span>
      </div>
      <div class="toolbar-right">
        <!-- Search -->
        <div class="search-wrap">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="search-icon">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input
            bind:value={searchQuery}
            placeholder="搜索节点…"
            class="search-input"
          />
        </div>

        <!-- Sort toggle -->
        <div class="sort-seg">
          <button
            class="sort-btn {sortMode === 'delay' ? 'active' : ''}"
            onclick={() => sortMode = 'delay'}
          >延迟</button>
          <button
            class="sort-btn {sortMode === 'name' ? 'active' : ''}"
            onclick={() => sortMode = 'name'}
          >名称</button>
        </div>

        <!-- View mode toggle (Pro mode only; Lite always uses list) -->
        {#if !isLite}
        <div class="view-seg">
          <button
            class="view-btn {viewMode === 'list' ? 'active' : ''}"
            onclick={() => viewMode = 'list'}
            title="列表视图"
            aria-label="列表视图"
          >
            <!-- List icon -->
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/>
              <line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>
            </svg>
          </button>
          <button
            class="view-btn {viewMode === 'grid' ? 'active' : ''}"
            onclick={() => viewMode = 'grid'}
            title="网格视图"
            aria-label="网格视图"
          >
            <!-- Grid icon -->
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/>
              <rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/>
            </svg>
          </button>
        </div>
        {/if}

        <!-- Probe all -->
        <button
          class="probe-all-btn"
          onclick={handleProbeAll}
          disabled={probingAll || filteredNodes.length === 0}
        >
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
        <span class="empty-text">{searchQuery ? '无匹配节点' : '暂无节点数据'}</span>
        {#if searchQuery}
          <button class="empty-clear" onclick={() => searchQuery = ''}>清除搜索</button>
        {/if}
      </div>
    {:else if viewMode === 'list'}
      <!-- ═══════ LIST VIEW ═══════ -->
      <div class="node-list">
        {#each filteredNodes as node (node.id)}
          {@const isActive = activeNodeId === node.name}
          {@const isSwitching = switching === node.id}
          {@const isProbing = probing === node.id}
          {@const ds = getDelayStyle(node.delay)}

          <div class="node-row {isActive ? 'active' : ''}">
            <!-- Main click area: select node -->
            <button
              class="node-main"
              onclick={() => handleSelect(node)}
              disabled={switching !== null || !store.isActionOperable('policies.select')}
            >
              <!-- Radio indicator -->
              <span class="node-radio {isActive ? 'on' : ''}">
                {#if isSwitching}
                  <span class="node-spin-inline">⟳</span>
                {/if}
              </span>

              <!-- Node info -->
              <div class="node-info">
                <span class="node-name" class:active-name={isActive}>{node.name}</span>
                <div class="node-meta">
                  <span class="proto-label">{node.protocol}</span>
                  {#if node.domain && node.domain !== 'selected' && node.domain !== 'policy' && node.domain !== 'unavailable'}
                    <span class="node-domain">{node.domain}</span>
                  {/if}
                  {#if node.domain === 'unavailable'}
                    <span class="node-unavailable">离线</span>
                  {/if}
                </div>
              </div>
            </button>

            <!-- Right side: delay + probe -->
            <div class="node-actions">
              <!-- Delay pill -->
              <span class="delay-pill" style="color: {getDelayColor(node.delay)}; background: {ds.bg};">
                {formatDelay(node.delay)}
                {#if node.delay > 0}
                  <span class="delay-unit">ms</span>
                {/if}
              </span>

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
        {/each}
      </div>
    {:else}
      <!-- ═══════ GRID VIEW ═══════ -->
      <div class="node-grid">
        {#each filteredNodes as node (node.id)}
          {@const isActive = activeNodeId === node.name}
          {@const isSwitching = switching === node.id}
          {@const ds = getDelayStyle(node.delay)}

          <button
            class="grid-card {isActive ? 'active' : ''} {isSwitching ? 'switching' : ''}"
            onclick={() => handleSelect(node)}
            disabled={switching !== null || !store.isActionOperable('policies.select')}
          >
            <!-- Header: name + status -->
            <div class="grid-card-header">
              <span class="grid-card-name" class:active-name={isActive}>{node.name}</span>
              {#if isActive}
                <span class="grid-check" aria-hidden="true">
                  <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="2,5 4,7 8,3"/>
                  </svg>
                </span>
              {/if}
            </div>

            <!-- Protocol label -->
            <span class="proto-label grid-proto">
              {node.protocol}
            </span>

            <!-- Bottom: delay -->
            <div class="grid-card-footer">
              {#if isSwitching}
                <span class="grid-spin">⟳</span>
              {:else}
                <span class="grid-delay" style="color: {getDelayColor(node.delay)};">
                  {formatDelay(node.delay)}{#if node.delay > 0}<span class="grid-delay-unit">ms</span>{/if}
                </span>
              {/if}
            </div>

            <!-- Bottom delay bar -->
            <div class="grid-bar-track">
              <div class="grid-bar-fill" style="width: {delayBarWidth(node.delay)}; background: {ds.bar};"></div>
            </div>
          </button>
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
        <button class="error-dismiss" onclick={() => lastError = null} aria-label="关闭错误提示">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><line x1="2" y1="2" x2="8" y2="8"/><line x1="8" y1="2" x2="2" y2="8"/></svg>
        </button>
      </div>
    {/if}
  </div>
</div>

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

  /* Sort segment */
  .sort-seg {
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

  .sort-btn.active {
    background: var(--segment-active-bg);
    color: var(--foreground);
    font-weight: 600;
    box-shadow: var(--segment-active-shadow);
  }

  /* View mode segment */
  .view-seg {
    display: inline-flex;
    gap: 1px;
    background: var(--segment-bg);
    padding: 2px;
    border-radius: 6px;
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

  .view-btn.active {
    background: var(--segment-active-bg);
    color: var(--foreground);
    box-shadow: var(--segment-active-shadow);
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
     LIST VIEW
     ═══════════════════════════════════════ */
  .node-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 6px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-height: 0;
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

  .node-meta {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  /* Protocol label */
  .proto-label {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.02em;
    white-space: nowrap;
    flex-shrink: 0;
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

  /* Delay pill */
  .delay-pill {
    display: inline-flex;
    align-items: baseline;
    gap: 1px;
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
    align-items: center;
  }

  .delay-unit {
    font-size: 9px;
    font-weight: 600;
    opacity: 0.6;
    margin-left: 1px;
  }

  /* Delay bar (horizontal, Clash-style) */
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
    padding: 10px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(135px, 1fr));
    gap: 8px;
    align-content: start;
    min-height: 0;
  }

  .grid-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 11px 12px;
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

  .grid-proto {
    font-size: 10px;
    align-self: flex-start;
    margin-top: 1px;
    opacity: 0.6;
  }

  .grid-card-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    margin-top: auto;
    min-height: 18px;
  }

  .grid-delay {
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

  .grid-spin {
    font-size: 12px;
    color: var(--muted-foreground);
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
