<script lang="ts">
  const { label }: { label: string } = $props();
</script>

<div class="flex-1 w-full bg-card border border-card-border rounded-2xl p-6 shadow-sm flex items-center justify-center animate-fade-in">
  <div class="text-center">
    <div class="text-2xl mb-2">🚧</div>
    <div class="text-sm font-bold text-foreground mb-1">{label}</div>
    <div class="text-[10px] text-muted-foreground font-mono">开发中...</div>
  </div>
</div>
