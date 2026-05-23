<script lang="ts">
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { store } from '$lib/services/store.svelte';
  import { selectPolicy, probePolicy } from '$lib/services/core';
  import type { ProxyNode } from '$lib/types/protocol';
  import type { PolicyGroup } from '$lib/types/gui-api';

  let searchQuery = $state('');
  let sortMode = $state<'delay' | 'name'>('delay');
  let selectedGroup = $state<string | null>(null);
  let switching = $state<string | null>(null);
  let probing = $state<string | null>(null);
  let lastError = $state<string | null>(null);

  const groups = $derived(guiState.policyGroups);
  const allNodes = $derived(overviewData.proxyNodes);

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

  function getDelayClass(delay: number): string {
    if (delay <= 0) return '';
    if (delay < 100) return 'text-emerald-500';
    if (delay < 200) return 'text-amber-500';
    return 'text-red-500';
  }

  function formatDelay(delay: number): string {
    if (delay <= 0) return '—';
    if (delay < 1000) return `${delay} ms`;
    return `${(delay / 1000).toFixed(1)} s`;
  }

  async function handleSelect(node: ProxyNode) {
    if (switching) return;
    switching = node.id;
    lastError = null;
    try {
      const result = await selectPolicy('proxy', node.name);
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
    try {
      await probePolicy(node.name);
      await overviewData.refreshPolicyNodes();
    } catch {
      // Probe failure is non-blocking
    } finally {
      probing = null;
    }
  }

  async function handleProbeAll() {
    for (const node of filteredNodes) {
      if (node.delay > 0) continue;
      await handleProbe(node);
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
</script>

<div class="nodes-root animate-fade-in">
  <!-- Left: Policy group sidebar -->
  <aside class="group-sidebar">
    <div class="group-header">策略组</div>
    <button
      class="group-item {!selectedGroup ? 'active' : ''}"
      onclick={() => selectedGroup = null}
    >
      <span class="group-name">全部节点</span>
      <span class="group-count">{allNodes.length}</span>
    </button>
    {#each groups as group}
      <button
        class="group-item {selectedGroup === group.name ? 'active' : ''}"
        onclick={() => selectedGroup = group.name}
      >
        <div class="group-info">
          <span class="group-name truncate">{group.name}</span>
          {#if group.selected}
            <span class="group-selected truncate">{group.selected}</span>
          {/if}
        </div>
        <span class="group-count">{group.outbounds.length}</span>
      </button>
    {/each}
    {#if groups.length === 0}
      <div class="group-empty">等待策略数据…</div>
    {/if}
  </aside>

  <!-- Right: Node list -->
  <div class="node-panel">
    <!-- Toolbar -->
    <div class="node-toolbar">
      <div class="flex items-center gap-2">
        <span class="node-title">
          {selectedGroup || '全部节点'}
        </span>
        <span class="node-count">{filteredNodes.length}</span>
      </div>
      <div class="flex items-center gap-2">
        <div class="search-wrap">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="search-icon">
            <circle cx="5" cy="5" r="3.5"/><line x1="8" y1="8" x2="10.5" y2="10.5"/>
          </svg>
          <input
            bind:value={searchQuery}
            placeholder="搜索节点…"
            class="search-input"
          />
        </div>
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
        <button class="action-btn" onclick={handleProbeAll} disabled={probing !== null}>
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M10 6A4 4 0 1 1 6 2M6 2L9 2L9 5"/>
          </svg>
          测速
        </button>
      </div>
    </div>

    <!-- List -->
    {#if filteredNodes.length === 0}
      <div class="node-empty">
        {#if searchQuery}
          无匹配结果
        {:else}
          暂无节点数据
        {/if}
      </div>
    {:else}
      <div class="node-list">
        {#each filteredNodes as node (node.id)}
          <div class="node-row {activeNodeId === node.name ? 'active' : ''}">
            <!-- Select area -->
            <button
              class="node-main"
              onclick={() => handleSelect(node)}
              disabled={switching !== null || !store.isActionOperable('policies.select')}
            >
              <span class="node-indicator {activeNodeId === node.name ? 'on' : ''}"></span>
              <div class="node-info">
                <span class="node-name">{node.name}</span>
                <span class="node-meta">
                  <span class="node-protocol">{node.protocol}</span>
                  {#if node.domain && node.domain !== 'selected' && node.domain !== 'policy' && node.domain !== 'unavailable'}
                    <span class="node-domain">{node.domain}</span>
                  {/if}
                </span>
              </div>
              {#if switching === node.id}
                <span class="node-spin">⟳</span>
              {/if}
            </button>

            <!-- Delay + probe -->
            <div class="node-actions">
              <span class="node-delay {getDelayClass(node.delay)}">
                {formatDelay(node.delay)}
              </span>
              <button
                class="probe-btn"
                onclick={() => handleProbe(node)}
                disabled={probing === node.id}
                title="测试延迟"
              >
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" class="{probing === node.id ? 'animate-spin' : ''}">
                  <path d="M10 6A4 4 0 1 1 6 2M6 2L9 2L9 5"/>
                </svg>
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    {#if lastError}
      <div class="node-error">{lastError}</div>
    {/if}
  </div>
</div>

<style>
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

  /* ---- Group sidebar ---- */
  .group-sidebar {
    width: 160px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 12px 8px;
    border-right: 1px solid var(--border);
    background: var(--surface, rgba(0,0,0,0.015));
    overflow-y: auto;
  }

  :global(.dark) .group-sidebar {
    background: rgba(255,255,255,0.012);
  }

  .group-header {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    padding: 2px 8px 8px;
    opacity: 0.6;
  }

  .group-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    width: 100%;
    padding: 6px 8px;
    border-radius: 6px;
    border: none;
    background: transparent;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s ease;
  }

  .group-item:hover { background: var(--muted); }
  .group-item.active {
    background: var(--primary);
    color: var(--primary-foreground);
  }

  .group-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }

  .group-name {
    font-size: 12px;
    font-weight: 500;
  }

  .group-item.active .group-name { font-weight: 600; }

  .group-selected {
    font-size: 10.5px;
    opacity: 0.7;
    font-family: var(--font-mono);
  }

  .group-count {
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-mono);
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--muted);
    flex-shrink: 0;
  }

  .group-item.active .group-count {
    background: rgba(255,255,255,0.2);
  }

  .group-empty {
    font-size: 11px;
    color: var(--muted-foreground);
    padding: 12px 8px;
    text-align: center;
    opacity: 0.5;
  }

  /* ---- Node panel ---- */
  .node-panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

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
    width: 140px;
    height: 28px;
    padding: 0 8px 0 26px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--foreground);
    font-size: 12px;
    outline: none;
    transition: border-color 0.12s ease, width 0.15s ease;
  }

  .search-input:focus {
    border-color: rgba(99, 102, 241, 0.4);
    width: 180px;
  }

  .sort-seg {
    display: inline-flex;
    gap: 1px;
    background: var(--muted);
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
    background: var(--card);
    color: var(--foreground);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0,0,0,0.06);
  }

  .action-btn {
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
    transition: background 0.12s ease;
    white-space: nowrap;
  }

  .action-btn:hover:not(:disabled) { background: var(--surface); }
  .action-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  /* ---- Node list ---- */
  .node-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    color: var(--muted-foreground);
    opacity: 0.5;
  }

  .node-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-height: 0;
  }

  .node-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid transparent;
    transition: background 0.12s ease, border-color 0.12s ease;
  }

  .node-row:hover { background: var(--muted); }
  .node-row.active {
    background: rgba(99, 102, 241, 0.06);
    border-color: rgba(99, 102, 241, 0.15);
  }

  .node-main {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    padding: 0;
    cursor: pointer;
    text-align: left;
  }

  .node-main:disabled { opacity: 0.5; cursor: not-allowed; }

  .node-indicator {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted-foreground);
    opacity: 0.25;
    flex-shrink: 0;
    transition: all 0.15s ease;
  }

  .node-indicator.on {
    background: #22C55E;
    opacity: 1;
  }

  .node-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .node-name {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .node-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--muted-foreground);
    opacity: 0.7;
  }

  .node-protocol {
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .node-domain {
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 180px;
  }

  .node-spin {
    font-size: 14px;
    color: var(--muted-foreground);
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }

  .node-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .node-delay {
    font-size: 12px;
    font-weight: 700;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    min-width: 48px;
    text-align: right;
  }

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

  .probe-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .node-error {
    margin: 4px;
    padding: 7px 10px;
    background: rgba(239, 68, 68, 0.06);
    border: 1px solid rgba(239, 68, 68, 0.14);
    border-radius: 6px;
    font-size: 11px;
    color: var(--destructive);
    flex-shrink: 0;
  }

  .animate-spin { animation: spin 0.8s linear infinite; }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  @media (max-width: 700px) {
    .group-sidebar { width: 120px; padding: 10px 6px; }
    .search-input { width: 100px; }
    .search-input:focus { width: 140px; }
  }
</style>
