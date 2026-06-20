<script lang="ts">
  import type { ProxyNode } from '$lib/types/protocol';
  import {
    delayBarWidth,
    formatDelay,
    getNodeChips,
    getProtocolStyle,
    gradeDelay,
  } from '$lib/services/node-utils';

  interface Props {
    node: ProxyNode;
    isActive: boolean;
    isSwitching: boolean;
    isProbing: boolean;
    probingAll: boolean;
    probeDisabled: boolean;
    selectDisabled: boolean;
    onSelectNode: (node: ProxyNode) => void | Promise<void>;
    onProbeNode: (node: ProxyNode) => void | Promise<void>;
    onShowPopover: (event: MouseEvent, node: ProxyNode) => void;
    onHidePopover: () => void;
  }

  let {
    node,
    isActive,
    isSwitching,
    isProbing,
    probingAll,
    probeDisabled,
    selectDisabled,
    onSelectNode,
    onProbeNode,
    onShowPopover,
    onHidePopover,
  }: Props = $props();

  const delayState = $derived(gradeDelay(node.delay, node.alive));
  const protocolStyle = $derived(getProtocolStyle(node.protocol));
  const chips = $derived(getNodeChips(node));
</script>

<div
  class="grid-card-wrap"
  role="listitem"
  onmouseenter={(event) => onShowPopover(event, node)}
  onmouseleave={onHidePopover}
>
  <button
    class="grid-card {isActive ? 'active' : ''} {isSwitching ? 'switching' : ''}"
    onclick={() => onSelectNode(node)}
    disabled={selectDisabled}
  >
    <div class="grid-card-header">
      <span class="grid-card-name {isActive ? 'grid-card-name-active' : ''}">
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

    <div class="grid-badges">
      <span class="proto-label" style="background: {protocolStyle.bg}; color: {protocolStyle.color};">{protocolStyle.label}</span>
      {#each chips as chip (chip.key)}
        <span class="attr-chip tone-{chip.tone}" title={chip.title}>{chip.label}</span>
      {/each}
    </div>

    <div class="grid-card-footer">
      {#if isSwitching}
        <span class="grid-spin">⟳</span>
      {:else}
        <span class="grid-delay" style="color: {delayState.color};">
          {formatDelay(node.delay)}{#if node.delay > 0}<span class="grid-delay-unit">ms</span>{/if}
          {#if delayState.grade && delayState.grade !== '—'}<span class="grid-delay-grade">{delayState.grade}</span>{/if}
        </span>
      {/if}
    </div>

    <div class="grid-bar-track">
      <div class="grid-bar-fill" style="width: {delayBarWidth(node.delay)}; background: {delayState.bar};"></div>
    </div>
  </button>

  <button
    class="grid-probe-btn"
    onclick={() => onProbeNode(node)}
    disabled={probeDisabled || isProbing || probingAll}
    title="测试延迟"
    aria-label="测试 {node.name} 延迟"
  >
    {#if isProbing}<span class="grid-probe-spin">⟳</span>{:else}测速{/if}
  </button>
</div>

<style>
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

  .grid-card-name-active {
    color: var(--accent-foreground);
  }

  :global(.dark) .grid-card-name-active {
    color: #a5b4fc;
  }

  .node-emoji {
    margin-right: 2px;
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
    color: #a5b4fc;
  }

  .grid-badges {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-wrap: wrap;
    min-height: 15px;
  }

  .proto-label {
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

  .attr-chip {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 5px;
    border-radius: 999px;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.01em;
    flex-shrink: 0;
  }

  .attr-chip.tone-success {
    background: rgba(34, 197, 94, 0.1);
    color: #16a34a;
  }

  .attr-chip.tone-accent {
    background: rgba(99, 102, 241, 0.1);
    color: #6366f1;
  }

  .attr-chip.tone-warning {
    background: rgba(245, 158, 11, 0.1);
    color: #d97706;
  }

  .attr-chip.tone-muted {
    background: var(--muted);
    color: var(--muted-foreground);
  }

  :global(.dark) .attr-chip.tone-success {
    color: #4ade80;
  }

  :global(.dark) .attr-chip.tone-warning {
    color: #fbbf24;
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

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
