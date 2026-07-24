<script lang="ts">
  import { ToggleGroup } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  type Props = Omit<
    ToggleGroup.RootProps,
    'type' | 'value' | 'onValueChange' | 'children' | 'child'
  > & {
    value?: string;
    onValueChange?: (value: string) => void;
    children?: Snippet;
  };

  let {
    value = $bindable(''),
    onValueChange,
    disabled = false,
    class: className,
    children,
    ...restProps
  }: Props = $props();

  let lastValue = $state(value);

  $effect(() => {
    if (value) lastValue = value;
  });

  function handleValueChange(nextValue: string) {
    if (!nextValue) {
      value = lastValue;
      return;
    }
    lastValue = nextValue;
    onValueChange?.(nextValue);
  }
</script>

<ToggleGroup.Root
  type="single"
  bind:value
  onValueChange={handleValueChange}
  {disabled}
  data-slot="segmented-root"
  class={cn('znet-segmented-root', className)}
  {...restProps}
>
  {@render children?.()}
</ToggleGroup.Root>

<style>
  :global([data-slot='segmented-root'].znet-segmented-root) {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    padding: 2px;
    border: 0;
    border-radius: var(--control-radius);
    background: var(--segment-bg);
    flex-shrink: 0;
  }
</style>
