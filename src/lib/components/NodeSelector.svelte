<script lang="ts">
  import type { ProxyNode } from '$lib/types/protocol';

  const { nodes, initialSelected = 'node-2' }: {
    nodes: ProxyNode[];
    initialSelected?: string;
  } = $props();
  
  let selected = $state(initialSelected);
</script>

<div class="bg-card border border-card-border rounded-xl p-3 flex flex-col h-full">
  <div class="flex items-center justify-between mb-3 flex-shrink-0 overflow-hidden">
    <span class="text-sm font-medium text-muted-foreground truncate">核心策略出口</span>
    <span class="text-xs text-muted-foreground flex-shrink-0 ml-2">Radio</span>
  </div>
  
  <div class="flex flex-col gap-1 flex-1 overflow-y-auto min-h-0">
    {#each nodes as node}
      <button
        onclick={() => selected = node.id}
        class="w-full px-3 py-2 rounded-lg text-left text-sm font-medium transition-all flex items-center justify-between flex-shrink-0 overflow-hidden
               {selected === node.id 
                 ? 'bg-muted text-foreground border border-card-border' 
                 : 'text-muted-foreground hover:bg-muted/50'}"
      >
        <span class="truncate">{node.name}</span>
        {#if selected === node.id}
          <span class="text-xs text-muted-foreground flex-shrink-0 ml-2">{node.delay}ms</span>
        {/if}
      </button>
    {/each}
  </div>
</div>
