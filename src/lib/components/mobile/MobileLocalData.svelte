<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { decodeLocalData, storageAge, type LocalDataSummary } from '$lib/features/mobile/local-data';
  import { clearLogs, getAppErrorMessage } from '$lib/services/core';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  let target = $state<'logs' | 'history' | 'capture' | null>(null);
  let busy = $state(false);
  let status = $state<string | null>(null);
  let error = $state<string | null>(null);
  let summary = $state<LocalDataSummary | null>(null);
  let reading = $state(false);
  let stale = $state(false);
  let readError = $state(false);
  let generation = 0;
  let disposed = false;
  const names = { logs: '日志页记录', history: '本地完成连接历史', capture: '捕获帧文件' };
  async function refresh() {
    if (reading || disposed) return;
    const request = ++generation; reading = true; readError = false;
    try {
      const result = decodeLocalData(await invoke('gui_local_data_summary'));
      if (!disposed && request === generation) { summary = result; stale = false; }
    } catch {
      if (!disposed && request === generation) { readError = true; stale = summary !== null; }
    } finally { if (!disposed && request === generation) reading = false; }
  }
  onMount(() => { void refresh(); return () => { disposed = true; generation++; }; });
  async function clear() {
    const selected = target;
    if (!selected || busy) return;
    generation++; reading = false; stale = summary !== null;
    target = null; busy = true; status = null; error = null;
    try {
      if (selected === 'logs') await clearLogs();
      else await invoke('gui_debug_clear', { scope: selected === 'history' ? 'connection-history' : 'debug-capture' });
      status = selected === 'logs' ? '本地日志清理已确认' : selected === 'history' ? '本地完成连接历史清理已确认' : '捕获已停止，调试记录及捕获恢复账本清理已确认';
    } catch (cause) { error = getAppErrorMessage(cause, '清理结果未确认，请核对本地数据；不会自动重试。'); }
    finally { busy = false; if (!disposed) void refresh(); }
  }
</script>
<section class="local-data">
  <h2>分别管理本地记录</h2>
  <p>清理不会删除配置、订阅、授权或结束活动连接。</p>
  <Button variant="outline" disabled={busy || reading} onclick={refresh}>{reading ? '正在读取实际容量…' : '重新读取容量与保留预算'}</Button>
  <p>仅统计以下记录文件，不包含配置、订阅、恢复账本、内核日志文件或已导出的诊断材料。读取不执行清理；各范围分别采样。</p>
  {#if readError}<p role="alert">容量读取失败，未知值不会置零。可重新读取。{summary ? '上次采样仅供历史查看。' : ''}</p>{/if}
  {#if summary}
    <p>采样时间（Unix 毫秒）：{summary.sampledAtUnixMs}{stale ? ' · 已过期' : ''}</p>
    {#each summary.stores as item (item.kind)}
      <section class="storage-card" aria-label={`${names[item.kind]}容量`}>
        <h3>{names[item.kind]}</h3>
        <p>文件占用：{item.bytes === null ? '—（不可读取）' : `${item.bytes} 字节`} · 文件数：{item.files ?? '—'}</p>
        <p>字节预算：{item.byteLimit === null ? '未设置固定字节上限' : `${item.byteLimit} 字节`}；记录预算：{item.recordLimit ?? '未设置固定条数'}；保留时长：{storageAge(item.maxAgeMs)}</p>
        <p>{item.enforcement === 'append-and-read-rotation' ? '追加与读取时轮换，按条数、字节和时长淘汰。' : item.enforcement === 'segment-rotation' ? '按文件段字节预算轮换；不是单次捕获额度。' : '按后端追加轮换策略保留；条数不等于文件字节上限。'}</p>
      </section>
    {/each}
  {/if}
  <Button variant="outline" disabled={busy} onclick={() => target = 'logs'}>清理本地日志</Button>
  <Button variant="outline" disabled={busy} onclick={() => target = 'history'}>清理本地完成连接历史</Button>
  <h2>调试捕获</h2><p>先停止捕获并等待写入协调，再清理既有调试记录及捕获恢复账本。日志和完成连接历史保留；之后的新观测记录仍受存储预算约束。</p>
  <Button variant="outline" disabled={busy} onclick={() => target = 'capture'}>停止捕获并清理调试记录</Button>
  {#if status}<p role="status">{status}</p>{/if}{#if error}<p class="error" role="alert">{error}</p>{/if}
</section>
<Dialog.Root open={target !== null} onOpenChange={open => { if (!open) target = null; }}>
  <Dialog.Content><Dialog.Header><Dialog.Title>确认清理{target === 'logs' ? '本地日志' : target === 'history' ? '本地完成连接历史' : '调试捕获记录'}？</Dialog.Title><Dialog.Description>{target === 'capture' ? '停止本次捕获，协调在途写入，仅删除调试记录及捕获恢复账本，保留日志及完成连接历史。' : '删除所选本地记录。'}配置、订阅、授权和活动连接不受影响。</Dialog.Description></Dialog.Header><Dialog.Footer><Button variant="outline" onclick={() => target = null}>取消</Button><Button variant="destructive" onclick={clear}>确认清理</Button></Dialog.Footer></Dialog.Content>
</Dialog.Root>
<style>
  .local-data { padding:16px 14px 26px; }
  h2 { font-size:1rem; font-weight:500; margin:16px 0 8px; }
  p { color:var(--muted-foreground); font-size:.85rem; line-height:1.6; }
  .local-data :global(button) { width:100%; min-height:44px; height:auto; white-space:normal; margin-top:10px; }
  .error { color:var(--destructive); }
  .storage-card { margin-top:12px; padding:12px; border:1px solid var(--border); border-radius:10px; overflow-wrap:anywhere; }
  h3 { font-size:.9rem; font-weight:500; }
</style>
