<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { RotateCcw } from '@lucide/svelte';
  import { guiState } from '$lib/services/gui-state.svelte';

  let { busy = false, compact = false, label = '处理系统代理', class: className = '' }: {
    busy?: boolean; compact?: boolean; label?: string; class?: string;
  } = $props();
</script>

<Button variant="outline" size="sm" disabled={busy || !guiState.canResetSystemProxy}
  aria-label={label} title="接管或清除当前系统代理"
  class={[compact ? 'w-7 px-0 text-[10px] font-mono' : 'min-w-0', className]}
  onclick={() => guiState.openSystemProxyRecovery()}>
  {#if !compact}<RotateCcw class="h-4 w-4" />{/if}{compact ? 'SYS' : label}
</Button>
