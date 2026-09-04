<script lang="ts">
  import type { Snippet } from 'svelte';

  export interface SectionWorkspaceItem {
    id: string;
    label: string;
    group?: string;
  }

  interface Props {
    title: string;
    items: SectionWorkspaceItem[];
    activeId: string;
    onSelect: (id: string) => void;
    contentMode?: 'scroll' | 'contained';
    children: Snippet;
  }

  let {
    title,
    items,
    activeId,
    onSelect,
    contentMode = 'scroll',
    children,
  }: Props = $props();
</script>

<section class="section-workspace animate-fade-in">
  <nav class="section-nav" aria-label={title}>
    <div class="section-nav-header">{title}</div>
    {#each items as item, index (item.id)}
      {#if item.group && (index === 0 || items[index - 1]?.group !== item.group)}
        <div class="section-nav-group">{item.group}</div>
      {/if}
      <button data-slot="surface-button"
        type="button"
        class="section-nav-item"
        class:active={activeId === item.id}
        aria-current={activeId === item.id ? 'page' : undefined}
        onclick={() => onSelect(item.id)}
      >
        {item.label}
      </button>
    {/each}
  </nav>

  <div class="section-content" class:contained={contentMode === 'contained'}>
    {@render children()}
  </div>
</section>

<style>
  .section-workspace {
    display: flex;
    flex: 1;
    width: 100%;
    min-height: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--card);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
  }

  .section-nav {
    display: flex;
    width: min(130px, 30vw);
    min-width: 80px;
    flex-shrink: 0;
    flex-direction: column;
    gap: 1px;
    padding: 14px 8px;
    border-right: 1px solid var(--border);
    background: var(--surface, rgba(0, 0, 0, 0.018));
  }

  :global(.dark) .section-nav {
    background: rgba(255, 255, 255, 0.015);
  }

  .section-nav-header {
    padding: 4px 8px 10px;
    color: var(--muted-foreground);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    opacity: 0.65;
  }

  .section-nav-group {
    margin: 8px 8px 3px;
    color: var(--muted-foreground);
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    opacity: 0.58;
  }

  .section-nav-group:first-of-type {
    margin-top: 0;
  }

  .section-nav-item {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition: background 0.13s ease, color 0.13s ease;
  }

  .section-nav-item:hover {
    background: var(--muted);
    color: var(--foreground);
  }

  .section-nav-item.active {
    background: var(--primary);
    color: var(--primary-foreground);
    font-weight: 600;
  }

  .section-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    padding: 14px 16px;
  }

  .section-content.contained {
    display: flex;
    overflow: hidden;
  }

  @media (max-width: 500px) {
    .section-content {
      padding: 10px 12px;
    }

    .section-nav-item {
      padding: 6px;
      font-size: 11px;
    }
  }
</style>
