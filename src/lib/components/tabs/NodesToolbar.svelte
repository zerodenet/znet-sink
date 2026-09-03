<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { onMount } from 'svelte';
  import { ArrowUpDown, EyeOff } from '@lucide/svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { Button } from '$lib/components/ui/button';
  import { nodesDisplayPreferences } from '$lib/components/tabs/nodes-display-preferences.svelte';

  type ViewMode = 'list' | 'grid';

  interface ProbeProgress {
    done: number;
    total: number;
  }

  interface Props {
    selectedGroup: string | null;
    filteredCount: number;
    isCoreAvailable: boolean;
    searchQuery: string;
    viewMode: ViewMode;
    probing: boolean;
    probeProgress: ProbeProgress;
    canProbeAll: boolean;
    canSortByDelay: boolean;
    probeDisabledReason?: string | null;
    onSearchQueryChange: (value: string) => void;
    onViewModeChange: (mode: ViewMode) => void;
    onProbeAll: () => void | Promise<void>;
  }

  let {
    selectedGroup,
    filteredCount,
    isCoreAvailable,
    searchQuery,
    viewMode,
    probing,
    probeProgress,
    canProbeAll,
    canSortByDelay,
    probeDisabledReason = null,
    onSearchQueryChange,
    onViewModeChange,
    onProbeAll,
  }: Props = $props();

  const hideTimeout = $derived(nodesDisplayPreferences.hideTimeout);
  const sortByDelay = $derived(nodesDisplayPreferences.sortByDelay);

  onMount(() => {
    nodesDisplayPreferences.load();
  });
</script>

<div class="node-toolbar">
  <div class="toolbar-left">
    <span class="node-title">{selectedGroup || '全部节点'}</span>
    <span class="node-count">{filteredCount}</span>
    <span
      class="conn-badge {isCoreAvailable ? 'on' : 'off'}"
      title={isCoreAvailable ? '内核已就绪' : '内核未就绪，延迟与切换不可用'}
    >
      <span class="conn-dot"></span>
      {isCoreAvailable ? '已就绪' : '未就绪'}
    </span>
  </div>

  <div class="toolbar-right">
    <div class="search-wrap">
      <svg
        width="13"
        height="13"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        class="search-icon"
      >
        <circle cx="11" cy="11" r="8"></circle>
        <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
      </svg>
      <Input
        value={searchQuery}
        oninput={(event) => onSearchQueryChange((event.currentTarget as HTMLInputElement).value)}
        placeholder={'搜索节点'}
        class="w-full pl-8"
      />
    </div>

    <Button
      variant="outline"
      size="icon-sm"
      aria-pressed={hideTimeout}
      aria-label={hideTimeout ? '显示超时节点' : '隐藏超时节点'}
      title={hideTimeout ? '当前已隐藏测速超时或离线节点；点击恢复显示' : '隐藏已经测速确认超时或离线的节点'}
      onclick={() => nodesDisplayPreferences.setHideTimeout(!hideTimeout)}
    >
      <EyeOff class="h-3.5 w-3.5" />
    </Button>

    {#if canSortByDelay}
      <Button
        variant="outline"
        size="icon-sm"
        aria-pressed={sortByDelay}
        aria-label={sortByDelay ? '恢复节点配置顺序' : '按节点延迟排序'}
        title={sortByDelay ? '当前按延迟排序；点击恢复配置顺序' : '按测速延迟从低到高排列 URLTest 节点'}
        onclick={() => nodesDisplayPreferences.setSortByDelay(!sortByDelay)}
      >
        <ArrowUpDown class="h-3.5 w-3.5" />
      </Button>
    {/if}

    <SegmentedControl.Root
      value={viewMode}
      onValueChange={(value) => onViewModeChange(value as ViewMode)}
      aria-label="节点显示方式"
    >
      <SegmentedControl.Item
        value="list"
        size="icon"
        title={'列表视图'}
        aria-label={'列表视图'}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <line x1="8" y1="6" x2="21" y2="6"></line>
          <line x1="8" y1="12" x2="21" y2="12"></line>
          <line x1="8" y1="18" x2="21" y2="18"></line>
          <line x1="3" y1="6" x2="3.01" y2="6"></line>
          <line x1="3" y1="12" x2="3.01" y2="12"></line>
          <line x1="3" y1="18" x2="3.01" y2="18"></line>
        </svg>
      </SegmentedControl.Item>
      <SegmentedControl.Item
        value="grid"
        size="icon"
        title={'网格视图'}
        aria-label={'网格视图'}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <rect x="3" y="3" width="7" height="7"></rect>
          <rect x="14" y="3" width="7" height="7"></rect>
          <rect x="3" y="14" width="7" height="7"></rect>
          <rect x="14" y="14" width="7" height="7"></rect>
        </svg>
      </SegmentedControl.Item>
    </SegmentedControl.Root>

    <Button variant="default" size="sm"

      onclick={onProbeAll}
      disabled={!canProbeAll}
      title={probeDisabledReason ?? undefined}
    >
      {#if probing}
        <span class="probe-spinner">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" class="animate-spin">
            <path d="M21 12a9 9 0 1 1-6.219-8.56"></path>
          </svg>
        </span>
        <span class="probe-progress-text">
          {probeProgress.total > 0 ? `${probeProgress.done}/${probeProgress.total}` : '测速中'}
        </span>
      {:else}
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12a9 9 0 1 1-6.219-8.56"></path>
        </svg>
        <span>{`测速`}</span>
      {/if}
    </Button>
  </div>
</div>

<style>
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
    background: rgba(34, 197, 94, 0.1);
    color: #16a34a;
  }

  .conn-badge.off {
    background: rgba(245, 158, 11, 0.1);
    color: #d97706;
  }

  :global(.dark) .conn-badge.on {
    background: rgba(74, 222, 128, 0.1);
    color: #4ade80;
  }

  :global(.dark) .conn-badge.off {
    background: rgba(251, 191, 36, 0.1);
    color: #fbbf24;
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

  @media (max-width: 700px) {

  }
</style>
