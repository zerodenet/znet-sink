<script lang="ts">
  import { Download, RefreshCw, RotateCw, Settings2, ScrollText } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { guiExportDiagnostics, restartCoreProcess } from '$lib/services/core';
  import { store } from '$lib/services/store.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { success, warning } from '$lib/services/toast.svelte';

  let {
    code,
    context = 'generic',
    onretry,
  }: {
    code?: string;
    context?: 'dns' | 'tun' | 'rules' | 'generic';
    onretry?: () => void | Promise<void>;
  } = $props();

  let busy = $state(false);
  const coreRecovery = $derived(['core_unavailable', 'connection_closed', 'timeout', 'internal'].includes(code ?? ''));
  const kernelRecovery = $derived(['feature_disabled', 'unsupported', 'tun_dns_system_auto_unsupported'].includes(code ?? ''));
  const dnsRecovery = $derived(context === 'tun' || code === 'tun_dns_hijack_requires_dns');

  async function restartCore() {
    if (busy || guiState.isCoreBusy || guiState.isSwitchingTun || guiState.isConnecting || guiState.isDisconnecting) return;
    busy = true;
    guiState.isStoppingCore = true;
    guiState.invalidateTunObservation();
    try {
      const result = await restartCoreProcess();
      if (result.tunRestoreError) warning(`内核已重启，但 TUN 恢复失败：${result.tunRestoreError.message}`);
      else success('内核已重新启动');
      await onretry?.();
    } catch {
      warning('内核重启失败，请查看日志或导出诊断材料');
    } finally {
      guiState.invalidateTunObservation();
      guiState.isStoppingCore = false;
      await Promise.allSettled([guiState.refreshTunStatus(), guiState.refreshConnectionStatus()]);
      busy = false;
    }
  }

  async function exportDiagnostics() {
    if (busy) return;
    busy = true;
    try {
      const result = await guiExportDiagnostics();
      success(`诊断材料已导出到 ${result.directory}`);
    } catch {
      warning('导出诊断材料失败，请前往“关于”页面重试');
    } finally {
      busy = false;
    }
  }
</script>

<div class="recovery-actions" aria-label="错误恢复操作">
  {#if onretry}
    <Button variant="outline" size="sm" onclick={() => onretry?.()} disabled={busy}><RefreshCw />重试</Button>
  {/if}
  {#if dnsRecovery}
    <Button variant="outline" size="sm" onclick={() => store.openSettings('dns')} disabled={busy}><Settings2 />DNS 设置</Button>
  {/if}
  {#if kernelRecovery}
    <Button variant="outline" size="sm" onclick={() => store.openSettings('core')} disabled={busy}><Settings2 />内核版本</Button>
  {/if}
  {#if context === 'rules'}
    <Button variant="outline" size="sm" onclick={() => (store.activeTab = 'rules')} disabled={busy}><Settings2 />管理规则</Button>
  {/if}
  {#if coreRecovery}
    <Button variant="outline" size="sm" onclick={restartCore} disabled={busy || guiState.isCoreBusy || guiState.isSwitchingTun || guiState.isConnecting || guiState.isDisconnecting}><RotateCw />重启内核</Button>
  {/if}
  <Button variant="ghost" size="sm" onclick={() => (store.activeTab = 'logs')} disabled={busy}><ScrollText />查看日志</Button>
  <Button variant="ghost" size="sm" onclick={exportDiagnostics} disabled={busy}><Download />导出诊断</Button>
</div>

<style>
  .recovery-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .recovery-actions :global(svg) { width: 13px; height: 13px; }
</style>
