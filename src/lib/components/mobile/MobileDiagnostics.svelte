<script lang="ts">
  import NativeDiagnosticExport from './NativeDiagnosticExport.svelte';
  import DiagnosticsPanel from '$lib/components/tabs/DiagnosticsPanel.svelte';
  import MobileRuntimeInspection from './MobileRuntimeInspection.svelte';
</script>
<section class="mobile-diagnostics" aria-label="诊断与受限捕获">
  <div class="diagnostic-export"><NativeDiagnosticExport /></div>
  <div class="diagnostic-export"><MobileRuntimeInspection /></div>
  <DiagnosticsPanel />
</section>
<style>
  .mobile-diagnostics { display:flex; flex:1; min-height:0; min-width:0; flex-direction:column; overflow:auto; }
  .diagnostic-export { padding:12px 14px; }
  .diagnostic-export :global(button) { min-height:44px; }
</style>
