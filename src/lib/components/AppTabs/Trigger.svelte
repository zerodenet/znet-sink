<script lang="ts">
  import type { ComponentProps } from 'svelte';
  import { Trigger as BaseTrigger } from '$lib/components/ui/tabs';
  import { cn } from '$lib/utils';

  let {
    class: className,
    ...restProps
  }: ComponentProps<typeof BaseTrigger> = $props();
</script>

<BaseTrigger class={cn('znet-tabs-trigger', className)} {...restProps} />
