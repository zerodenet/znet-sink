<script lang="ts">
  import { X } from '@lucide/svelte';
  import { cn } from '$lib/utils.js';
  import { Button } from '$lib/components/ui/button';
  import Root from './select.svelte';
  import Group from './select-group.svelte';
  import Trigger from './select-trigger.svelte';
  import Value from './select-value.svelte';
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

  const canClear = $derived(!disabled && value !== defaultValue);

  function clear() {
    value = defaultValue;
  }
</script>

<div class={cn('flex items-center gap-1', className)} data-slot="clearable-select">
  <Root type="single" bind:value items={options} disabled={disabled}>
    <Trigger
      {size}
      class="min-w-0 flex-1"
      aria-label={ariaLabel}
      disabled={disabled}
    >
      <Value />
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
      variant="ghost"
      size="icon-sm"
      class="shrink-0 text-muted-foreground"
      title={clearLabel}
      aria-label={clearLabel}
      onclick={clear}
    >
      <X class="size-3.5" />
    </Button>
  {/if}
</div>
