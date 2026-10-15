<script lang="ts">
  import type { ToolJobsState } from './state.svelte';
  import { Button } from '$lib/components/ui/button';
  let { jobs }: { jobs: ToolJobsState } = $props();
  let expandedJobId = $state<number | null>(null);
  const labels: Record<string, string> = { dns_lookup: 'DNS 查询', dns_cache: 'DNS 缓存', fake_ip_lookup: 'Fake-IP 查询', fake_ip_clear: 'Fake-IP 清理', route_trace: '路由评估' };
  const states: Record<string, string> = { queued: '排队', running: '执行中', cancelling: '停止后等待在途结果', completed: '完成', failed: '失败', timed_out: '超时', cancelled: '取消', invalidated_by_config_change: '配置变化已失效', invalidated_by_core_restart: '内核变化已失效' };
</script>
{#if jobs.error}
  <div role="status" class="scope-notice"><span>{jobs.error}</span><Button variant="outline" onclick={() => { void jobs.init().then(() => jobs.refreshScope()).catch(() => {}); }}>重新核对</Button></div>
{/if}
{#if jobs.history().length}
  <details class="tool-history">
    <summary>任务历史（{jobs.history().length}）</summary>
    <p>历史只用于追溯，不代表当前网络状态，不会自动重试或触发后续请求。</p>
    {#each jobs.history() as job (job.id)}
      <details class="history-item" ontoggle={event => { if (event.currentTarget.open) expandedJobId = job.id; else if (expandedJobId === job.id) expandedJobId = null; }}>
        <summary>#{job.id} · {labels[job.kind] ?? job.kind} · {states[job.state] ?? job.state}</summary>
        <dl><dt>目标</dt><dd>{job.subject || '未提供'}</dd><dt>配置</dt><dd>{job.scope.profileId ?? '未绑定配置'}</dd><dt>配置版本</dt><dd>{job.scope.configRevision}</dd><dt>客户端内核代次</dt><dd>{job.scope.coreInstanceId}</dd><dt>记录时间</dt><dd>{new Date(job.updatedAtUnixMs).toLocaleString()}</dd></dl>
        {#if job.error}<p>错误：{job.error.code} · {job.error.message}</p>{/if}
        {#if expandedJobId === job.id && job.result !== undefined}<pre>{JSON.stringify(job.result, null, 2)}</pre>{:else if expandedJobId === job.id}<p>没有保留结果正文；不据此判断当前运行状态。</p>{/if}
      </details>
    {/each}
  </details>
{/if}
<style>
  .scope-notice { display:flex; gap:10px; align-items:center; justify-content:space-between; padding:12px; background:var(--muted); border-radius:9px; font-size:12px; }
  .tool-history { border:1px solid var(--border); border-radius:10px; padding:12px; margin-bottom:12px; font-size:12px; min-width:0; }
  summary { cursor:pointer; min-height:44px; display:list-item; align-content:center; overflow-wrap:anywhere; }
  p { color:var(--muted-foreground); margin:8px 0; overflow-wrap:anywhere; }
  .history-item { border-top:1px solid var(--border); }
  dl { display:grid; grid-template-columns:105px minmax(0,1fr); gap:6px; } dd { overflow-wrap:anywhere; }
  pre { white-space:pre-wrap; overflow-wrap:anywhere; max-height:260px; overflow:auto; }
  .scope-notice :global(button) { min-height:44px; flex-shrink:0; }
</style>
