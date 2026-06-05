<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import type { ProxyNode } from '$lib/types/protocol';
  import { selectPolicy } from '$lib/services/core';

  const { nodes, initialSelected = '' }: {
    nodes: ProxyNode[];
    initialSelected?: string;
  } = $props();

  let selected = $state('');

  $effect(() => {
    selected = initialSelected;
  });

  let switching = $state<string | null>(null);
  let lastError = $state<string | null>(null);

  async function handleSelect(node: ProxyNode) {
    if (switching) return;
    switching = node.id;
    lastError = null;

    try {
      const result = await selectPolicy('proxy', node.name);
      if (result.error) {
        lastError = result.error.message;
      } else {
        selected = node.id;
      }
    } catch (e) {
      lastError = String(e);
    } finally {
      switching = null;
    }
  }

  function getDelayColor(delay: number): string {
    if (delay <= 0) return 'var(--muted-foreground)';
    if (delay < 200) return '#22C55E';
    if (delay < 500) return '#F59E0B';
    return '#EF4444';
  }

  function getDelayBg(delay: number): string {
    if (delay <= 0) return 'transparent';
    if (delay < 200) return 'rgba(34,197,94,0.10)';
    if (delay < 500) return 'rgba(245,158,11,0.10)';
    return 'rgba(239,68,68,0.10)';
  }

  function getProtoStyle(protocol: string): { bg: string; color: string } {
    const key = protocol.toLowerCase();
    const map: Record<string, { bg: string; color: string }> = {
      shadowsocks: { bg: 'rgba(139,92,246,0.12)', color: '#8B5CF6' },
      vmess:       { bg: 'rgba(59,130,246,0.12)',  color: '#3B82F6' },
      vless:       { bg: 'rgba(16,185,129,0.12)',  color: '#10B981' },
      trojan:      { bg: 'rgba(239,68,68,0.12)',   color: '#EF4444' },
    };
    for (const [k, v] of Object.entries(map)) {
      if (key.includes(k)) return v;
    }
    return { bg: 'rgba(107,114,128,0.10)', color: '#6B7280' };
  }
</script>

<div class="ns-root desk-card h-full flex flex-col overflow-hidden">
  <!-- Header -->
  <div class="ns-header">
    <span class="ns-label">核心策略出口</span>
    <span class="ns-badge">Radio</span>
  </div>

  <!-- List -->
  {#if nodes.length === 0}
    <div class="flex-1 flex items-center justify-center text-xs text-muted-foreground">等待节点数据...</div>
  {:else}
    <div class="ns-list">
      {#each nodes as node}
        {@const isActive = selected === node.id}
        {@const ps = getProtoStyle(node.protocol)}

        <button
          class="ns-row {isActive ? 'active' : ''} {switching === node.id ? 'switching' : ''}"
          onclick={() => handleSelect(node)}
          disabled={switching !== null || !store.isActionOperable('policies.select')}
        >
          <!-- Radio indicator -->
          <span class="ns-radio {isActive ? 'on' : ''}">
            {#if switching === node.id}
              <span class="ns-spin-dot">⟳</span>
            {/if}
          </span>

          <!-- Name + proto -->
          <div class="ns-info">
            <span class="ns-name truncate">{node.name}</span>
            <span class="ns-proto" style="background: {ps.bg}; color: {ps.color};">{node.protocol}</span>
          </div>

          <!-- Delay pill -->
          <span class="ns-delay" style="color: {getDelayColor(node.delay)}; background: {getDelayBg(node.delay)};">
            {node.delay > 0 ? node.delay : '—'}
            {#if node.delay > 0}
              <span style="font-size:9px;opacity:0.55;font-weight:600;">ms</span>
            {/if}
          </span>
        </button>
      {/each}
    </div>
  {/if}

  <!-- Error -->
  {#if lastError}
    <div class="ns-error" title={lastError}>{lastError}</div>
  {/if}
</div>

<style>
  .ns-root {
    display: flex;
    flex-direction: column;
  }

  .ns-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 9px 12px 7px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .ns-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--muted-foreground);
  }

  .ns-badge {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    background: var(--muted);
    padding: 2px 7px;
    border-radius: 4px;
  }

  .ns-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-height: 0;
  }

  .ns-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    cursor: pointer;
    transition: background 0.12s ease, border-color 0.12s ease;
    text-align: left;
  }

  .ns-row:hover {
    background: var(--muted);
  }

  .ns-row.active {
    background: rgba(99, 102, 241, 0.06);
    border-color: rgba(99, 102, 241, 0.15);
  }

  :global(.dark) .ns-row.active {
    background: rgba(99, 102, 241, 0.08);
    border-color: rgba(165, 180, 252, 0.15);
  }

  .ns-row.switching {
    opacity: 0.55;
    pointer-events: none;
  }

  /* Radio indicator */
  .ns-radio {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--muted-foreground);
    opacity: 0.3;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
  }

  .ns-radio.on {
    border-color: var(--accent-foreground);
    opacity: 1;
    background: var(--accent-foreground);
  }

  :global(.dark) .ns-radio.on {
    border-color: #A5B4FC;
    background: #A5B4FC;
  }

  .ns-spin-dot {
    font-size: 8px;
    color: var(--muted-foreground);
    animation: spin 0.8s linear infinite;
  }

  .ns-radio.on .ns-spin-dot {
    color: white;
  }

  :global(.dark) .ns-radio.on .ns-spin-dot {
    color: #0F1014;
  }

  .ns-info {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .ns-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--foreground);
    flex: 1;
    min-width: 0;
    transition: color 0.12s ease;
  }

  .ns-row.active .ns-name { font-weight: 600; }

  .ns-proto {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 5px;
    border-radius: 3px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    flex-shrink: 0;
  }

  .ns-delay {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    font-size: 12px;
    font-family: var(--font-mono);
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 4px;
    letter-spacing: -0.02em;
    line-height: 1;
    flex-shrink: 0;
  }

  .ns-error {
    margin: 4px;
    padding: 7px 10px;
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.16);
    border-radius: 6px;
    font-size: 11px;
    color: var(--destructive);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 0;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
