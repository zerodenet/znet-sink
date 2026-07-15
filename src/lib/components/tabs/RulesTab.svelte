<script lang="ts">
  import { Clock3 } from '@lucide/svelte';
</script>

<div class="rules-coming-soon animate-fade-in">
  <Clock3 size={28} strokeWidth={1.5} aria-hidden="true" />
  <h2>规则功能敬请期待</h2>
  <p>当前内核尚未提供规则管理能力，功能将在内核支持后开放。</p>
</div>

<style>
  .rules-coming-soon {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--muted-foreground);
    text-align: center;
    padding: 24px;
  }

  h2 {
    margin: 4px 0 0;
    color: var(--foreground);
    font-size: 15px;
    font-weight: 600;
  }

  p {
    margin: 0;
    max-width: 360px;
    font-size: 12px;
    line-height: 1.6;
  }
</style>
