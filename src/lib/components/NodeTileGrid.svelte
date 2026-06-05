<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { selectPolicy } from '$lib/services/core';
  import type { ProxyNode } from '$lib/types/protocol';

  interface Props {
    nodes: ProxyNode[];
    showCheck?: boolean;
  }
  let { nodes, showCheck = false }: Props = $props();

  let switching = $state<string | null>(null);

  // Protocol color mapping
  const PROTOCOL_STYLES: Record<string, { bg: string; color: string }> = {
    shadowsocks: { bg: 'rgba(139,92,246,0.12)', color: '#8B5CF6' },
    vmess:       { bg: 'rgba(59,130,246,0.12)',  color: '#3B82F6' },
    vless:       { bg: 'rgba(16,185,129,0.12)',  color: '#10B981' },
    trojan:      { bg: 'rgba(239,68,68,0.12)',   color: '#EF4444' },
    hysteria:    { bg: 'rgba(249,115,22,0.12)',  color: '#F97316' },
    hysteria2:   { bg: 'rgba(249,115,22,0.12)',  color: '#F97316' },
    wireguard:   { bg: 'rgba(20,184,166,0.12)',  color: '#14B8A6' },
    tuic:        { bg: 'rgba(99,102,241,0.12)',  color: '#6366F1' },
  };

  const DEFAULT_STYLE = { bg: 'rgba(107,114,128,0.10)', color: '#6B7280' };

  function getProtoStyle(protocol: string) {
    const key = protocol.toLowerCase().replace(/[-_]/g, '');
    for (const [k, v] of Object.entries(PROTOCOL_STYLES)) {
      if (key.includes(k)) return v;
    }
    return DEFAULT_STYLE;
  }

  function getDelayColor(delay: number): string {
    if (delay <= 0) return 'var(--muted-foreground)';
    if (delay < 200) return '#22C55E';
    if (delay < 500) return '#F59E0B';
    return '#EF4444';
  }

  function getDelayBg(delay: number): string {
    if (delay <= 0) return 'var(--muted)';
    if (delay < 200) return 'rgba(34,197,94,0.10)';
    if (delay < 500) return 'rgba(245,158,11,0.10)';
    return 'rgba(239,68,68,0.10)';
  }

  async function handleSelect(node: ProxyNode) {
    if (switching) return;
    switching = node.id;
    try {
      const result = await selectPolicy('proxy', node.name);
      if (!result.error) {
        store.selectedNodeId = node.id;
      }
    } catch (e) {
      console.error('Policy switch failed:', e);
    } finally {
      switching = null;
    }
  }
</script>

<div class="tile-grid">
  {#each nodes as node}
    {@const isActive = store.selectedNodeId === node.id}
    {@const isSwitching = switching === node.id}
    {@const ps = getProtoStyle(node.protocol)}

    <button
      onclick={() => handleSelect(node)}
      disabled={switching !== null}
      class="tile {isActive ? 'active' : ''} {isSwitching ? 'switching' : ''}"
    >
      <!-- Header: name + check -->
      <div class="tile-header">
        <span class="tile-name truncate">{node.name}</span>
        {#if showCheck && isActive}
          <span class="tile-check" aria-hidden="true">
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="2,5 4,7 8,3"/>
            </svg>
          </span>
        {/if}
      </div>

      <!-- Protocol badge -->
      <span class="tile-proto" style="background: {ps.bg}; color: {ps.color};">
        {node.protocol}
      </span>

      <!-- Bottom: delay -->
      <div class="tile-footer">
        {#if isSwitching}
          <span class="tile-switching">⟳</span>
        {:else}
          <span class="tile-delay" style="color: {getDelayColor(node.delay)}; background: {getDelayBg(node.delay)};">
            {node.delay > 0 ? node.delay : '—'}
            {#if node.delay > 0}
              <span class="tile-delay-unit">ms</span>
            {/if}
          </span>
        {/if}
      </div>

      <!-- Delay indicator bar -->
      <div class="tile-bar">
        <div
          class="tile-bar-fill"
          style="background: {getDelayColor(node.delay)}; width: {node.delay > 0 ? Math.min(100, (node.delay / 1000) * 100) : 0}%;"
        ></div>
      </div>
    </button>
  {/each}
</div>

<style>
  .tile-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 8px;
    overflow-y: auto;
    padding: 4px;
    width: 100%;
    align-content: start;
  }

  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 11px 13px;
    min-height: 82px;
    background: var(--card);
    border: 1.5px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    overflow: hidden;
    transition: background 0.12s ease, border-color 0.15s ease, box-shadow 0.15s ease;
    text-align: left;
  }

  .tile:hover {
    background: var(--surface);
    border-color: rgba(128, 128, 160, 0.18);
  }

  .tile.active {
    background: rgba(99, 102, 241, 0.06);
    border-color: rgba(99, 102, 241, 0.3);
    box-shadow: 0 0 0 1px rgba(99,102,241,0.08);
  }

  :global(.dark) .tile.active {
    background: rgba(99, 102, 241, 0.1);
    border-color: rgba(165, 180, 252, 0.25);
    box-shadow: 0 0 0 1px rgba(165,180,252,0.08);
  }

  .tile.switching {
    opacity: 0.5;
    pointer-events: none;
  }

  .tile-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 4px;
  }

  .tile-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--foreground);
    line-height: 1.25;
    max-width: calc(100% - 20px);
  }

  .tile.active .tile-name {
    color: var(--accent-foreground);
  }

  :global(.dark) .tile.active .tile-name {
    color: #A5B4FC;
  }

  .tile-proto {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 5px;
    border-radius: 3px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    align-self: flex-start;
  }

  .tile-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    margin-top: auto;
    min-height: 18px;
  }

  .tile-delay {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    padding: 2px 7px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 700;
    font-family: var(--font-mono);
    line-height: 1;
  }

  .tile-delay-unit {
    font-size: 9px;
    font-weight: 600;
    opacity: 0.55;
  }

  .tile-switching {
    font-size: 12px;
    color: var(--muted-foreground);
    animation: pulse 1s infinite;
  }

  .tile-check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: rgba(99,102,241,0.18);
    color: var(--accent-foreground);
    flex-shrink: 0;
    margin-top: 1px;
  }

  :global(.dark) .tile-check {
    background: rgba(165,180,252,0.18);
    color: #A5B4FC;
  }

  .tile-bar {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: 2.5px;
    background: var(--muted);
    opacity: 0.2;
    border-radius: 0 0 8px 8px;
    overflow: hidden;
  }

  .tile-bar-fill {
    height: 100%;
    border-radius: 0 0 8px 8px;
    transition: width 0.3s ease;
  }

  .tile:hover .tile-bar,
  .tile.active .tile-bar {
    opacity: 0.5;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }
</style>
