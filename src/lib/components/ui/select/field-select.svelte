<script lang="ts">
  import type { ComponentProps } from 'svelte';
  import * as Select from './index';
  import { cn } from '$lib/utils.js';

  let {
    value = $bindable(''),
    options,
    onValueChange,
    disabled = false,
    placeholder = '请选择',
    class: className,
    ...triggerProps
  }: Omit<ComponentProps<typeof Select.Trigger>, 'children' | 'value' | 'disabled'> & {
    value?: string;
    options: readonly { value: string; label: string; disabled?: boolean }[];
    onValueChange?: (value: string) => void;
    disabled?: boolean;
    placeholder?: string;
  } = $props();
</script>

<Select.Root type="single" bind:value {onValueChange} {disabled}>
  <Select.Trigger class={cn('w-full min-w-0', className)} {...triggerProps}>
    <span class="min-w-0 truncate">{options.find((option) => option.value === value)?.label ?? placeholder}</span>
  </Select.Trigger>
  <Select.Content>
    {#each options as option (option.value)}
      <Select.Item value={option.value} label={option.label} disabled={option.disabled}>
        {option.label}
      </Select.Item>
    {/each}
  </Select.Content>
</Select.Root>
