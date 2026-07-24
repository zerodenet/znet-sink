<script lang="ts">
  import { ToggleGroup } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  type Size = 'default' | 'icon' | 'comfortable';

  type Props = Omit<ToggleGroup.ItemProps, 'children' | 'child'> & {
    size?: Size;
    children?: Snippet;
  };

  let {
    value,
    size = 'default',
    class: className,
    children,
    ...restProps
  }: Props = $props();
</script>

<ToggleGroup.Item
  {value}
  data-slot="segmented-item"
  class={cn(
    'znet-segmented-item',
    size === 'icon' && 'znet-segmented-item-icon',
    size === 'comfortable' && 'znet-segmented-item-comfortable',
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</ToggleGroup.Item>

<style>
  :global([data-slot='segmented-item'].znet-segmented-item) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-width: 0;
    height: var(--control-height-compact);
    padding: 0 10px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted-foreground);
    font: inherit;
    font-size: 11.5px;
    font-weight: 500;
    line-height: 1;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background 0.12s ease,
      color 0.12s ease,
      box-shadow 0.12s ease;
  }

  :global([data-slot='segmented-item'].znet-segmented-item:hover:not(:disabled)) {
    color: var(--foreground);
  }

  :global([data-slot='segmented-item'].znet-segmented-item[data-state='on']) {
    background: var(--segment-active-bg);
    color: var(--foreground);
    font-weight: 600;
    box-shadow: var(--segment-active-shadow);
  }

  :global([data-slot='segmented-item'].znet-segmented-item:focus-visible) {
    outline: 2px solid var(--ring);
    outline-offset: 1px;
  }

  :global([data-slot='segmented-item'].znet-segmented-item:disabled) {
    opacity: 0.45;
    cursor: not-allowed;
  }

  :global([data-slot='segmented-item'].znet-segmented-item-icon) {
    width: var(--control-height-compact);
    padding: 0;
  }

  :global([data-slot='segmented-item'].znet-segmented-item-comfortable) {
    height: 36px;
    padding: 0 12px;
    border-radius: 6px;
  }
</style>
