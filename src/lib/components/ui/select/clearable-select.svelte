<script lang="ts">
  import { X } from '@lucide/svelte';
  import { cn } from '$lib/utils.js';
  import { Button } from '$lib/components/ui/button';
  import Root from './select.svelte';
  import Trigger from './select-trigger.svelte';
  import Content from './select-content.svelte';
  import Item from './select-item.svelte';

  export interface ClearableSelectOption {
    value: string;
    label: string;
    disabled?: boolean;
  }

  let {
    value = $bindable('all'),
    options,
    defaultValue = 'all',
    ariaLabel,
    clearLabel = '清除筛选',
    class: className,
    size = 'sm',
    disabled = false,
  } = $props<{
    value?: string;
    options: ClearableSelectOption[];
    defaultValue?: string;
    ariaLabel: string;
    clearLabel?: string;
    class?: string;
    size?: 'sm' | 'default';
    disabled?: boolean;
  }>();

  const selectedLabel = $derived(
    options.find((option) => option.value === value)?.label ?? value,
  );
  const canClear = $derived(!disabled && value !== defaultValue);

  function clear(event: MouseEvent) {
    event.stopPropagation();
    value = defaultValue;
  }
</script>

<div class={cn('relative', className)} data-slot="clearable-select">
  <Root type="single" bind:value disabled={disabled}>
    <Trigger
      {size}
      class={cn('w-full', canClear && 'pr-12')}
      aria-label={ariaLabel}
      disabled={disabled}
    >
      {selectedLabel}
    </Trigger>
    <Content>
      {#each options as option (option.value)}
        <Item value={option.value} label={option.label} disabled={option.disabled} />
      {/each}
    </Content>
  </Root>

  {#if canClear}
    <Button
      variant="ghost"
      size="icon-xs"
      class="absolute right-6 top-1/2 z-10 -translate-y-1/2 text-muted-foreground"
      title={clearLabel}
      aria-label={clearLabel}
      onpointerdown={(event) => event.stopPropagation()}
      onclick={clear}
    >
      <X class="size-3" />
    </Button>
  {/if}
</div>
