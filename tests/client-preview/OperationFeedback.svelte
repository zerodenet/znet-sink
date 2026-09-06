<script lang="ts">
  import { preview } from './state.svelte';
  let { target, pendingLabel = '正在切换，等待内核确认…' }: { target: string; pendingLabel?: string } = $props();
</script>

{#if preview.noticeTarget === target && (preview.pending === target || preview.notice)}
  <p class="operation-feedback" class:failed={preview.operationError} role={preview.operationError ? 'alert' : 'status'}>
    {preview.pending === target ? pendingLabel : preview.notice}
  </p>
{/if}

<style>
  .operation-feedback { margin:4px 0 0; font-size:11px; line-height:1.45; color:var(--muted-foreground); overflow-wrap:anywhere; }
  .operation-feedback.failed { color:var(--destructive); }
</style>
