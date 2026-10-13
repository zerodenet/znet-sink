<script lang="ts">
  import { LifeBuoy } from '@lucide/svelte';
  import SystemProxyRecovery from '$lib/components/core/SystemProxyRecovery.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';

  const status = $derived(!guiState.connection || guiState.connectionError ? '待确认'
    : !guiState.isSystemProxyEnabled ? '已关闭'
    : guiState.hasUnmanagedSystemProxy ? '未接管' : '已接管');
</script>

<section class="rescue-panel" aria-label="网络救援">
  <h3><LifeBuoy class="h-4 w-4" />网络救援</h3>
  <div class="rescue-row">
    <div class="rescue-description"><span>系统代理 <small>{status}</small></span>
      <p>重新接管或清除残留的系统代理。</p>
    </div>
    <SystemProxyRecovery />
  </div>
</section>

<style>
  .rescue-panel { padding:14px; border:1px solid var(--border); border-radius:10px; background:var(--card); margin-bottom:20px; }
  h3 { display:flex; align-items:center; gap:7px; font-size:13px; font-weight:600; margin:0 0 12px; }
  .rescue-row { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:12px; }
  .rescue-description { min-width:0; font-size:12px; }
  small { margin-left:8px; color:var(--muted-foreground); font-size:11px; }
  p { margin:4px 0 0; font-size:11px; color:var(--muted-foreground); }
</style>
