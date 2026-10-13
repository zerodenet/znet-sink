<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { clearLogs, getAppErrorMessage } from '$lib/services/core';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  let target = $state<'logs' | 'history' | null>(null);
  let busy = $state(false);
  let status = $state<string | null>(null);
  let error = $state<string | null>(null);
  async function clear() {
    const selected = target;
    if (!selected || busy) return;
    target = null; busy = true; status = null; error = null;
    try {
      if (selected === 'logs') await clearLogs();
      else await invoke('gui_debug_clear', { scope: 'connection-history' });
      status = selected === 'logs' ? '本地日志清理已确认' : '本地完成连接历史清理已确认';
    } catch (cause) { error = getAppErrorMessage(cause, '清理结果未确认，请核对本地数据；不会自动重试。'); }
    finally { busy = false; }
  }
</script>
<section class="local-data">
  <h2>分别管理本地记录</h2>
  <p>清理不会删除配置、订阅、授权或结束活动连接。</p>
  <Button variant="outline" disabled={busy} onclick={() => target = 'logs'}>清理本地日志</Button>
  <Button variant="outline" disabled={busy} onclick={() => target = 'history'}>清理本地完成连接历史</Button>
  <h2>调试捕获</h2><p>独立捕获清理及移动系统保存/分享尚未适配。当前不使用包含日志等其他数据的桌面目录清理。</p>
  {#if status}<p role="status">{status}</p>{/if}{#if error}<p class="error" role="alert">{error}</p>{/if}
</section>
<Dialog.Root open={target !== null} onOpenChange={open => { if (!open) target = null; }}>
  <Dialog.Content><Dialog.Header><Dialog.Title>确认清理{target === 'logs' ? '本地日志' : '本地完成连接历史'}？</Dialog.Title><Dialog.Description>删除所选本地记录。配置、订阅、授权和活动连接不受影响。</Dialog.Description></Dialog.Header><Dialog.Footer><Button variant="outline" onclick={() => target = null}>取消</Button><Button variant="destructive" onclick={clear}>确认清理</Button></Dialog.Footer></Dialog.Content>
</Dialog.Root>
<style>
  .local-data { padding:16px 14px 26px; }
  h2 { font-size:1rem; font-weight:500; margin:16px 0 8px; }
  p { color:var(--muted-foreground); font-size:.85rem; line-height:1.6; }
  .local-data :global(button) { width:100%; margin-top:10px; }
  .error { color:var(--destructive); }
</style>
