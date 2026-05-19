<script lang="ts">
  export let logs: { time: string; level: string; content: string }[];
</script>

<div class="bg-card border border-card-border rounded-xl p-3 flex flex-col">
  <div class="flex items-center justify-between mb-2">
    <div class="flex items-center gap-2">
      <div class="w-2 h-2 rounded-full bg-muted"></div>
      <span class="text-xs font-medium text-muted-foreground">内核控制台</span>
    </div>
    <span class="text-[10px] px-2 py-0.5 rounded bg-muted text-muted-foreground">Info</span>
  </div>
  
  <div class="overflow-y-auto font-mono text-[10px] space-y-1 bg-muted/20 rounded p-2 max-h-24">
    {#each logs as log}
      <div class="flex gap-2">
        <span class="text-muted-foreground whitespace-nowrap">[{log.time}]</span>
        <span class="whitespace-nowrap {log.level === 'INFO' ? 'text-foreground' : 'text-muted-foreground'}">[{log.level}]</span>
        <span class="text-muted-foreground">{log.content}</span>
      </div>
    {/each}
  </div>
</div>
