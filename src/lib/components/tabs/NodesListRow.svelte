<script lang="ts">
  import type { ProxyNode } from '$lib/types/protocol';
  import {
    delayBarWidth,
    formatDelay,
    getNodeChips,
    getProtocolStyle,
    gradeDelay,
    getProbeTimeStyle,
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
  const probeTimeState = $derived(getProbeTimeStyle(node.lastProbeAt));
</script>

<div class="node-row {isActive ? 'active' : ''}" role="listitem">
  <button
    class="node-main"
    onclick={() => onSelectNode(node)}
    disabled={selectDisabled}
  >
    <span class="node-radio {isActive ? 'on' : ''}">
      {#if isSwitching}
        <span class="node-spin-inline">⟳</span>
      {/if}
    </span>

    <div class="node-info">
      <span class="node-name {isActive ? 'active-name' : ''}">
        {#if node.emoji}<span class="node-emoji">{node.emoji}</span>{/if}
        {node.cleanName || node.name}
      </span>
      <div class="node-meta">
        <span class="proto-label" style="background: {protocolStyle.bg}; color: {protocolStyle.color};">{protocolStyle.label}</span>
        {#each chips as chip (chip.key)}
          <span class="attr-chip tone-{chip.tone}" title={chip.title}>{chip.label}</span>
        {/each}
        {#if node.domain && node.domain !== 'selected' && node.domain !== 'policy' && node.domain !== 'unavailable'}
          <span class="node-domain">{node.domain}</span>
        {/if}
        {#if delayState.level === 'dead'}
          <span class="node-unavailable">离线</span>
        {/if}
      </div>
    </div>
  </button>

    <div class="node-actions">
      <div
        class="delay-wrap"
        role="presentation"
        onmouseenter={(event) => onShowPopover(event, node)}
        onmouseleave={onHidePopover}
      >
        <span class="delay-pill" style="color: {delayState.color}; background: {delayState.bg};">
          {formatDelay(node.delay)}
          {#if node.delay > 0}<span class="delay-unit">ms</span>{/if}
          {#if delayState.grade && delayState.grade !== '—'}
            <span class="delay-grade">{delayState.grade}</span>
          {/if}
        </span>
        {#if node.lastProbeAt}
          <span class="probe-time" style="color: {probeTimeState.color};">
            {probeTimeState.label}
          </span>
        {/if}
      </div>

    <div class="delay-bar-track">
      <div class="delay-bar-fill" style="width: {delayBarWidth(node.delay)}; background: {delayState.bar};"></div>
    </div>

    <button
      class="probe-btn"
      onclick={() => onProbeNode(node)}
      disabled={probeDisabled || isProbing || probingAll}
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

<style>
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
    border-color: #a5b4fc;
    background: #a5b4fc;
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
    color: #0f1014;
  }

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
    color: #16a34a;
    border-color: rgba(34, 197, 94, 0.18);
  }

  .attr-chip.tone-accent {
    background: rgba(99, 102, 241, 0.10);
    color: var(--accent-foreground);
    border-color: rgba(99, 102, 241, 0.18);
  }

  .attr-chip.tone-warning {
    background: rgba(245, 158, 11, 0.10);
    color: #d97706;
    border-color: rgba(245, 158, 11, 0.18);
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

  .node-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding-right: 2px;
  }

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

  .probe-time {
    font-size: 9px;
    font-weight: 500;
    opacity: 0.6;
    margin-top: 1px;
    text-align: center;
  }

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

  .animate-spin {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>

