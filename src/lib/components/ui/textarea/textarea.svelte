<script lang="ts">
  import type { HTMLTextareaAttributes } from 'svelte/elements';
  import { cn, type WithElementRef } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    value = $bindable(''),
    class: className,
    ...restProps
  }: WithElementRef<HTMLTextareaAttributes> = $props();
</script>

<textarea
  bind:this={ref}
  bind:value
  data-slot="textarea"
  class={cn('znet-field min-h-20 w-full resize-y px-2.5 py-2', className)}
  {...restProps}
></textarea>
