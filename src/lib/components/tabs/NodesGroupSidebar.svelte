<script lang="ts">
  import { getGroupKindStyle } from '$lib/services/node-utils';
  import type { PolicyGroup } from '$lib/types/gui-api';
  import { collectGroupNodeTags } from '$lib/components/tabs/nodes-view-model';

  interface Props {
    groups: PolicyGroup[];
    allNodesCount: number;
    selectedGroup: string | null;
    proxyMode?: string | null;
    onSelectGroup: (groupName: string | null) => void;
  }

  let { groups, allNodesCount, selectedGroup, proxyMode, onSelectGroup }: Props = $props();

  // "全部节点" 仅在全局模式下显示 — 非全局时用户按具体分组筛选；
  // 不在此处回退到"全部"，否则会和全局模式语义混淆。
  const showAllNodes = $derived(proxyMode === 'global');
</script>

<aside class="group-sidebar">
  <div class="group-header">
    <span class="group-header-label">{`策略组`}</span>
    <span class="group-header-count">{groups.length}</span>
  </div>

  {#if showAllNodes}
    <button
      class="group-item {!selectedGroup ? 'active' : ''}"
      onclick={() => onSelectGroup(null)}
    >
      <div class="group-info">
        <span class="group-name">{`全部节点`}</span>
      </div>
      <span class="group-count">{allNodesCount}</span>
    </button>
  {/if}

  {#each groups as group}
    <button
      class="group-item {selectedGroup === group.name ? 'active' : ''}"
      onclick={() => onSelectGroup(group.name)}
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
      <span class="group-count">{collectGroupNodeTags(groups, group.name).size}</span>
    </button>
  {/each}

  {#if groups.length === 0}
    {#if allNodesCount > 0}
      <div class="group-empty">{`配置节点`} ({allNodesCount})</div>
    {:else}
      <div class="group-empty">{`等待数据…`}</div>
    {/if}
  {/if}
</aside>

<style>
  .group-sidebar {
    width: 168px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 10px 8px;
    border-right: 1px solid var(--border);
    background: var(--surface, rgba(0, 0, 0, 0.015));
    overflow-y: auto;
  }

  :global(.dark) .group-sidebar {
    background: rgba(255, 255, 255, 0.012);
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

  .group-item:hover {
    background: var(--muted);
  }

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

  .group-item.active .group-name {
    font-weight: 600;
  }

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
    background: #22c55e;
    flex-shrink: 0;
  }

  :global(.dark) .group-selected-dot {
    background: #4ade80;
  }

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

  @media (max-width: 700px) {
    .group-sidebar {
      width: 120px;
      padding: 8px 6px;
    }
  }
</style>