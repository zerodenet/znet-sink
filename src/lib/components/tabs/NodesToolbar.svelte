<script lang="ts">
  import { onMount } from 'svelte';
  import { EyeOff } from '@lucide/svelte';
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
    probeDisabledReason = null,
    onSearchQueryChange,
    onViewModeChange,
    onProbeAll,
  }: Props = $props();

  const hideTimeout = $derived(nodesDisplayPreferences.hideTimeout);

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
      <input
        value={searchQuery}
        oninput={(event) => onSearchQueryChange((event.currentTarget as HTMLInputElement).value)}
        placeholder={'搜索节点'}
        class="search-input"
      />
    </div>

    <Button
      variant="outline"
      size="sm"
      aria-pressed={hideTimeout}
      aria-label={hideTimeout ? '显示超时节点' : '隐藏超时节点'}
      title={hideTimeout ? '当前已隐藏测速超时或离线节点；点击恢复显示' : '隐藏已经测速确认超时或离线的节点'}
      onclick={() => nodesDisplayPreferences.setHideTimeout(!hideTimeout)}
    >
      <EyeOff class="h-3.5 w-3.5" />
      <span>隐藏超时</span>
    </Button>

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

    <button
      class="probe-all-btn"
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
    </button>
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

  .search-input {
    width: 130px;
    height: var(--control-height);
    padding: 0 8px 0 26px;
    border-radius: var(--control-radius);
    border: 1px solid var(--input);
    background: var(--background);
    color: var(--foreground);
    font-size: 12px;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.04);
    outline: none;
    transition: border-color 0.15s ease, box-shadow 0.15s ease, width 0.2s ease;
  }

  .search-input::placeholder {
    color: var(--muted-foreground);
    opacity: 0.5;
  }

  .search-input:focus {
    border-color: var(--ring);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--ring) 18%, transparent);
    width: 180px;
  }

  .probe-all-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: var(--control-height);
    padding: 0 10px;
    border-radius: var(--control-radius);
    border: 1px solid transparent;
    background: var(--primary);
    color: var(--primary-foreground);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.12s ease, box-shadow 0.12s ease, transform 0.12s ease;
    white-space: nowrap;
  }

  .probe-all-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--primary) 88%, transparent);
  }

  .probe-all-btn:active:not(:disabled) {
    transform: translateY(1px);
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

  @media (max-width: 700px) {
    .search-input {
      width: 100px;
    }

    .search-input:focus {
      width: 140px;
    }
  }
</style>
