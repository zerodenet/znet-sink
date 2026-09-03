<script lang="ts">
  import { open as openFile, save as saveFile } from '@tauri-apps/plugin-dialog';
  import { Download, Upload } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { exportClientKernelSettings, importClientKernelSettings } from '$lib/services/core';
  import { success, warning } from '$lib/services/toast.svelte';

  let busy = $state(false);
  let status = $state('');

  async function exportSettings() {
    const selected = await saveFile({
      title: '导出客户端配置',
      defaultPath: 'znet-kernel-settings.json',
      filters: [{ name: 'ZNet 客户端配置', extensions: ['json'] }],
    });
    if (!selected) return;

    busy = true;
    status = '';
    try {
      await exportClientKernelSettings(selected);
      status = '配置已导出';
      success(status);
    } catch (error) {
      status = error instanceof Error ? error.message : '导出失败';
      warning(status);
    } finally {
      busy = false;
    }
  }

  async function importSettings() {
    const selected = await openFile({
      title: '导入客户端配置',
      multiple: false,
      directory: false,
      filters: [{ name: 'ZNet 客户端配置', extensions: ['json'] }],
    });
    if (typeof selected !== 'string') return;

    busy = true;
    status = '';
    try {
      await importClientKernelSettings(selected);
      status = '配置已导入';
      success(status);
    } catch (error) {
      status = error instanceof Error ? error.message : '导入失败';
      warning(status);
    } finally {
      busy = false;
    }
  }
</script>

<section class="transfer-card">
  <div>
    <strong>内核配置迁移</strong>
    <span>导入或导出 DNS、TUN 和客户端运行偏好。</span>
    {#if status}<small>{status}</small>{/if}
  </div>
  <div class="transfer-actions">
    <Button variant="outline" size="sm" onclick={importSettings} disabled={busy}>
      <Upload class="h-3.5 w-3.5" />导入
    </Button>
    <Button variant="outline" size="sm" onclick={exportSettings} disabled={busy}>
      <Download class="h-3.5 w-3.5" />导出
    </Button>
  </div>
</section>

<style>
  .transfer-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-top: 4px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
  }

  .transfer-card > div:first-child {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  strong { font-size: 12px; }
  span, small { color: var(--muted-foreground); font-size: 10.5px; line-height: 1.45; }
  .transfer-actions { display: flex; flex: none; gap: 6px; }

  @media (max-width: 720px) {
    .transfer-card { align-items: stretch; flex-direction: column; }
  }
</style>
