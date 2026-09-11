<script lang="ts">
  import { diagnosticTools } from 'virtual:znet-product';
</script>

<div class="diag-panel">
  {#each diagnosticTools as tool (tool.id)}
    <div class="tool-slot"><tool.panel /></div>
  {/each}
</div>

<style>
  .diag-panel { display: flex; flex-direction: column; flex: 1; overflow-y: auto; min-height: 0; }
  .tool-slot + .tool-slot { border-top: 1px solid var(--border); padding-top: 16px; }
</style>
