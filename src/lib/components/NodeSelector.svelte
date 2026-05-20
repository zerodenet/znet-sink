<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import type { ProxyNode } from '$lib/types/protocol';
  import { selectPolicy, probePolicy } from '$lib/services/core';
  import { Card, CardContent } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';

  const { nodes, initialSelected = '' }: {
    nodes: ProxyNode[];
    initialSelected?: string;
  } = $props();

  let selected = $state('');

  $effect(() => {
    selected = initialSelected;
  });
  let switching = $state<string | null>(null);
  let lastError = $state<string | null>(null);

  async function handleSelect(node: ProxyNode) {
    if (switching) return;
    switching = node.id;
    lastError = null;

    try {
      await probePolicy(node.name);
      const result = await selectPolicy('proxy', node.name);
      if (result.error) {
        lastError = result.error.message;
      } else {
        selected = node.id;
      }
    } catch (e) {
      lastError = String(e);
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

<Card class="h-full">
  <CardContent class="p-3 h-full flex flex-col">
    <div class="flex items-center justify-between mb-3 flex-shrink-0 overflow-hidden">
      <span class="text-xs font-medium text-muted-foreground truncate">核心策略出口</span>
      <Badge variant="secondary" class="text-[10px]">Radio</Badge>
    </div>

    {#if nodes.length === 0}
      <div class="flex-1 flex items-center justify-center text-[10px] text-muted-foreground">等待节点数据...</div>
    {:else}
      <div class="flex flex-col gap-1 flex-1 overflow-y-auto min-h-0">
        {#each nodes as node}
          <Button
            variant={selected === node.id ? 'default' : 'ghost'}
            class="w-full justify-between h-auto py-2.5 px-3
                   {selected === node.id ? 'bg-primary/10 hover:bg-primary/15 text-foreground border border-primary/20 shadow-sm' : 'text-muted-foreground hover:text-foreground border border-transparent'}
                   {switching === node.id ? 'opacity-60' : ''}"
            onclick={() => handleSelect(node)}
            disabled={switching !== null || !store.isActionOperable('policies.select')}
          >
            <span class="truncate flex items-center gap-2">
              <span class="w-1.5 h-1.5 rounded-full flex-shrink-0 {selected === node.id ? 'bg-primary' : 'bg-muted-foreground/30 group-hover:bg-muted-foreground/50'} transition-colors"></span>
              <span class="font-medium text-xs">{node.name}</span>
              {#if switching === node.id}
                <span class="text-[10px] text-muted-foreground animate-pulse">切换中...</span>
              {/if}
            </span>
            <span class="text-[10px] font-mono font-bold flex-shrink-0 ml-2 {getDelayColor(node.delay)}">
              {node.delay}ms
            </span>
          </Button>
        {/each}
      </div>
    {/if}

    {#if lastError}
      <div class="mt-2 text-[10px] text-red-500 truncate flex-shrink-0 bg-red-500/10 px-2 py-1.5 rounded-lg" title={lastError}>
        {lastError}
      </div>
    {/if}
  </CardContent>
</Card>
