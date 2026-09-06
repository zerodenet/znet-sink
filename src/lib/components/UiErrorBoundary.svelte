<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { recordTelemetry } from '$lib/services/telemetry';
  import { describeUiError } from '$lib/services/ui-error';
  let { context, children }: { context: Record<string, unknown>; children: Snippet } = $props();
  function report(error: unknown) {
    const { message, ...details } = describeUiError(error, '页面渲染失败');
    void recordTelemetry({ level: 'error', area: 'ui', operation: 'page.render', message, context: { ...context, ...details } });
  }
</script>

<svelte:boundary onerror={report}>
  {@render children()}
  {#snippet failed(error, reset)}
    <div class="page-error" role="alert">
      <strong>页面显示异常</strong>
      <span>{describeUiError(error, '请重新打开此页面').message}</span>
      <Button variant="outline" size="sm" onclick={reset}>重新打开页面</Button>
    </div>
  {/snippet}
</svelte:boundary>

<style>
  .page-error { flex:1; min-width:0; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; padding:20px; font-size:12px; text-align:center; overflow-wrap:anywhere; color:var(--muted-foreground); }
  .page-error strong { color:var(--destructive); }
</style>
