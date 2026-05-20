<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { selectPolicy, probePolicy } from '$lib/services/core';
  import type { ProxyNode } from '$lib/types/protocol';

  interface Props {
    nodes: ProxyNode[];
    showCheck?: boolean;
  }
  let { nodes, showCheck = false }: Props = $props();

  let switching = $state<string | null>(null);

  async function handleSelect(node: ProxyNode) {
    if (switching) return;
    switching = node.id;
    try {
      await probePolicy(node.name);
      const result = await selectPolicy('proxy', node.name);
      if (!result.error) {
        store.selectedNodeId = node.id;
      }
    } catch (e) {
      console.error('Policy switch failed:', e);
    } finally {
      switching = null;
    }
  }

  function getDelayColor(delay: number): string {
    if (delay < 100) return 'text-green-500';
    if (delay < 200) return 'text-yellow-500';
    return 'text-red-500';
  }
</script>

<div class="flex-1 w-full grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-3 overflow-y-auto content-start">
  {#each nodes as node}
    <button
      onclick={() => handleSelect(node)}
      disabled={switching !== null}
      class="p-4 rounded-xl border text-left transition-all duration-200 flex flex-col justify-between h-24 bg-card group
             {store.selectedNodeId === node.id ? 'border-primary/40 shadow-lg bg-primary/5 ring-1 ring-primary/20 scale-[1.02]' : 'border-card-border hover:border-border hover:shadow-md hover:scale-[1.01]'}
             {switching === node.id ? 'opacity-60' : ''}"
    >
      <div class="w-full flex justify-between items-start">
        <div class="flex flex-col gap-0.5 min-w-0 flex-1">
          <span class="text-xs font-bold truncate {store.selectedNodeId === node.id ? 'text-primary' : 'text-foreground'}">{node.name}</span>
          <span class="text-[10px] text-muted-foreground/60 font-mono truncate">{node.domain}</span>
        </div>
        {#if showCheck && store.selectedNodeId === node.id}
          <span class="w-5 h-5 rounded-full bg-primary/20 text-primary font-bold text-xs flex items-center justify-center flex-shrink-0 ml-2">✓</span>
        {/if}
      </div>
      <div class="w-full flex justify-between items-center">
        <div class="w-2 h-2 rounded-full flex-shrink-0 {getDelayColor(node.delay)} {store.selectedNodeId === node.id ? 'animate-pulse' : 'opacity-60 group-hover:opacity-100'} transition-opacity"></div>
        <div class="text-xs font-mono font-bold {store.selectedNodeId === node.id ? 'text-primary' : getDelayColor(node.delay)}">
          {#if switching === node.id}
            <span class="text-[10px] animate-pulse">切换中...</span>
          {:else}
            {node.delay} ms
          {/if}
        </div>
      </div>
    </button>
  {/each}
</div>
