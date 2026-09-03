<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils.js';

  let {
    type = 'checkbox',
    checked = $bindable(false),
    onchange,
    class: className,
    ...restProps
  }: Omit<HTMLInputAttributes, 'type'> & { type?: 'checkbox' | 'radio' } = $props();
</script>

{#if type === 'radio'}
<input
  type="radio"
  {checked}
  onchange={(event) => { checked = event.currentTarget.checked; onchange?.(event); }}
  data-slot="choice"
  class={cn('znet-choice', className)}
  {...restProps}
/>
{:else}
<input
  type="checkbox"
  bind:checked
  {onchange}
  data-slot="choice"
  class={cn('znet-choice', className)}
  {...restProps}
/>
{/if}
