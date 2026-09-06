<script lang="ts">
  import type { OverviewFeedback } from './types';
  let { feedback, target, pendingLabel = '正在切换，等待确认…' }: { feedback: OverviewFeedback; target: string; pendingLabel?: string } = $props();
</script>

{#if feedback.target === target && (feedback.pending === target || feedback.message)}
  <p class="operation-feedback" class:failed={feedback.error} role={feedback.error ? 'alert' : 'status'}>
    {feedback.pending === target ? pendingLabel : feedback.message}
  </p>
{/if}

<style>
  .operation-feedback { margin:4px 0 0; font-size:11px; line-height:1.45; color:var(--muted-foreground); overflow-wrap:anywhere; }
  .operation-feedback.failed { color:var(--destructive); }
</style>
