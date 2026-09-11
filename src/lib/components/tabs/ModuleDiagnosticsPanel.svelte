<script lang="ts">
  import { onMount } from 'svelte';
  import { Clipboard, RefreshCcw } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { copyTextToClipboard } from '$lib/services/clipboard';
  import type { ModuleReport, ModuleStatus } from '$lib/features/diagnostics/model';
  let { load = () => import('$lib/services/module-diagnostics').then((service) => service.getModuleDiagnostics()) }: {load?: () => Promise<ModuleReport>} = $props();
  let report = $state<ModuleReport | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let feedback = $state('');
  let generation = 0;
  const labels: Record<ModuleStatus['state'], string> = {idle: '空闲／未加载', busy: '进行中', ready: '已读取', error: '异常', unavailable: '未知'};
  const totals = $derived([
    {label: '模块总数', count: report?.modules.length ?? 0, tone: 'all'},
    {label: '已读取', count: report?.modules.filter(m => m.state === 'ready').length ?? 0, tone: 'ready'},
    {label: '进行中', count: report?.modules.filter(m => m.state === 'busy').length ?? 0, tone: 'busy'},
    {label: '需要关注', count: report?.modules.filter(m => m.state === 'error' || m.state === 'unavailable').length ?? 0, tone: 'error'},
  ]);
  async function refresh() {
    if (loading) return;
    const current = generation;
    loading = true; error = null;
    try { const snapshot = await load(); if (current === generation) report = snapshot; }
    catch (cause) { if (current === generation) error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (current === generation) loading = false; }
  }
  async function copy() {
    if (!report) return;
    try { await copyTextToClipboard(JSON.stringify({schemaId: 'znet.modules.v1', ...report}, null, 2)); feedback = '已复制模块诊断'; }
    catch { feedback = '复制失败，请重试'; }
  }
  onMount(() => { void refresh(); return () => { generation++; }; });
</script>
<section class="module-panel" aria-label="模块状态">
  <div class="module-toolbar">
    <div><h2>模块状态</h2><p>读取当前状态不会启停内核、更改配置或触发网络探测。</p></div>
    <div class="actions"><Button variant="outline" size="sm" onclick={copy} disabled={!report}><Clipboard />复制诊断</Button><Button size="sm" onclick={refresh} disabled={loading}><RefreshCcw />{loading ? '读取中…' : '刷新'}</Button></div>
  </div>
  {#if error}<p role="alert">读取失败：{error}{report ? '；仍显示上次快照。' : ''}</p>{/if}
  {#if feedback}<p role="status">{feedback}</p>{/if}
  {#if report}
    <div class="module-totals" aria-label="模块状态汇总">
      {#each totals as total}
        <div class="total" data-tone={total.tone}><span>{total.label}</span><strong>{total.count}</strong></div>
      {/each}
    </div>
    <div class="snapshot-meta"><p class="stamp">采集于 {new Date(report.collectedAt).toLocaleString()}</p><p class="stamp">历史错误仅供排查，以当前状态为准</p></div>
    <div class="module-grid">
      {#each report.modules as module (module.id)}
        <article class="module-card" data-state={module.state}>
          <header><h3>{module.title}</h3><span class="state-badge" data-state={module.state}><i aria-hidden="true"></i>{labels[module.state]}</span></header>
          <p class="module-summary">{module.summary}</p>
          {#if module.updatedAt}<p class="stamp">观测时间：{new Date(module.updatedAt).toLocaleString()}</p>{/if}
          {#if module.facts.length}<dl>{#each module.facts as fact}<div class:long-value={fact.value.length > 45}><dt>{fact.label}</dt><dd>{fact.value}</dd></div>{/each}</dl>{/if}
          {#if module.error}<details class="module-error" open={module.state === 'error' || module.state === 'unavailable'}><summary>{module.state === 'error' || module.state === 'unavailable' ? '错误详情' : '历史错误'}</summary><p>{module.error}</p></details>{/if}
        </article>
      {/each}
    </div>
  {:else if loading}<p role="status">正在读取模块状态…</p>{/if}
</section>
<style>
  .module-panel { overflow: auto; padding: 8px 4px 24px; min-height: 0; width: 100%; max-width: 1320px; margin-inline: auto; }
  .module-toolbar, .actions, header { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .module-toolbar { flex-wrap: wrap; margin-bottom: 20px; } .actions { flex-wrap: wrap; }
  h2, h3, p { margin: 0; } h2 { font-size: 18px; font-weight: 600; } h3 { font-size: 14px; font-weight: 600; }
  p, dt, dd { font-size: 12px; line-height: 1.6; overflow-wrap: anywhere; } p { margin-top: 6px; }
  .stamp, dt, .module-toolbar p { color: var(--muted-foreground); }
  .module-totals { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); border: 1px solid var(--border); border-radius: 12px; background: var(--card); }
  .total { display: flex; flex-direction: column; gap: 6px; padding: 14px 18px; }
  .total + .total { border-left: 1px solid var(--border); }
  .total span { font-size: 12px; color: var(--muted-foreground); }
  .total strong { font-size: 24px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .snapshot-meta { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 4px 16px; margin-top: 12px; }
  .module-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 340px), 1fr)); align-items: start; gap: 16px; margin-top: 16px; }
  .module-card { border: 1px solid var(--border); background: var(--card); border-radius: 12px; padding: 18px; min-width: 0; }
  .module-card[data-state="unavailable"], .module-card[data-state="error"] { border-color: color-mix(in srgb, var(--destructive) 35%, var(--border)); }
  header { align-items: flex-start; }
  .state-badge { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; background: var(--muted); color: var(--muted-foreground); border-radius: 999px; padding: 3px 8px; white-space: nowrap; }
  .state-badge i { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }
  .state-badge[data-state="ready"] { color: var(--foreground); }
  .state-badge[data-state="busy"] { color: var(--primary); background: color-mix(in srgb, var(--primary) 10%, var(--card)); }
  .state-badge[data-state="error"], .state-badge[data-state="unavailable"] { color: var(--destructive); background: color-mix(in srgb, var(--destructive) 10%, var(--card)); }
  .module-summary { margin-top: 14px; }
  dl { margin: 14px 0 0; padding-top: 10px; border-top: 1px solid var(--border); }
  dl div { display: grid; grid-template-columns: minmax(110px, 1fr) 1.6fr; gap: 6px 14px; padding: 4px 0; }
  dl .long-value { grid-template-columns: 1fr; }
  dd { margin: 0; font-variant-numeric: tabular-nums; }
  .module-error { margin-top: 14px; border-top: 1px solid var(--border); padding-top: 10px; font-size: 12px; color: var(--muted-foreground); }
  .module-error summary { cursor: pointer; }
  .module-card[data-state="unavailable"] .module-error, .module-card[data-state="error"] .module-error { color: var(--destructive); }
  @media (max-width: 560px) {
    .total { padding: 12px 8px; } .total strong { font-size: 20px; }
    .module-card { padding: 14px; } .module-grid { gap: 12px; }
    dl div { grid-template-columns: 1fr; }
  }
</style>
