<script lang="ts">
  import { RouteTraceState } from './state.svelte';
  import { routeActionLabel } from './result';
  import { onDestroy } from 'svelte';
  import { Clipboard, LoaderCircle, Network, XCircle } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import ToolJobHistory from '$lib/features/tool-jobs/History.svelte';
  import { getAppErrorMessage } from '$lib/services/core';
  import { cancelToolJob, listToolJobs, startToolJob, subscribeToolJobs, getToolScope, subscribeToolScope } from '$lib/features/tool-jobs/client';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  const route = new RouteTraceState({ start: startToolJob, list: listToolJobs, cancel: cancelToolJob,
    subscribe: subscribeToolJobs, scope: getToolScope, subscribeScope: subscribeToolScope, getAppErrorMessage });
  let copyFeedback = $state<string | null>(null);
  let copyEpoch = 0;
  async function copyResult() {
    if (!route.traceResult) return;
    const result = route.traceResult, epoch = route.jobs.epoch, copy = ++copyEpoch;
    try {
      await copyTextToClipboard(JSON.stringify(result, null, 2));
      if (copy === copyEpoch && epoch === route.jobs.epoch && route.traceResult === result && !route.jobs.disposed) copyFeedback = '已复制评估结果';
    } catch {
      if (copy === copyEpoch && epoch === route.jobs.epoch && route.traceResult === result && !route.jobs.disposed) copyFeedback = '复制未完成，请重试';
    }
  }
  $effect(() => { const epoch = route.jobs.epoch, result = route.traceResult; void epoch; void result; copyFeedback = null; });
  onDestroy(() => { copyEpoch++; route.dispose(); });
</script>
<ToolJobHistory jobs={route.jobs} />
{#if route.jobs.active('route_trace').length}
  <div class="job-strip" aria-label="路由评估活动任务">
    {#each route.jobs.active('route_trace') as job (job.id)}
      <p><LoaderCircle class="animate-spin" /><span>#{job.id} · {job.subject} · {job.state === 'queued' ? '排队中' : job.state === 'cancelling' ? '停止后等待在途结果' : '执行中'}</span>
        <Button variant="ghost" onclick={() => route.cancel(job.id)}><XCircle />停止</Button></p>
    {/each}
  </div>
{/if}
<section class="diag-tool" aria-label="路由评估">
  <h2>路由评估</h2>
  <p class="hint">评估当前配置的命中规则与路由目标；不是 ICMP 逐跳探测，不证明目标可达。内核可能按路由策略解析域名。</p>
  <div class="diag-form">
    <label>目标域名或 IP<Input aria-label="评估目标" placeholder="example.com" bind:value={route.traceTarget} disabled={route.traceLoading} onkeydown={e => e.key === 'Enter' && route.runTrace()} /></label>
    <label>端口<Input aria-label="评估端口" type="number" min="1" max="65535" step="1" bind:value={route.tracePort} disabled={route.traceLoading} /></label>
    <label>传输协议<FieldSelect aria-label="评估协议" bind:value={route.traceProtocol} options={[{value:'tcp',label:'TCP'},{value:'udp',label:'UDP'}]} disabled={route.traceLoading} /></label>
    <label>入站标签（可选）<Input aria-label="评估入站标签" placeholder="当前配置的入站标签" bind:value={route.traceInboundTag} disabled={route.traceLoading} /></label>
  </div>
  <Button onclick={() => route.runTrace()} disabled={!route.jobs.ready || route.traceLoading || !route.traceTarget.trim()}>
    {#if route.traceLoading}<LoaderCircle class="animate-spin" />{:else}<Network />{/if}{route.traceLoading ? '评估中…' : '评估'}
  </Button>
  {#if route.traceError}<p class="error" role="status">{route.traceError}</p>{/if}
  {#if route.traceResult}
    {@const result = route.traceResult}
    <dl class="route-facts" aria-label="当前路由评估结果">
      <dt>目标</dt><dd>{result.target}:{result.port} · {result.protocol.toUpperCase()}</dd>
      <dt>入站</dt><dd>{result.inbound_tag ?? '未提供'}</dd>
      <dt>有效模式</dt><dd>{{rule:'规则',direct:'直连',global:'全局'}[result.effective_mode]}</dd>
      <dt>路由动作</dt><dd>{routeActionLabel(result)}</dd>
      <dt>命中规则</dt><dd>{result.matched_rule ? `索引 ${result.matched_rule.index}（从 0 开始）` : '未命中具体规则（使用最终动作或模式决策）'}</dd>
      <dt>匹配条件</dt><dd>{result.matched_rule?.condition ?? '未提供'}</dd>
      <dt>解析及出口链</dt><dd>当前接口未提供解析候选、组选择链或最终叶子；不能从路由目标推断。</dd>
    </dl>
    <Button variant="outline" onclick={copyResult}><Clipboard />复制评估结果</Button>
    {#if copyFeedback}<p role="status">{copyFeedback}</p>{/if}
  {/if}
</section>
<style>
  .diag-tool { display:flex;flex-direction:column;gap:12px;min-width:0; }
  h2 { font-size:16px;font-weight:500; } .hint { font-size:12px;color:var(--muted-foreground);overflow-wrap:anywhere; }
  .diag-form { display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px; }
  label { display:flex;flex-direction:column;gap:6px;min-width:0;font-size:12px; }
  label:first-child,label:last-child { grid-column:1/-1; }
  .diag-tool :global(input),.diag-tool :global(button),.job-strip :global(button) { min-height:44px; }
  .route-facts { display:grid;grid-template-columns:88px minmax(0,1fr);gap:10px;font-size:13px; }
  dt { color:var(--muted-foreground); } dd { overflow-wrap:anywhere;min-width:0; }
  .error { font-size:12px;color:var(--destructive);overflow-wrap:anywhere; }
  .job-strip { padding:10px;background:var(--muted);border-radius:8px;margin-bottom:12px; }
  .job-strip p { display:flex;align-items:center;gap:8px;min-width:0;font-size:12px; }
  .job-strip span { flex:1;overflow-wrap:anywhere; } .job-strip :global(svg) { width:16px;height:16px;flex-shrink:0; }
</style>
