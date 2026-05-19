<script lang="ts">
  import type { Capability } from '$lib/types/core';

  export let capability: Capability;
</script>

<div class="bg-card border border-card-border rounded-xl p-3 flex flex-col gap-2">
  <div class="flex items-center justify-between">
    <span class="text-xs font-medium text-foreground">{capability.name}</span>
    <div class="w-2 h-2 rounded-full {capability.available ? (capability.enabled ? 'bg-green-500' : 'bg-yellow-500') : 'bg-muted'}"></div>
  </div>
  <p class="text-[10px] text-muted-foreground">{capability.description}</p>
</div>
