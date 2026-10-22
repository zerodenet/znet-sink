<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { readAndroidPluginJobs, jobReason, type AndroidPluginJobs } from '$lib/services/android-plugin-jobs';
  let snapshot = $state<AndroidPluginJobs | null>(null);
  let loading = $state(false);
  let error = $state('');
  let alive = true;
  const states: Record<string, string> = { running: '执行中', completed: '已结束', deferred: '本次推迟', failed: '未完成', stopped: '已停止，待核对', 'result-unknown': '结果未确认' };
  function time(value?: string | null): string {
    if (!value) return '—';
    const date = new Date(Number(value));
    return Number.isFinite(date.getTime()) ? date.toLocaleString() : '—';
  }
  async function refresh() {
    if (loading) return;
    loading = true; error = '';
    try { const value = await readAndroidPluginJobs(); if (alive) snapshot = value; }
    catch { if (alive) error = '系统作业记录读取失败，请重试；保留已有任务记录。'; }
    finally { if (alive) loading = false; }
  }
  onMount(() => { alive = true; void refresh(); return () => { alive = false; }; });
</script>
<details class="plugin-authorization-notice">
  <summary>Android 后台任务</summary>
  <p>后台最短系统周期为 15 分钟。锁屏、省电、网络及厂商限制可能继续推迟；强行停止后需重新打开应用。任务完成仅指插件执行与材料保存，配置仍需核对应用，未确认 VPN 接管。</p>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if snapshot}
    <p>系统登记：{snapshot.scheduling?.state === 'scheduled' ? '已登记' : '尚未确认'} · {jobReason(snapshot.scheduling?.reason)}</p>
    {#if snapshot.lastRun}
      <p>最近实际执行：{time(snapshot.lastRun.startedAtUnixMs)} · {states[snapshot.lastRun.state] ?? '未知'}</p>
      <p>完成 {snapshot.lastRun.completed ?? '—'} · 失败 {snapshot.lastRun.failed ?? '—'} · 未确认 {snapshot.lastRun.unconfirmed ?? '—'}</p>
      <p>{jobReason(snapshot.lastRun.reason)}</p>
    {:else}<p>尚无系统后台执行记录。前台任务与后台执行分别记录。</p>{/if}
  {:else if loading}<p role="status">正在读取系统记录…</p>{/if}
  <Button variant="outline" disabled={loading} onclick={refresh}>重新核对后台记录</Button>
</details>
