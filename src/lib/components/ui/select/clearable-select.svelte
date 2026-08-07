<script lang="ts">
  import { X } from '@lucide/svelte';
  import { cn } from '$lib/utils.js';
  import { Button } from '$lib/components/ui/button';
  import * as ButtonGroup from '$lib/components/ui/button-group';
  import Root from './select.svelte';
  import Group from './select-group.svelte';
  import Trigger from './select-trigger.svelte';
  import Content from './select-content.svelte';
  import Item from './select-item.svelte';

  interface ClearableSelectOption {
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

  function clear() {
    value = defaultValue;
  }
</script>

<ButtonGroup.Root class={cn('w-full', className)} aria-label={ariaLabel}>
  <Root type="single" bind:value disabled={disabled}>
    <Trigger
      {size}
      class="min-w-0 flex-1"
      aria-label={ariaLabel}
      disabled={disabled}
    >
      <span class="truncate">{selectedLabel}</span>
    </Trigger>
    <Content>
      <Group>
        {#each options as option (option.value)}
          <Item value={option.value} label={option.label} disabled={option.disabled}>
            {option.label}
          </Item>
        {/each}
      </Group>
    </Content>
  </Root>

  {#if canClear}
    <Button
      variant="outline"
      size="icon-sm"
      class="shrink-0"
      title={clearLabel}
      aria-label={clearLabel}
      onclick={clear}
    >
      <X class="size-3.5" />
    </Button>
  {/if}
</ButtonGroup.Root>
