<script lang="ts">
  import type { ComponentProps } from 'svelte';
  import { List as BaseList } from '$lib/components/ui/tabs';
  import { cn } from '$lib/utils';

  type Surface = 'segment' | 'transparent';

  let {
    surface = 'segment',
    class: className,
    ...restProps
  }: ComponentProps<typeof BaseList> & { surface?: Surface } = $props();
</script>

<BaseList
  class={cn(
    'znet-tabs-list',
    surface === 'transparent' && 'znet-tabs-list-transparent',
    className,
  )}
  {...restProps}
/>

<style>
  :global(
    [data-slot='tabs-list'].znet-tabs-list:not(.znet-tabs-list-transparent)[data-variant='default']
  ) {
    background: var(--segment-bg);
  }

  :global(
    [data-slot='tabs-list'].znet-tabs-list[data-variant='default']
      [data-slot='tabs-trigger'].znet-tabs-trigger:is([data-state='active'], [data-active])
  ) {
    background: var(--segment-active-bg);
    color: var(--foreground);
    font-weight: 600;
    box-shadow: var(--segment-active-shadow);
  }
</style>
